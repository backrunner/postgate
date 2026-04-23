//! Shared upstream HTTP/HTTPS client.
//!
//! Rebuilding a hyper `Client` per request throws away its connection pool and
//! forces a fresh TCP + TLS handshake every time. A single process-wide client
//! can amortize this across all proxied requests and speeds up typical page
//! loads by an order of magnitude.
//!
//! The client uses `hyper-rustls` with ALPN so it will transparently negotiate
//! HTTP/2 or HTTP/1.1 to each upstream and keep separate pools per origin.

use crate::error::{PostGateError, Result};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::header::HeaderMap;
use hyper::{Request, Response};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Boxed body type matching what the proxy forwards. Using a single body type
/// across the proxy lets us store exactly one client.
pub type UpstreamBody = BoxBody<Bytes, hyper::Error>;

/// Handle to a shared hyper client.
pub type SharedClient = Arc<Client<HttpsConnector<HttpConnector>, UpstreamBody>>;

/// Build the shared upstream client with connection pooling enabled.
///
/// * ALPN: advertises `h2` and `http/1.1` so upstreams pick HTTP/2 when they
///   support it. Falls back transparently to HTTP/1.1.
/// * Idle pool timeout is kept modest (30s) to avoid holding many sockets open
///   against dev servers that restart frequently.
pub fn build_upstream_client(enable_http2: bool) -> SharedClient {
    let builder = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1();

    let https = if enable_http2 {
        builder.enable_http2().build()
    } else {
        builder.build()
    };

    // Connection pool tuning: the defaults are fine for most workloads, but
    // we cap idle sockets so we don't hold file descriptors indefinitely.
    let client = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(16)
        .build(https);

    Arc::new(client)
}

/// Forward a request to an absolute URL through the shared client, then
/// collect the response body eagerly. This is the common shape used by all
/// proxy code paths that don't need response streaming.
///
/// `timeout_ms` optionally wraps both the request dispatch and body-collect
/// phases with a tokio timeout — returning `PostGateError::Proxy("timeout")`
/// when exceeded. Used to implement whistle's `timeout://<ms>` rule action.
pub async fn forward_collect(
    client: &SharedClient,
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
    timeout_ms: Option<u64>,
) -> Result<(Response<()>, CapturedBody)> {
    forward_collect_with_proxy(client, method, absolute_url, headers, body, timeout_ms, None).await
}

/// Same as `forward_collect` but routes through an upstream proxy if one
/// was supplied via a `proxy://` / `socks://` rule action.
pub async fn forward_collect_with_proxy(
    client: &SharedClient,
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
    timeout_ms: Option<u64>,
    upstream_proxy: Option<&crate::rules::UpstreamProxy>,
) -> Result<(Response<()>, CapturedBody)> {
    // When a proxy rule matched we bypass the pooled client entirely because
    // the pool keys on (scheme, host, port) and knows nothing about chained
    // proxies. Performance-critical use cases for chained proxies are rare.
    if let Some(proxy) = upstream_proxy {
        return super::chain::forward_via_proxy(method, absolute_url, headers, body, proxy, timeout_ms)
            .await;
    }

    let req = build_upstream_request(method, absolute_url, headers, body)?;
    let fut = async {
        let resp = client
            .request(req)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Upstream request failed: {}", e)))?;

        let (parts, incoming) = resp.into_parts();
        let captured = collect_body(incoming, MAX_BODY_SIZE)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;

        Ok((Response::from_parts(parts, ()), captured))
    };

    match timeout_ms {
        Some(ms) => match tokio::time::timeout(std::time::Duration::from_millis(ms), fut).await {
            Ok(result) => result,
            Err(_) => Err(PostGateError::Proxy(format!(
                "Upstream request timed out after {} ms",
                ms
            ))),
        },
        None => fut.await,
    }
}

/// Forward a request to an absolute URL and return the streaming response
/// (parts + unconsumed body) so the caller can either collect or pass through.
/// Used by the TTFB-optimized streaming path.
///
/// `timeout_ms` bounds only the time until response headers arrive — streaming
/// the body itself is unbounded, matching whistle semantics where the timeout
/// covers only upstream responsiveness.
pub async fn forward_stream(
    client: &SharedClient,
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
    timeout_ms: Option<u64>,
) -> Result<(hyper::http::response::Parts, hyper::body::Incoming)> {
    let req = build_upstream_request(method, absolute_url, headers, body)?;
    let fut = async {
        let resp = client
            .request(req)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Upstream request failed: {}", e)))?;
        Ok(resp.into_parts())
    };

    match timeout_ms {
        Some(ms) => match tokio::time::timeout(std::time::Duration::from_millis(ms), fut).await {
            Ok(result) => result,
            Err(_) => Err(PostGateError::Proxy(format!(
                "Upstream request timed out after {} ms",
                ms
            ))),
        },
        None => fut.await,
    }
}

/// Build a boxed-body request targeting an absolute URL, filtering out
/// hop-by-hop headers that confuse pooled HTTP/2 connections.
fn build_upstream_request(
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<Request<UpstreamBody>> {
    let mut builder = Request::builder().method(method).uri(absolute_url);

    // `Connection`/`Transfer-Encoding`/`Upgrade` are hop-by-hop; forwarding
    // them into a pooled h2 connection triggers stream resets. `Host` is
    // unnecessary — hyper derives it from the absolute URI.
    for (k, v) in headers {
        let key_lower = k.to_lowercase();
        if matches!(
            key_lower.as_str(),
            "connection"
                | "keep-alive"
                | "proxy-connection"
                | "transfer-encoding"
                | "upgrade"
                | "host"
        ) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(key_lower.as_bytes()),
            hyper::header::HeaderValue::from_str(v),
        ) {
            builder = builder.header(name, value);
        }
    }

    builder
        .body(Full::new(body).map_err(|_| unreachable!()).boxed())
        .map_err(|e| PostGateError::Proxy(format!("Failed to build upstream request: {}", e)))
}

/// Like [`build_upstream_request`] but takes a full `HeaderMap` so multi-value
/// headers (`cookie` over h2, repeated client headers) reach the upstream
/// with their original cardinality intact. Used by the header-preserving
/// forwarding path introduced to fix lost cookies.
fn build_upstream_request_from_headermap(
    method: hyper::Method,
    absolute_url: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Request<UpstreamBody>> {
    let mut req = Request::builder()
        .method(method)
        .uri(absolute_url)
        .body(Full::new(body).map_err(|_| unreachable!()).boxed())
        .map_err(|e| PostGateError::Proxy(format!("Failed to build upstream request: {}", e)))?;

    // Copy headers directly so multi-value entries (e.g. several `cookie` or
    // `via` header lines) are preserved. The caller is responsible for
    // having already stripped hop-by-hop headers via
    // `proxy::headers::build_forward_request_headers`.
    {
        let dst = req.headers_mut();
        for (name, value) in headers.iter() {
            dst.append(name.clone(), value.clone());
        }
    }

    Ok(req)
}

/// Forward a request using a caller-provided `HeaderMap`, then collect the
/// response body eagerly. This is the multi-value-preserving counterpart to
/// `forward_collect`.
pub async fn forward_collect_headermap(
    client: &SharedClient,
    method: hyper::Method,
    absolute_url: &str,
    headers: &HeaderMap,
    body: Bytes,
    timeout_ms: Option<u64>,
) -> Result<(Response<()>, CapturedBody)> {
    let req = build_upstream_request_from_headermap(method, absolute_url, headers, body)?;
    let fut = async {
        let resp = client
            .request(req)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Upstream request failed: {}", e)))?;

        let (parts, incoming) = resp.into_parts();
        let captured = collect_body(incoming, MAX_BODY_SIZE)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;

        Ok((Response::from_parts(parts, ()), captured))
    };

    match timeout_ms {
        Some(ms) => match tokio::time::timeout(std::time::Duration::from_millis(ms), fut).await {
            Ok(result) => result,
            Err(_) => Err(PostGateError::Proxy(format!(
                "Upstream request timed out after {} ms",
                ms
            ))),
        },
        None => fut.await,
    }
}

/// Streaming variant of [`forward_collect_headermap`].
pub async fn forward_stream_headermap(
    client: &SharedClient,
    method: hyper::Method,
    absolute_url: &str,
    headers: &HeaderMap,
    body: Bytes,
    timeout_ms: Option<u64>,
) -> Result<(hyper::http::response::Parts, hyper::body::Incoming)> {
    let req = build_upstream_request_from_headermap(method, absolute_url, headers, body)?;
    let fut = async {
        let resp = client
            .request(req)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Upstream request failed: {}", e)))?;
        Ok(resp.into_parts())
    };

    match timeout_ms {
        Some(ms) => match tokio::time::timeout(std::time::Duration::from_millis(ms), fut).await {
            Ok(result) => result,
            Err(_) => Err(PostGateError::Proxy(format!(
                "Upstream request timed out after {} ms",
                ms
            ))),
        },
        None => fut.await,
    }
}
