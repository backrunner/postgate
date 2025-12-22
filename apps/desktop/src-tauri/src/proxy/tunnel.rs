use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use crate::proxy::forward::ForwardTarget;
use crate::proxy::handler::ProxyContext;
use crate::rules::{apply_request_rules, apply_response_rules};
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

use super::tls::{create_tls_connector, parse_server_name, tls_version_string, TlsAcceptor};

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
    
    // Get the negotiated TLS version from the client connection
    let tls_version = {
        let (_, server_conn) = tls_stream.get_ref();
        tls_version_string(server_conn.protocol_version())
    };
    
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
                let tls_ver = tls_version.clone();
                async move { handle_https_request(req, &host, port, ctx, &tls_ver).await }
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

    // Apply request rules to get modifications
    let request_modification = apply_request_rules(
        &matched_rules,
        &url,
        &method_str,
        &request_headers,
        Some(&request_body.data),
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
        for (k, v) in &short_circuit.headers {
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

    // Determine target for forwarding
    let forward_result = if let Some(target_host) = &request_modification.target_host {
        // Rule specifies a target - parse it and forward
        let remaining_path = request_modification.remaining_path.as_deref().unwrap_or("");
        
        match ForwardTarget::parse(target_host, remaining_path, "https") {
            Ok(target) => {
                tracing::debug!(
                    "Forwarding {} to {} (remaining: {})", 
                    url, 
                    target.build_url(),
                    remaining_path
                );
                
                // Use modified body and headers
                let body_to_send = request_modification.body.unwrap_or(request_body.data.clone());
                forward_to_target(method.clone(), &target, &request_modification.headers, body_to_send).await
            }
            Err(e) => Err(e)
        }
    } else {
        // No rule target - forward to original host
        let body_to_send = request_modification.body.unwrap_or(request_body.data.clone());
        forward_https_request(parts, body_to_send, host, port).await
    };

    match forward_result {
        Ok((resp, response_body)) => {
            let status = resp.status().as_u16();
            let response_headers: HashMap<String, String> = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
                .collect();

            let response_content_type = response_headers.get("content-type").cloned();
            
            // Apply response rules
            let response_modification = apply_response_rules(
                &matched_rules,
                &url,
                &method_str,
                &request_headers,
                &response_headers,
                Some(&response_body.data),
                response_content_type.as_deref(),
            );

            let final_body = response_modification.body.unwrap_or(response_body.data.clone());
            let final_status = response_modification.status_code.unwrap_or(status);
            
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
                    response_headers: Some(response_modification.headers.clone()),
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
            for (k, v) in &response_modification.headers {
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

/// Forward request to a specific target (handles protocol conversion)
async fn forward_to_target(
    method: hyper::Method,
    target: &ForwardTarget,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    if target.is_https() {
        forward_https_to_target(method, target, headers, body).await
    } else {
        forward_http_to_target(method, target, headers, body).await
    }
}

/// Forward to HTTP target
async fn forward_http_to_target(
    method: hyper::Method,
    target: &ForwardTarget,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    let addr = format!("{}:{}", target.host, target.port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP handshake error: {}", e)))?;

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("HTTP connection error: {}", e);
        }
    });

    let uri = target.build_path();
    let mut builder = Request::builder().method(method).uri(&uri);

    // Copy headers, updating Host header
    for (key, value) in headers {
        let key_lower = key.to_lowercase();
        if key_lower == "host" {
            let host_value = if target.port == 80 {
                target.host.clone()
            } else {
                format!("{}:{}", target.host, target.port)
            };
            builder = builder.header("host", host_value);
        } else {
            if let (Ok(name), Ok(value)) = (
                hyper::header::HeaderName::from_bytes(key.as_bytes()),
                hyper::header::HeaderValue::from_str(value),
            ) {
                builder = builder.header(name, value);
            }
        }
    }

    // Ensure Host header exists
    if !headers.contains_key("host") {
        let host_value = if target.port == 80 {
            target.host.clone()
        } else {
            format!("{}:{}", target.host, target.port)
        };
        builder = builder.header("host", host_value);
    }

    let req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;

    let resp = sender.send_request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP request error: {}", e)))?;

    let (parts, incoming) = resp.into_parts();
    let response_body = collect_body(incoming, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;
    
    Ok((Response::from_parts(parts, ()), response_body))
}

/// Forward to HTTPS target
async fn forward_https_to_target(
    method: hyper::Method,
    target: &ForwardTarget,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    let addr = format!("{}:{}", target.host, target.port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

    let connector = create_tls_connector()?;
    let server_name = parse_server_name(&target.host)?;

    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("TLS connect error: {}", e)))?;

    let io = TokioIo::new(tls_stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTPS handshake error: {}", e)))?;

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("HTTPS connection error: {}", e);
        }
    });

    let uri = target.build_path();
    let mut builder = Request::builder().method(method).uri(&uri);

    // Copy headers, updating Host header
    for (key, value) in headers {
        let key_lower = key.to_lowercase();
        if key_lower == "host" {
            let host_value = if target.port == 443 {
                target.host.clone()
            } else {
                format!("{}:{}", target.host, target.port)
            };
            builder = builder.header("host", host_value);
        } else {
            if let (Ok(name), Ok(value)) = (
                hyper::header::HeaderName::from_bytes(key.as_bytes()),
                hyper::header::HeaderValue::from_str(value),
            ) {
                builder = builder.header(name, value);
            }
        }
    }

    // Ensure Host header exists
    if !headers.contains_key("host") {
        let host_value = if target.port == 443 {
            target.host.clone()
        } else {
            format!("{}:{}", target.host, target.port)
        };
        builder = builder.header("host", host_value);
    }

    let req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;

    let resp = sender.send_request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTPS request error: {}", e)))?;

    let (parts, incoming) = resp.into_parts();
    let response_body = collect_body(incoming, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;
    
    Ok((Response::from_parts(parts, ()), response_body))
}

/// Forward an HTTPS request to the original upstream server (no rule target)
async fn forward_https_request(
    parts: hyper::http::request::Parts,
    body: Bytes,
    host: &str,
    port: u16,
) -> Result<(Response<()>, CapturedBody)> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

    let connector = create_tls_connector()?;
    let server_name = parse_server_name(host)?;

    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("TLS connect error: {}", e)))?;

    let io = TokioIo::new(tls_stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP handshake error: {}", e)))?;

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("Connection error: {}", e);
        }
    });

    let uri = parts
        .uri
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());

    let mut builder = Request::builder().method(parts.method).uri(uri);

    for (key, value) in parts.headers.iter() {
        builder = builder.header(key, value);
    }

    let new_req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;

    let resp = sender
        .send_request(new_req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to send request: {}", e)))?;

    let (resp_parts, resp_body) = resp.into_parts();
    let captured_body = collect_body(resp_body, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;

    Ok((Response::from_parts(resp_parts, ()), captured_body))
}
