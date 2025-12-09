//! HTTP/2 support for proxy connections
//!
//! This module provides HTTP/2 client and server handling for HTTPS tunnels.

use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use crate::proxy::handler::ProxyContext;
use crate::state::{CapturedRequestData, CapturedRequestEvent, RequestEventType};
use bytes::Bytes;
use h2::server::SendResponse;
use h2::RecvStream;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{Request, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use uuid::Uuid;

use super::tls::{create_tls_connector, parse_server_name};

/// Check if a connection should use HTTP/2
pub fn should_use_http2(alpn: Option<&[u8]>) -> bool {
    alpn.map(|p| p == b"h2").unwrap_or(false)
}

/// Handle an HTTP/2 connection from client
pub async fn handle_http2_connection<S>(
    stream: S,
    host: String,
    port: u16,
    ctx: Arc<ProxyContext>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut connection = h2::server::handshake(stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("H2 handshake error: {}", e)))?;

    while let Some(result) = connection.accept().await {
        let (request, respond) = result
            .map_err(|e| PostGateError::Proxy(format!("H2 accept error: {}", e)))?;

        let host = host.clone();
        let ctx = ctx.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_http2_request(request, respond, &host, port, ctx).await {
                tracing::error!("HTTP/2 request error: {}", e);
            }
        });
    }

    Ok(())
}

/// Handle a single HTTP/2 request
async fn handle_http2_request(
    request: Request<RecvStream>,
    mut respond: SendResponse<Bytes>,
    host: &str,
    port: u16,
    ctx: Arc<ProxyContext>,
) -> Result<()> {
    let request_id = Uuid::new_v4().to_string();
    let start_time = std::time::Instant::now();
    let timestamp = chrono::Utc::now().timestamp_millis();

    let method = request.method().to_string();
    let path = request
        .uri()
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());
    let url = format!("https://{}:{}{}", host, port, path);

    // Extract headers
    let request_headers: HashMap<String, String> = request
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
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
            protocol: "h2".to_string(),
            content_type: content_type.clone(),
            tls_version: Some("TLS 1.3".to_string()),
            ..Default::default()
        },
    });

    // Match rules
    let matched_rules = ctx
        .rule_engine
        .match_request(&method, host, &path, &request_headers);
    let matched_rule_ids: Vec<String> = matched_rules.iter().map(|r| r.id.clone()).collect();

    // Collect request body from H2 stream
    let (parts, body) = request.into_parts();
    let request_body = collect_h2_body(body).await?;
    let request_size = request_body.size as u64;

    ctx.body_storage
        .store_request_body(&request_id, request_body.clone())
        .await;

    // Forward to upstream via HTTP/2
    match forward_http2_request(&parts, request_body.data, host, port).await {
        Ok((response, response_body)) => {
            let status = response.status().as_u16();
            let response_headers: HashMap<String, String> = response
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string().to_lowercase(),
                        v.to_str().unwrap_or("").to_string(),
                    )
                })
                .collect();

            let response_content_type = response_headers.get("content-type").cloned();
            let response_size = response_body.size as u64;
            let duration = start_time.elapsed().as_millis() as u64;

            ctx.body_storage
                .store_response_body(&request_id, response_body.clone())
                .await;

            // Emit completion event
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
                    response_headers: Some(response_headers.clone()),
                    duration_ms: Some(duration),
                    matched_rules: matched_rule_ids,
                    protocol: "h2".to_string(),
                    content_type: response_content_type,
                    request_size,
                    response_size: Some(response_size),
                    tls_version: Some("TLS 1.3".to_string()),
                    ..Default::default()
                },
            });

            // Send response back to client
            let mut h2_response = Response::builder().status(response.status());

            for (key, value) in response.headers() {
                h2_response = h2_response.header(key, value);
            }

            let h2_response = h2_response
                .body(())
                .map_err(|e| PostGateError::Proxy(format!("Failed to build response: {}", e)))?;

            let mut send_stream = respond
                .send_response(h2_response, false)
                .map_err(|e| PostGateError::Proxy(format!("Failed to send response: {}", e)))?;

            send_stream
                .send_data(response_body.data, true)
                .map_err(|e| PostGateError::Proxy(format!("Failed to send data: {}", e)))?;
        }
        Err(e) => {
            let duration = start_time.elapsed().as_millis() as u64;
            tracing::error!("HTTP/2 forward error: {}", e);

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
                    protocol: "h2".to_string(),
                    request_size,
                    error: Some(e.to_string()),
                    ..Default::default()
                },
            });

            // Send error response
            let error_response = Response::builder()
                .status(502)
                .body(())
                .map_err(|e| PostGateError::Proxy(format!("Failed to build error response: {}", e)))?;

            let mut send_stream = respond
                .send_response(error_response, false)
                .map_err(|e| PostGateError::Proxy(format!("Failed to send error response: {}", e)))?;

            send_stream
                .send_data(Bytes::from(format!("Proxy error: {}", e)), true)
                .map_err(|e| PostGateError::Proxy(format!("Failed to send error data: {}", e)))?;
        }
    }

    Ok(())
}

/// Collect body from H2 RecvStream
async fn collect_h2_body(mut body: RecvStream) -> Result<CapturedBody> {
    let mut collected = Vec::new();
    let mut truncated = false;

    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(|e| PostGateError::Proxy(format!("H2 body error: {}", e)))?;

        if collected.len() + chunk.len() > MAX_BODY_SIZE {
            let remaining = MAX_BODY_SIZE - collected.len();
            collected.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }

        collected.extend_from_slice(&chunk);

        // Release flow control capacity
        let _ = body.flow_control().release_capacity(chunk.len());
    }

    Ok(CapturedBody {
        data: Bytes::from(collected.clone()),
        size: collected.len(),
        truncated,
    })
}

/// Forward request to upstream server via HTTP/2
async fn forward_http2_request(
    parts: &hyper::http::request::Parts,
    body: Bytes,
    host: &str,
    port: u16,
) -> Result<(Response<()>, CapturedBody)> {
    // Connect to upstream
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to connect: {}", e)))?;

    // TLS handshake with ALPN for HTTP/2
    let connector = create_tls_connector()?;
    let server_name = parse_server_name(host)?;

    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("TLS error: {}", e)))?;

    // HTTP/2 handshake
    let (mut client, conn) = h2::client::handshake(tls_stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("H2 client handshake error: {}", e)))?;

    // Spawn connection driver
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("H2 connection error: {}", e);
        }
    });

    // Build request
    let uri = parts
        .uri
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());

    let mut request = Request::builder().method(parts.method.clone()).uri(uri);

    // Copy headers
    for (key, value) in parts.headers.iter() {
        // Skip connection-specific headers
        let key_str = key.as_str();
        if key_str == "connection" || key_str == "transfer-encoding" || key_str == "upgrade" {
            continue;
        }
        request = request.header(key, value);
    }

    // Add required HTTP/2 headers if missing
    if !parts.headers.contains_key("host") {
        request = request.header("host", host);
    }

    let request = request
        .body(())
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;

    // Send request
    let (response, mut send_stream) = client
        .send_request(request, false)
        .map_err(|e| PostGateError::Proxy(format!("Failed to send request: {}", e)))?;

    // Send body
    send_stream
        .send_data(body, true)
        .map_err(|e| PostGateError::Proxy(format!("Failed to send body: {}", e)))?;

    // Wait for response
    let response = response
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to receive response: {}", e)))?;

    let (parts, body) = response.into_parts();

    // Collect response body
    let response_body = collect_h2_body(body).await?;

    Ok((Response::from_parts(parts, ()), response_body))
}
