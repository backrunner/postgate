//! Response body wrapper that streams data to the client while capturing a
//! copy for the UI / persistence layer, emitting the `Completed` event once
//! the stream ends.
//!
//! The old code path collected the entire upstream body before forwarding
//! anything to the client, so browser-visible TTFB was `upstream_ttfb +
//! full_body_download`. Whistle streams by default — this module brings
//! PostGate in line when no matched rule rewrites the body.

use crate::proxy::body::{BodyStorage, CapturedBody, MAX_BODY_SIZE};
use crate::rules::ResponseModification;
use crate::state::{
    AppState, CapturedRequestData, CapturedRequestEvent, RequestEventType,
};
use bytes::{Bytes, BytesMut};
use hyper::body::{Body, Frame, Incoming, SizeHint};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Apply a `ResponseModification` to an upstream response header map. This
/// centralizes:
///
/// * `headers_to_remove` — `resHeaders://-X-Foo` style removals.
/// * `cookies` — `Set-Cookie` entries produced by `resCookies://` actions.
/// * Body-was-replaced hygiene — strips stale `content-encoding` /
///   `transfer-encoding` / `content-length` when the bytes no longer match
///   what upstream sent, otherwise the browser trips on length mismatch
///   (`ERR_EMPTY_RESPONSE` / decoding failure).
///
/// `new_body_len` is `Some(n)` in the buffering path where we replaced the
/// body; `None` in the streaming path where the upstream body flows through
/// unchanged.
pub fn finalize_response_headers(
    upstream_headers: &HashMap<String, String>,
    modification: &ResponseModification,
    new_body_len: Option<usize>,
) -> HashMap<String, String> {
    // Start from the upstream headers but layer the modification's header
    // overrides on top. The applicator returns `modification.headers` as a
    // fully merged set (it starts from the upstream headers too), so we use
    // it directly.
    let mut final_headers = modification.headers.clone();

    // Drop explicitly removed headers.
    for name in &modification.headers_to_remove {
        final_headers.remove(&name.to_lowercase());
    }

    // Appending set-cookie from `resCookies://` actions. HTTP allows multiple
    // Set-Cookie headers; our flat header map keys on lowercase name so we
    // fold additional values with `\r\nset-cookie: ` — hyper's HeaderMap
    // accepts multi-value headers when we `append`, but callers that
    // serialize through this flat map need a single string. We join with
    // `, ` which works for RFC-compliant readers; anything sensitive to
    // multiple raw Set-Cookie lines should use hyper's HeaderMap directly.
    if !modification.cookies.is_empty() {
        let existing = final_headers.remove("set-cookie");
        let mut combined = existing.unwrap_or_default();
        for cookie in &modification.cookies {
            if !combined.is_empty() {
                combined.push_str(", ");
            }
            combined.push_str(cookie);
        }
        final_headers.insert("set-cookie".to_string(), combined);
    }

    // If the body was replaced in the buffering path, the upstream's
    // length/encoding headers are stale.
    if let Some(len) = new_body_len {
        // Only override content-length if the body actually changed. Callers
        // pass the final length; if they only want to emit streaming we
        // pass None.
        let upstream_len = upstream_headers
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok());
        if upstream_len != Some(len) {
            final_headers.remove("content-encoding");
            final_headers.remove("transfer-encoding");
            final_headers.insert("content-length".to_string(), len.to_string());
        }
    }

    final_headers
}

/// Metadata carried alongside the streaming body so we can emit a proper
/// `Completed` event when the stream ends.
#[derive(Clone)]
pub struct PassthroughMeta {
    pub request_id: String,
    pub timestamp: i64,
    pub method: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub request_headers: HashMap<String, String>,
    pub response_status: u16,
    pub response_headers: HashMap<String, String>,
    pub matched_rules: Vec<String>,
    pub protocol: String,
    pub content_type: Option<String>,
    pub request_size: u64,
    pub start_time: std::time::Instant,
    pub persistence_enabled: bool,
    pub tls_version: Option<String>,
    /// Whether whistle `disable://capture` is NOT set, i.e. the UI / storage
    /// should still receive the Completed event. When false the streaming
    /// body wrapper proxies bytes normally but doesn't emit.
    pub capture: bool,
}

pub struct PassthroughCapturingBody {
    inner: Incoming,
    meta: PassthroughMeta,
    app_state: Arc<AppState>,
    body_storage: Arc<BodyStorage>,
    collected: BytesMut,
    total_bytes: u64,
    truncated: bool,
    ended: bool,
}

impl PassthroughCapturingBody {
    pub fn new(
        inner: Incoming,
        meta: PassthroughMeta,
        app_state: Arc<AppState>,
        body_storage: Arc<BodyStorage>,
    ) -> Self {
        Self {
            inner,
            meta,
            app_state,
            body_storage,
            collected: BytesMut::new(),
            total_bytes: 0,
            truncated: false,
            ended: false,
        }
    }

    fn finish(&mut self, emit: bool) {
        if self.ended {
            return;
        }
        self.ended = true;
        if !emit {
            return;
        }

        let collected = std::mem::take(&mut self.collected).freeze();
        let duration = self.meta.start_time.elapsed().as_millis() as u64;
        let response_size = self.total_bytes;
        let truncated = self.truncated;

        // Storage is now DashMap-backed so store_response_body is
        // essentially synchronous; still avoid holding the body poll path
        // longer than needed by spawning.
        let stored = CapturedBody {
            data: collected.clone(),
            size: collected.len(),
            truncated,
        };
        let body_storage = self.body_storage.clone();
        let request_id = self.meta.request_id.clone();
        tokio::spawn(async move {
            body_storage.store_response_body(&request_id, stored).await;
        });
        // Only persist & emit when capture is enabled — whistle
        // `disable://capture` should leave no trace.
        if !self.meta.capture {
            return;
        }
        if self.meta.persistence_enabled {
            self.app_state
                .persist_body(self.meta.request_id.clone(), collected.clone(), false);
        }

        self.app_state.emit_request_event(&CapturedRequestEvent {
            id: self.meta.request_id.clone(),
            event_type: RequestEventType::Completed,
            data: CapturedRequestData {
                id: self.meta.request_id.clone(),
                timestamp: self.meta.timestamp,
                method: self.meta.method.clone(),
                url: self.meta.url.clone(),
                host: self.meta.host.clone(),
                path: self.meta.path.clone(),
                request_headers: Some(self.meta.request_headers.clone()),
                response_status: Some(self.meta.response_status),
                response_headers: Some(self.meta.response_headers.clone()),
                duration_ms: Some(duration),
                matched_rules: self.meta.matched_rules.clone(),
                protocol: self.meta.protocol.clone(),
                content_type: self.meta.content_type.clone(),
                request_size: self.meta.request_size,
                response_size: Some(response_size),
                tls_version: self.meta.tls_version.clone(),
                ..Default::default()
            },
        });
    }
}

impl Body for PassthroughCapturingBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        let inner = Pin::new(&mut self.inner);

        match inner.poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    self.total_bytes += data.len() as u64;
                    // Cap how much we actually buffer for the UI; bytes flow
                    // to the client regardless.
                    if self.collected.len() < MAX_BODY_SIZE {
                        let remaining = MAX_BODY_SIZE - self.collected.len();
                        if data.len() <= remaining {
                            self.collected.extend_from_slice(data);
                        } else {
                            self.collected.extend_from_slice(&data[..remaining]);
                            self.truncated = true;
                        }
                    } else {
                        self.truncated = true;
                    }
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(e))) => {
                self.finish(false);
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                self.finish(true);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for PassthroughCapturingBody {
    fn drop(&mut self) {
        if !self.ended {
            self.finish(true);
        }
    }
}
