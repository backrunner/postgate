use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use crate::proxy::forward::ForwardTarget;
use crate::proxy::handler::ProxyContext;
use crate::proxy::upstream::forward_collect;
use crate::rules::{
    apply_request_rules_with_values, apply_response_rules_with_values, ResolveCtx,
};
use crate::state::{CapturedRequestData, CapturedRequestEvent, RequestEventType};
use crate::values::RequestCtx;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use super::tls::{tls_version_string, TlsAcceptor};

/// Handle a tunneled HTTPS connection with MITM
/// Takes a TokioIo-wrapped stream for hyper interoperability
pub async fn tunnel_connection<S>(
    upgraded: TokioIo<S>,
    acceptor: TlsAcceptor,
    host: &str,
    port: u16,
    _ca: Arc<CertificateAuthority>,
    ctx: Arc<ProxyContext>,
) -> Result<()>
where
    TokioIo<S>: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // Accept TLS from client - TokioIo<S> implements AsyncRead/AsyncWrite
    let tls_stream = acceptor.accept(upgraded).await?;

    // Get the negotiated TLS version and ALPN from the client connection
    let (tls_version, alpn) = {
        let (_, server_conn) = tls_stream.get_ref();
        let version = tls_version_string(server_conn.protocol_version());
        let alpn = server_conn.alpn_protocol().map(|p| p.to_vec());
        (version, alpn)
    };

    let host = host.to_string();

    // Route to HTTP/2 or HTTP/1.1 based on ALPN
    if super::http2::should_use_http2(alpn.as_deref()) {
        super::http2::handle_http2_connection(tls_stream, host, port, ctx, tls_version).await
    } else {
        let io = TokioIo::new(tls_stream);
        let ctx_clone = ctx.clone();

        // Handle HTTP over TLS
        http1::Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .serve_connection(
                io,
                service_fn(move |req| {
                    let host = host.clone();
                    let ctx = ctx_clone.clone();
                    let tls_ver = tls_version.clone();
                    async move { handle_https_request(req, &host, port, ctx, &tls_ver).await }
                }),
            )
            .await
            .map_err(|e| PostGateError::Proxy(format!("HTTPS tunnel error: {}", e)))
    }
}

/// Handle an HTTPS request after TLS termination
async fn handle_https_request(
    req: Request<Incoming>,
    host: &str,
    port: u16,
    ctx: Arc<ProxyContext>,
    tls_version: &str,
) -> std::result::Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let request_id = Uuid::new_v4().to_string();
    let start_time = std::time::Instant::now();
    let timestamp = chrono::Utc::now().timestamp_millis();

    let method = req.method().clone();
    let method_str = method.to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());
    // Build URL - omit default port (443 for HTTPS) for cleaner display
    let url = if port == 443 {
        format!("https://{}{}", host, path)
    } else {
        format!("https://{}:{}{}", host, port, path)
    };

    // Extract headers
    let request_headers: HashMap<String, String> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let content_type = request_headers.get("content-type").cloned();

    // Emit started event
    ctx.app_state.emit_request_event(&CapturedRequestEvent {
        id: request_id.clone(),
        event_type: RequestEventType::Started,
        data: CapturedRequestData {
            id: request_id.clone(),
            timestamp,
            method: method_str.clone(),
            url: url.clone(),
            host: host.to_string(),
            path: path.clone(),
            request_headers: Some(request_headers.clone()),
            protocol: "https".to_string(),
            content_type: content_type.clone(),
            tls_version: Some(tls_version.to_string()),
            ..Default::default()
        },
    });

    // Match rules - now returns MatchedRule with remaining_path
    let matched_rules = ctx.rule_engine.match_request(&method_str, host, &path, "https", port, &request_headers);
    let matched_rule_ids: Vec<String> = matched_rules.iter().map(|r| r.rule.raw_line.clone()).collect();

    // Collect request body
    let (parts, body) = req.into_parts();
    let request_body = match collect_body(body, MAX_BODY_SIZE).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to collect request body: {}", e);
            return Ok(Response::builder()
                .status(502)
                .body(Full::new(Bytes::from(format!("Error: {}", e))).map_err(|_| unreachable!()).boxed())
                .unwrap());
        }
    };

    let request_size = request_body.size as u64;
    ctx.body_storage.store_request_body(&request_id, request_body.clone()).await;
    ctx.app_state.persist_body(request_id.clone(), request_body.data.clone(), true);

    // Build resolver context for whistle `{name}` references.
    let _ = ctx.app_state.ensure_values_loaded().await;
    let query_map = super::handler::tunnel_value_helpers::query_map(&url);
    let cookie_map = super::handler::tunnel_value_helpers::cookie_map(&request_headers);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let values_ctx = RequestCtx {
        url: &url,
        method: &method_str,
        client_ip: "",
        req_headers: &request_headers,
        query: &query_map,
        req_cookies: &cookie_map,
        now_ms,
    };
    let resolve_ctx = ResolveCtx {
        store: Some(&ctx.app_state.values_store),
        ctx: Some(&values_ctx),
    };

    // Apply request rules to get modifications
    let request_modification = apply_request_rules_with_values(
        &matched_rules,
        &url,
        &method_str,
        &request_headers,
        Some(&request_body.data),
        &resolve_ctx,
    );

    // Handle short-circuit responses (e.g., redirect, file, statusCode)
    if let Some(short_circuit) = request_modification.short_circuit {
        let duration = start_time.elapsed().as_millis() as u64;
        
        let response_body = CapturedBody {
            data: short_circuit.body.clone(),
            size: short_circuit.body.len(),
            truncated: false,
        };
        ctx.body_storage.store_response_body(&request_id, response_body.clone()).await;
        ctx.app_state.persist_body(request_id.clone(), response_body.data.clone(), false);

        ctx.app_state.emit_request_event(&CapturedRequestEvent {
            id: request_id.clone(),
            event_type: RequestEventType::Completed,
            data: CapturedRequestData {
                id: request_id,
                timestamp,
                method: method_str,
                url,
                host: host.to_string(),
                path,
                request_headers: Some(request_headers),
                response_status: Some(short_circuit.status),
                response_headers: Some(short_circuit.headers.clone()),
                duration_ms: Some(duration),
                matched_rules: matched_rule_ids,
                protocol: "https".to_string(),
                content_type: short_circuit.headers.get("content-type").cloned(),
                request_size,
                response_size: Some(short_circuit.body.len() as u64),
                tls_version: Some(tls_version.to_string()),
                ..Default::default()
            },
        });

        let mut builder = Response::builder().status(short_circuit.status);
        let mut sc_headers = short_circuit.headers.clone();
        sc_headers.remove("content-encoding");
        sc_headers.remove("transfer-encoding");
        sc_headers.insert(
            "content-length".to_string(),
            short_circuit.body.len().to_string(),
        );
        for (k, v) in &sc_headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        return Ok(builder
            .body(Full::new(short_circuit.body).map_err(|_| unreachable!()).boxed())
            .unwrap());
    }

    // Apply request delay if specified
    if let Some(delay_ms) = request_modification.delay_ms {
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
    }

    // Decide whether to buffer the response body. Streaming is the common
    // case (no resBody://, no htmlAppend://, no debug, no plugin) and matches
    // whistle's default behaviour — TTFB tracks upstream directly.
    let buffer_response_body = crate::rules::rules_require_response_body(&matched_rules);

    // Resolve the absolute upstream URL based on target_host rewrites.
    let absolute_url: std::result::Result<String, PostGateError> =
        if let Some(target_host) = &request_modification.target_host {
            let remaining_path = request_modification.remaining_path.as_deref().unwrap_or("");
            match ForwardTarget::parse(target_host, remaining_path, "https") {
                Ok(target) => {
                    tracing::debug!(
                        "Forwarding {} to {} (remaining: {})",
                        url,
                        target.build_url(),
                        remaining_path
                    );
                    Ok(target.build_url())
                }
                Err(e) => Err(e),
            }
        } else {
            let original_path = parts
                .uri
                .path_and_query()
                .map(|pq| pq.to_string())
                .unwrap_or_else(|| "/".to_string());
            let authority = if port == 443 {
                host.to_string()
            } else {
                format!("{}:{}", host, port)
            };
            Ok(format!("https://{}{}", authority, original_path))
        };

    let body_to_send = request_modification.body.unwrap_or(request_body.data.clone());

    // --- Streaming path ---------------------------------------------------
    if !buffer_response_body {
        let stream_result = match absolute_url {
            Ok(target_url) => {
                super::upstream::forward_stream(
                    &ctx.upstream_client,
                    method.clone(),
                    &target_url,
                    &request_modification.headers,
                    body_to_send,
                )
                .await
            }
            Err(e) => Err(e),
        };

        return match stream_result {
            Ok((parts, body)) => {
                let status = parts.status.as_u16();
                let upstream_headers: HashMap<String, String> = parts
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
                    .collect();

                let response_content_type = upstream_headers.get("content-type").cloned();
                let response_modification = apply_response_rules_with_values(
                    &matched_rules,
                    &url,
                    &method_str,
                    &request_headers,
                    &upstream_headers,
                    None,
                    response_content_type.as_deref(),
                    &resolve_ctx,
                );
                let final_status = response_modification.status_code.unwrap_or(status);
                // Unified header finalization: applies `headers_to_remove`,
                // fold in `resCookies://` set-cookies, strip stale
                // content-encoding/transfer-encoding/content-length when the
                // body was replaced (body is unchanged in this branch).
                let final_headers = super::passthrough::finalize_response_headers(
                    &upstream_headers,
                    &response_modification,
                    None,
                );

                // resDelay:// — delay before first byte.
                if let Some(delay_ms) = response_modification.delay_ms {
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }

                let wrapped = super::passthrough::PassthroughCapturingBody::new(
                    body,
                    super::passthrough::PassthroughMeta {
                        request_id: request_id.clone(),
                        timestamp,
                        method: method_str.clone(),
                        url: url.clone(),
                        host: host.to_string(),
                        path: path.clone(),
                        request_headers: request_headers.clone(),
                        response_status: final_status,
                        response_headers: final_headers.clone(),
                        matched_rules: matched_rule_ids.clone(),
                        protocol: "https".to_string(),
                        content_type: response_content_type,
                        request_size,
                        start_time,
                        persistence_enabled: ctx.app_state.is_persistence_enabled(),
                        tls_version: Some(tls_version.to_string()),
                    },
                    ctx.app_state.clone(),
                    ctx.body_storage.clone(),
                );

                let mut builder = Response::builder().status(final_status);
                for (k, v) in &final_headers {
                    if let (Ok(name), Ok(value)) = (
                        hyper::header::HeaderName::from_bytes(k.as_bytes()),
                        hyper::header::HeaderValue::from_str(v),
                    ) {
                        builder = builder.header(name, value);
                    }
                }
                Ok(builder.body(wrapped.boxed()).unwrap())
            }
            Err(e) => {
                let duration = start_time.elapsed().as_millis() as u64;
                tracing::error!("Forward error: {}", e);

                ctx.app_state.emit_request_event(&CapturedRequestEvent {
                    id: request_id.clone(),
                    event_type: RequestEventType::Error,
                    data: CapturedRequestData {
                        id: request_id,
                        timestamp,
                        method: method_str,
                        url,
                        host: host.to_string(),
                        path,
                        request_headers: Some(request_headers),
                        duration_ms: Some(duration),
                        protocol: "https".to_string(),
                        request_size,
                        error: Some(e.to_string()),
                        tls_version: Some(tls_version.to_string()),
                        ..Default::default()
                    },
                });

                Ok(Response::builder()
                    .status(502)
                    .body(Full::new(Bytes::from(format!("Proxy error: {}", e))).map_err(|_| unreachable!()).boxed())
                    .unwrap())
            }
        };
    }

    // --- Buffering path (body-modifying rules) ---------------------------
    let forward_result = match absolute_url {
        Ok(url) => {
            forward_collect(
                &ctx.upstream_client,
                method.clone(),
                &url,
                &request_modification.headers,
                body_to_send,
            )
            .await
        }
        Err(e) => Err(e),
    };

    match forward_result {
        Ok((resp, response_body)) => {
            let status = resp.status().as_u16();
            let upstream_headers: HashMap<String, String> = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
                .collect();

            let response_content_type = upstream_headers.get("content-type").cloned();

            // Apply response rules
            let response_modification = apply_response_rules_with_values(
                &matched_rules,
                &url,
                &method_str,
                &request_headers,
                &upstream_headers,
                Some(&response_body.data),
                response_content_type.as_deref(),
                &resolve_ctx,
            );

            // resDelay:// — applies before sending first byte back.
            if let Some(delay_ms) = response_modification.delay_ms {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            let mut final_body = response_modification.body.clone().unwrap_or(response_body.data.clone());

            // debug:// — inject CDP bridge into HTML responses.
            if response_modification.inject_debug {
                let injector = crate::debug::ScriptInjector::new(9229);
                if let Ok(html) = String::from_utf8(final_body.to_vec()) {
                    if !crate::debug::ScriptInjector::is_already_injected(&html) {
                        final_body = Bytes::from(injector.inject_into_html(&html));
                    }
                }
            }

            // resSpeed:// — throttle the buffered body before writing out.
            if let Some(speed_kbps) = response_modification.speed_kbps {
                final_body = super::throttle::apply_throttle(final_body, Some(speed_kbps)).await;
            }

            let body_was_modified = final_body != response_body.data;
            let final_status = response_modification.status_code.unwrap_or(status);
            let final_headers = super::passthrough::finalize_response_headers(
                &upstream_headers,
                &response_modification,
                if body_was_modified { Some(final_body.len()) } else { None },
            );

            let response_size = final_body.len() as u64;
            let duration = start_time.elapsed().as_millis() as u64;

            let final_body_captured = CapturedBody {
                data: final_body.clone(),
                size: final_body.len(),
                truncated: false,
            };
            ctx.body_storage.store_response_body(&request_id, final_body_captured).await;
            ctx.app_state.persist_body(request_id.clone(), final_body.clone(), false);

            // Emit completed event
            ctx.app_state.emit_request_event(&CapturedRequestEvent {
                id: request_id.clone(),
                event_type: RequestEventType::Completed,
                data: CapturedRequestData {
                    id: request_id,
                    timestamp,
                    method: method_str,
                    url,
                    host: host.to_string(),
                    path,
                    request_headers: Some(request_headers),
                    response_status: Some(final_status),
                    response_headers: Some(final_headers.clone()),
                    duration_ms: Some(duration),
                    matched_rules: matched_rule_ids,
                    protocol: "https".to_string(),
                    content_type: response_content_type,
                    request_size,
                    response_size: Some(response_size),
                    tls_version: Some(tls_version.to_string()),
                    ..Default::default()
                },
            });

            // Build final response
            let mut builder = Response::builder().status(final_status);
            for (k, v) in &final_headers {
                if let (Ok(name), Ok(value)) = (
                    hyper::header::HeaderName::from_bytes(k.as_bytes()),
                    hyper::header::HeaderValue::from_str(v),
                ) {
                    builder = builder.header(name, value);
                }
            }
            
            Ok(builder
                .body(Full::new(final_body).map_err(|_| unreachable!()).boxed())
                .unwrap())
        }
        Err(e) => {
            let duration = start_time.elapsed().as_millis() as u64;
            tracing::error!("Forward error: {}", e);

            ctx.app_state.emit_request_event(&CapturedRequestEvent {
                id: request_id.clone(),
                event_type: RequestEventType::Error,
                data: CapturedRequestData {
                    id: request_id,
                    timestamp,
                    method: method_str,
                    url,
                    host: host.to_string(),
                    path,
                    request_headers: Some(request_headers),
                    duration_ms: Some(duration),
                    protocol: "https".to_string(),
                    request_size,
                    error: Some(e.to_string()),
                    ..Default::default()
                },
            });

            Ok(Response::builder()
                .status(502)
                .body(Full::new(Bytes::from(format!("Proxy error: {}", e))).map_err(|_| unreachable!()).boxed())
                .unwrap())
        }
    }
}

