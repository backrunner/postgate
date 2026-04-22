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
pub async fn forward_collect(
    client: &SharedClient,
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    let req = build_upstream_request(method, absolute_url, headers, body)?;
    let resp = client
        .request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Upstream request failed: {}", e)))?;

    let (parts, incoming) = resp.into_parts();
    let captured = collect_body(incoming, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;

    Ok((Response::from_parts(parts, ()), captured))
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
