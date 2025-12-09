use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::proxy::body::{collect_body, MAX_BODY_SIZE};
use crate::proxy::handler::ProxyContext;
use crate::state::{CapturedRequestData, CapturedRequestEvent, RequestEventType};
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
use tokio::net::TcpStream;
use uuid::Uuid;

use super::tls::{create_tls_connector, parse_server_name, TlsAcceptor};

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
    let io = TokioIo::new(tls_stream);

    let host = host.to_string();
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
                async move { handle_https_request(req, &host, port, ctx).await }
            }),
        )
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTPS tunnel error: {}", e)))?;

    Ok(())
}

/// Handle an HTTPS request after TLS termination
async fn handle_https_request(
    req: Request<Incoming>,
    host: &str,
    port: u16,
    ctx: Arc<ProxyContext>,
) -> std::result::Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let request_id = Uuid::new_v4().to_string();
    let start_time = std::time::Instant::now();
    let timestamp = chrono::Utc::now().timestamp_millis();

    let method = req.method().to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());
    let url = format!("https://{}:{}{}", host, port, path);

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
            method: method.clone(),
            url: url.clone(),
            host: host.to_string(),
            path: path.clone(),
            request_headers: Some(request_headers.clone()),
            protocol: "https".to_string(),
            content_type: content_type.clone(),
            tls_version: Some("TLS 1.3".to_string()),
            ..Default::default()
        },
    });

    // Match rules
    let matched_rules = ctx.rule_engine.match_request(&method, host, &path, &request_headers);
    let matched_rule_ids: Vec<String> = matched_rules.iter().map(|r| r.id.clone()).collect();

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

    // Forward to upstream
    match forward_https_request(parts, request_body.data, host, port).await {
        Ok((resp, response_body)) => {
            let status = resp.status().as_u16();
            let response_headers: HashMap<String, String> = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
                .collect();

            let response_content_type = response_headers.get("content-type").cloned();
            let response_size = response_body.size as u64;
            let duration = start_time.elapsed().as_millis() as u64;

            ctx.body_storage.store_response_body(&request_id, response_body.clone()).await;

            // Emit completed event
            ctx.app_state.emit_request_event(&CapturedRequestEvent {
                id: request_id.clone(),
                event_type: RequestEventType::Completed,
                data: CapturedRequestData {
                    id: request_id,
                    timestamp,
                    method,
                    url,
                    host: host.to_string(),
                    path,
                    request_headers: Some(request_headers),
                    response_status: Some(status),
                    response_headers: Some(response_headers),
                    duration_ms: Some(duration),
                    matched_rules: matched_rule_ids,
                    protocol: "https".to_string(),
                    content_type: response_content_type,
                    request_size,
                    response_size: Some(response_size),
                    tls_version: Some("TLS 1.3".to_string()),
                    ..Default::default()
                },
            });

            // Rebuild response
            let (resp_parts, _) = resp.into_parts();
            Ok(Response::from_parts(
                resp_parts,
                Full::new(response_body.data).map_err(|_| unreachable!()).boxed(),
            ))
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
                    method,
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

/// Forward an HTTPS request to the upstream server
async fn forward_https_request(
    parts: hyper::http::request::Parts,
    body: Bytes,
    host: &str,
    port: u16,
) -> Result<(Response<()>, crate::proxy::body::CapturedBody)> {
    use http_body_util::Empty;

    // Connect to upstream
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

    // TLS handshake with upstream
    let connector = create_tls_connector()?;
    let server_name = parse_server_name(host)?;

    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("TLS connect error: {}", e)))?;

    let io = TokioIo::new(tls_stream);

    // Create HTTP connection with proper body type
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP handshake error: {}", e)))?;

    // Spawn connection driver
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("Connection error: {}", e);
        }
    });

    // Build the request with just path and query
    let uri = parts
        .uri
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());

    let mut builder = Request::builder().method(parts.method).uri(uri);

    // Copy headers
    for (key, value) in parts.headers.iter() {
        builder = builder.header(key, value);
    }

    let new_req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;

    // Send request
    let resp = sender
        .send_request(new_req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to send request: {}", e)))?;

    // Collect response body
    let (resp_parts, resp_body) = resp.into_parts();
    let captured_body = collect_body(resp_body, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;

    Ok((Response::from_parts(resp_parts, ()), captured_body))
}
