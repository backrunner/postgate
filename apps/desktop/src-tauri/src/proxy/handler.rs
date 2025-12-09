use crate::cert::CertificateAuthority;
use crate::debug::ScriptInjector;
use crate::error::Result;
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use crate::proxy::pool::ConnectionPool;
use crate::proxy::BodyStorage;
use crate::rules::{apply_request_rules, apply_response_rules, RuleEngine};
use crate::state::{AppState, CapturedRequestData, CapturedRequestEvent, RequestEventType};
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use uuid::Uuid;

use super::tls::TlsAcceptor;
use super::tunnel::tunnel_connection;

/// Context passed through the proxy pipeline
pub struct ProxyContext {
    pub ca: Arc<CertificateAuthority>,
    pub rule_engine: Arc<RuleEngine>,
    pub body_storage: Arc<BodyStorage>,
    pub app_state: Arc<AppState>,
    pub connection_pool: Arc<ConnectionPool>,
    pub enable_http2: bool,
}

/// Handle an incoming connection
pub async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    ctx: Arc<ProxyContext>,
) -> Result<()> {
    let io = TokioIo::new(stream);

    let ctx_clone = ctx.clone();

    http1::Builder::new()
        .preserve_header_case(true)
        .title_case_headers(true)
        .serve_connection(
            io,
            service_fn(move |req| {
                let ctx = ctx_clone.clone();
                let addr = peer_addr;
                async move { handle_request(req, addr, ctx).await }
            }),
        )
        .with_upgrades()
        .await
        .map_err(|e| crate::error::PostGateError::Proxy(format!("HTTP error: {}", e)))?;

    Ok(())
}

/// Handle a single HTTP request
async fn handle_request(
    req: Request<Incoming>,
    peer_addr: SocketAddr,
    ctx: Arc<ProxyContext>,
) -> std::result::Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let request_id = Uuid::new_v4().to_string();
    let start_time = std::time::Instant::now();
    let timestamp = chrono::Utc::now().timestamp_millis();

    // Check if this is a CONNECT request (HTTPS tunnel)
    if req.method() == Method::CONNECT {
        return handle_connect(req, &request_id, timestamp, ctx).await;
    }

    // Extract request info
    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let host = extract_host(&req);
    let path = req.uri().path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());

    // Extract headers
    let request_headers: HashMap<String, String> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let content_type = request_headers.get("content-type").cloned();

    // Emit request started event
    ctx.app_state.emit_request_event(&CapturedRequestEvent {
        id: request_id.clone(),
        event_type: RequestEventType::Started,
        data: CapturedRequestData {
            id: request_id.clone(),
            timestamp,
            method: method.clone(),
            url: uri.clone(),
            host: host.clone(),
            path: path.clone(),
            request_headers: Some(request_headers.clone()),
            protocol: "http1".to_string(),
            content_type: content_type.clone(),
            remote_addr: Some(peer_addr.to_string()),
            ..Default::default()
        },
    });

    // Match rules for this request
    let matched_rules = ctx.rule_engine.match_request(&method, &host, &path, &request_headers);
    let matched_rule_ids: Vec<String> = matched_rules.iter().map(|r| r.id.clone()).collect();

    // Collect request body first (needed for rule application)
    let (parts, body) = req.into_parts();
    let request_body = match collect_body(body, MAX_BODY_SIZE).await {
        Ok(b) => b,
        Err(e) => {
            emit_error_event(&ctx, &request_id, timestamp, &method, &uri, &host, &path, 
                &request_headers, start_time.elapsed().as_millis() as u64, &e.to_string());
            return Ok(error_response(502, &format!("Failed to read request body: {}", e)));
        }
    };

    let request_size = request_body.size as u64;

    // Store original request body
    ctx.body_storage.store_request_body(&request_id, request_body.clone()).await;

    // Apply request rules
    let request_modification = apply_request_rules(
        &matched_rules,
        &uri,
        &method,
        &request_headers,
        Some(&request_body.data),
    );

    // Handle short-circuit responses (e.g., redirect, file, statusCode)
    if let Some(short_circuit) = request_modification.short_circuit {
        let duration = start_time.elapsed().as_millis() as u64;
        
        // Store the short-circuit response as response body
        let response_body = CapturedBody {
            data: short_circuit.body.clone(),
            size: short_circuit.body.len(),
            truncated: false,
        };
        ctx.body_storage.store_response_body(&request_id, response_body).await;

        // Emit completion event
        ctx.app_state.emit_request_event(&CapturedRequestEvent {
            id: request_id.clone(),
            event_type: RequestEventType::Completed,
            data: CapturedRequestData {
                id: request_id,
                timestamp,
                method,
                url: uri,
                host,
                path,
                request_headers: Some(request_headers),
                response_status: Some(short_circuit.status),
                response_headers: Some(short_circuit.headers.clone()),
                duration_ms: Some(duration),
                matched_rules: matched_rule_ids,
                protocol: "http1".to_string(),
                content_type: short_circuit.headers.get("content-type").cloned(),
                request_size,
                response_size: Some(short_circuit.body.len() as u64),
                ..Default::default()
            },
        });

        // Build short-circuit response
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

    // Use modified headers and body for the forwarded request
    let modified_body = request_modification.body.unwrap_or(request_body.data.clone());
    let target_host = request_modification.target_host.as_ref().unwrap_or(&host);

    // Rebuild request with modified body and headers
    let mut new_req = Request::builder()
        .method(parts.method.clone())
        .uri(parts.uri.clone());

    // Apply modified headers
    for (k, v) in &request_modification.headers {
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(k.as_bytes()),
            hyper::header::HeaderValue::from_str(v),
        ) {
            new_req = new_req.header(name, value);
        }
    }

    let req = new_req
        .body(Full::new(modified_body).map_err(|_| unreachable!()).boxed())
        .unwrap();

    // Forward request to upstream
    let response = forward_http_request(req, target_host).await;

    match response {
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
                &uri,
                &method,
                &request_headers,
                &response_headers,
                Some(&response_body.data),
                response_content_type.as_deref(),
            );

            // Apply response delay if specified
            if let Some(delay_ms) = response_modification.delay_ms {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            // Use modified body and headers
            let mut final_body = response_modification.body.unwrap_or(response_body.data.clone());
            let mut final_headers = response_modification.headers;

            // Inject debug script if enabled
            if response_modification.inject_debug {
                let injector = ScriptInjector::new(9229); // Default debug port
                if let Ok(html) = String::from_utf8(final_body.to_vec()) {
                    if !ScriptInjector::is_already_injected(&html) {
                        let injected = injector.inject_into_html(&html);
                        final_body = Bytes::from(injected);
                        // Update content-length header
                        final_headers.insert("content-length".to_string(), final_body.len().to_string());
                    }
                }
            }

            // Apply response speed throttling if specified
            if let Some(speed_kbps) = response_modification.speed_kbps {
                final_body = super::throttle::apply_throttle(final_body, Some(speed_kbps)).await;
            }

            let final_size = final_body.len() as u64;

            // Store the final response body
            let stored_body = CapturedBody {
                data: final_body.clone(),
                size: final_body.len(),
                truncated: response_body.truncated,
            };
            ctx.body_storage.store_response_body(&request_id, stored_body).await;

            let final_duration = start_time.elapsed().as_millis() as u64;

            // Emit completion event
            ctx.app_state.emit_request_event(&CapturedRequestEvent {
                id: request_id.clone(),
                event_type: RequestEventType::Completed,
                data: CapturedRequestData {
                    id: request_id,
                    timestamp,
                    method,
                    url: uri,
                    host,
                    path,
                    request_headers: Some(request_headers),
                    response_status: Some(status),
                    response_headers: Some(final_headers.clone()),
                    duration_ms: Some(final_duration),
                    matched_rules: matched_rule_ids,
                    protocol: "http1".to_string(),
                    content_type: final_headers.get("content-type").cloned(),
                    request_size,
                    response_size: Some(final_size),
                    ..Default::default()
                },
            });

            // Build final response with modified headers and body
            let mut builder = Response::builder().status(status);
            for (k, v) in &final_headers {
                if let (Ok(name), Ok(value)) = (
                    hyper::header::HeaderName::from_bytes(k.as_bytes()),
                    hyper::header::HeaderValue::from_str(v),
                ) {
                    builder = builder.header(name, value);
                }
            }
            let new_resp = builder
                .body(Full::new(final_body).map_err(|_| unreachable!()).boxed())
                .unwrap();

            Ok(new_resp)
        }
        Err(e) => {
            let error_duration = start_time.elapsed().as_millis() as u64;
            emit_error_event(&ctx, &request_id, timestamp, &method, &uri, &host, &path,
                &request_headers, error_duration, &e.to_string());
            Ok(error_response(502, &format!("Proxy error: {}", e)))
        }
    }
}

/// Handle CONNECT request for HTTPS tunneling
async fn handle_connect(
    req: Request<Incoming>,
    request_id: &str,
    timestamp: i64,
    ctx: Arc<ProxyContext>,
) -> std::result::Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let host = req.uri().host().unwrap_or("").to_string();
    let port = req.uri().port_u16().unwrap_or(443);

    tracing::debug!("CONNECT request to {}:{}", host, port);

    // Emit started event for CONNECT
    ctx.app_state.emit_request_event(&CapturedRequestEvent {
        id: request_id.to_string(),
        event_type: RequestEventType::Started,
        data: CapturedRequestData {
            id: request_id.to_string(),
            timestamp,
            method: "CONNECT".to_string(),
            url: format!("{}:{}", host, port),
            host: host.clone(),
            path: "/".to_string(),
            protocol: "https".to_string(),
            ..Default::default()
        },
    });

    let ca = ctx.ca.clone();
    let request_id = request_id.to_string();
    let ctx_clone = ctx.clone();

    // Upgrade the connection
    tokio::task::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                // Create TLS acceptor for MITM
                match TlsAcceptor::new(&ca, &host) {
                    Ok(acceptor) => {
                        // Pass the upgraded connection wrapped in TokioIo
                        let io = TokioIo::new(upgraded);
                        if let Err(e) = tunnel_connection(io, acceptor, &host, port, ca, ctx_clone).await {
                            tracing::debug!("Tunnel error for {}: {}", request_id, e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to create TLS acceptor for {}: {}", host, e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Upgrade error: {}", e);
            }
        }
    });

    // Return 200 Connection Established
    Ok(Response::builder()
        .status(200)
        .body(Empty::<Bytes>::new().map_err(|_: std::convert::Infallible| unreachable!()).boxed())
        .unwrap())
}

/// Forward an HTTP request to the upstream server and capture response body
async fn forward_http_request(
    req: Request<BoxBody<Bytes, hyper::Error>>,
    _host: &str,
) -> std::result::Result<(Response<()>, CapturedBody), Box<dyn std::error::Error + Send + Sync>> {
    use hyper_util::client::legacy::Client;
    use hyper_util::rt::TokioExecutor;

    let client: Client<_, BoxBody<Bytes, hyper::Error>> = Client::builder(TokioExecutor::new())
        .build_http();

    let resp = client.request(req).await?;
    
    // Collect response body
    let (parts, body) = resp.into_parts();
    let incoming_body: Incoming = body;
    let captured_body = collect_body(incoming_body, MAX_BODY_SIZE).await?;
    
    let resp_without_body = Response::from_parts(parts, ());
    
    Ok((resp_without_body, captured_body))
}

/// Extract host from request
fn extract_host(req: &Request<Incoming>) -> String {
    req.uri()
        .host()
        .map(|h| h.to_string())
        .unwrap_or_else(|| {
            req.headers()
                .get("host")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("unknown")
                .split(':')
                .next()
                .unwrap_or("unknown")
                .to_string()
        })
}

/// Create an error response
fn error_response(status: u16, message: &str) -> Response<BoxBody<Bytes, hyper::Error>> {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
        .header("content-type", "text/plain")
        .body(Full::new(Bytes::from(message.to_string())).map_err(|_| unreachable!()).boxed())
        .unwrap()
}

/// Emit an error event
fn emit_error_event(
    ctx: &ProxyContext,
    request_id: &str,
    timestamp: i64,
    method: &str,
    url: &str,
    host: &str,
    path: &str,
    request_headers: &HashMap<String, String>,
    duration: u64,
    error: &str,
) {
    ctx.app_state.emit_request_event(&CapturedRequestEvent {
        id: request_id.to_string(),
        event_type: RequestEventType::Error,
        data: CapturedRequestData {
            id: request_id.to_string(),
            timestamp,
            method: method.to_string(),
            url: url.to_string(),
            host: host.to_string(),
            path: path.to_string(),
            request_headers: Some(request_headers.clone()),
            duration_ms: Some(duration),
            protocol: "http1".to_string(),
            error: Some(error.to_string()),
            ..Default::default()
        },
    });
}
