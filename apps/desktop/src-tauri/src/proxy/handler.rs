use crate::cert::CertificateAuthority;
use crate::debug::ScriptInjector;
use crate::error::Result;
use crate::plugin::{PluginRequest, PluginRequestContext, PluginResponse};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use crate::proxy::pool::ConnectionPool;
use crate::proxy::sse;
use crate::proxy::websocket;
use crate::proxy::BodyStorage;
use crate::rules::{
    apply_request_rules_with_values, apply_response_rules_with_values, ResolveCtx, RuleEngine,
};
use crate::state::{AppState, CapturedRequestData, CapturedRequestEvent, RequestEventType};
use crate::values::RequestCtx;
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
use super::upstream::SharedClient;

/// Context passed through the proxy pipeline
pub struct ProxyContext {
    pub ca: Arc<CertificateAuthority>,
    pub rule_engine: Arc<RuleEngine>,
    pub body_storage: Arc<BodyStorage>,
    pub app_state: Arc<AppState>,
    pub connection_pool: Arc<ConnectionPool>,
    pub enable_http2: bool,
    /// Shared hyper client used for ALL upstream forwarding.
    ///
    /// Hugely important for performance: `hyper_util::client::legacy::Client`
    /// has a built-in connection pool keyed by (scheme, host, port). Rebuilding
    /// the client per request (which the old code did) threw away the pool and
    /// forced a fresh TCP + TLS handshake for every single request — see
    /// `handler.rs:732` in git history. A single client is enough because it
    /// serves both HTTP and HTTPS through the hyper-rustls connector.
    pub upstream_client: SharedClient,
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

    // Check if this is a WebSocket upgrade request
    if websocket::is_websocket_upgrade(&request_headers) {
        return handle_websocket_upgrade(req, &request_id, timestamp, &host, &path, &request_headers, peer_addr, ctx).await;
    }

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
    // Extract protocol and port for filter matching
    let protocol = req.uri().scheme_str().unwrap_or("http").to_string();
    let port = req.uri().port_u16().unwrap_or(if protocol == "https" { 443 } else { 80 });
    let matched_rules = ctx.rule_engine.match_request(&method, &host, &path, &protocol, port, &request_headers);
    let matched_rule_ids: Vec<String> = matched_rules.iter().map(|r| r.rule.raw_line.clone()).collect();

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
    // Persist request body
    ctx.app_state.persist_body(request_id.clone(), request_body.data.clone(), true);

    // Ensure the in-memory values store is populated so `{name}` references
    // in rule actions resolve on the first request after startup.
    let _ = ctx.app_state.ensure_values_loaded().await;

    // Build query + cookie maps for template interpolation.
    let query_map = build_query_map(&uri);
    let cookie_map = build_cookie_map(&request_headers);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let values_ctx = RequestCtx {
        url: &uri,
        method: &method,
        client_ip: &peer_addr.ip().to_string(),
        req_headers: &request_headers,
        query: &query_map,
        req_cookies: &cookie_map,
        now_ms,
    };
    let resolve_ctx = ResolveCtx {
        store: Some(&ctx.app_state.values_store),
        ctx: Some(&values_ctx),
    };

    // Apply request rules
    let request_modification = apply_request_rules_with_values(
        &matched_rules,
        &uri,
        &method,
        &request_headers,
        Some(&request_body.data),
        &resolve_ctx,
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
        ctx.body_storage.store_response_body(&request_id, response_body.clone()).await;
        // Persist response body
        ctx.app_state.persist_body(request_id.clone(), response_body.data.clone(), false);

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

        // Build short-circuit response.
        // Ensure Content-Length matches the body we're sending. Hyper's
        // `Full::new` already sets it, but being explicit avoids any
        // transfer-encoding confusion.
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

    // Apply plugin handleRequest if a plugin was matched
    if let Some(ref plugin_info) = request_modification.plugin {
        let plugin_request = PluginRequest {
            id: request_id.clone(),
            method: method.clone(),
            url: uri.clone(),
            host: host.clone(),
            path: path.clone(),
            query: extract_query_params(&uri),
            headers: request_headers.clone(),
            body: Some(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &request_body.data,
            )),
            body_base64: true,
            timestamp,
        };

        let plugin_context = PluginRequestContext {
            rule_config: match &plugin_info.config {
                serde_json::Value::Object(map) => map.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                _ => std::collections::HashMap::new(),
            },
            matched_pattern: uri.clone(),
        };

        let plugin_manager = ctx.app_state.plugin_manager.read().await;
        match plugin_manager.handle_request(&plugin_info.name, plugin_request, plugin_context).await {
            Ok(Some(modified)) => {
                let duration = start_time.elapsed().as_millis() as u64;
                let decoded_body = if modified.body_base64 {
                    modified.body.as_ref()
                        .and_then(|b| base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            b,
                        ).ok())
                        .map(Bytes::from)
                } else {
                    modified.body.as_ref().map(|b| Bytes::from(b.clone()))
                };
                let body_bytes = decoded_body.unwrap_or_default();

                let response_body = CapturedBody {
                    data: body_bytes.clone(),
                    size: body_bytes.len(),
                    truncated: false,
                };
                ctx.body_storage.store_response_body(&request_id, response_body.clone()).await;
                ctx.app_state.persist_body(request_id.clone(), body_bytes.clone(), false);

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
                        response_status: Some(modified.status),
                        response_headers: Some(modified.headers.clone()),
                        duration_ms: Some(duration),
                        matched_rules: matched_rule_ids,
                        protocol: "http1".to_string(),
                        content_type: modified.headers.get("content-type").cloned(),
                        request_size,
                        response_size: Some(body_bytes.len() as u64),
                        ..Default::default()
                    },
                });

                let mut builder = Response::builder().status(modified.status);
                let mut resp_headers = modified.headers.clone();
                resp_headers.remove("content-encoding");
                resp_headers.remove("transfer-encoding");
                resp_headers.insert("content-length".to_string(), body_bytes.len().to_string());
                for (k, v) in &resp_headers {
                    builder = builder.header(k.as_str(), v.as_str());
                }
                return Ok(builder
                    .body(Full::new(body_bytes).map_err(|_| unreachable!()).boxed())
                    .unwrap());
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Plugin handleRequest failed for {}: {}", plugin_info.name, e);
            }
        }
    }

    // Apply request delay if specified
    if let Some(delay_ms) = request_modification.delay_ms {
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
    }

    // Use modified headers and body for the forwarded request
    let modified_body = request_modification.body.unwrap_or(request_body.data.clone());
    
    // Build the target URI using ForwardTarget for whistle-compatible path forwarding
    let target_uri = if let Some(target) = &request_modification.target_host {
        let remaining_path = request_modification.remaining_path.as_deref().unwrap_or("");
        
        // Parse target and build final URL with remaining path
        match super::forward::ForwardTarget::parse(target, remaining_path, &protocol) {
            Ok(ft) => {
                tracing::debug!(
                    "Forwarding {} to {} (remaining: {})", 
                    uri, 
                    ft.build_url(),
                    remaining_path
                );
                ft.build_url()
            }
            Err(e) => {
                tracing::error!("Failed to parse target {}: {}", target, e);
                parts.uri.to_string()
            }
        }
    } else {
        // No target_host override - use original URI
        parts.uri.to_string()
    };

    // Rebuild request with modified body and headers
    let mut new_req = Request::builder()
        .method(parts.method.clone())
        .uri(&target_uri);

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

    // Forward request to upstream and check for SSE
    let response = forward_http_request_check_sse(req, &request_id, &ctx).await;

    match response {
        Ok(ForwardResult::Sse { parts, body, content_type }) => {
            // Handle SSE streaming response
            let status = parts.status.as_u16();
            let response_headers: HashMap<String, String> = parts.headers
                .iter()
                .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
                .collect();

            // Emit started event with SSE protocol
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
                    response_status: Some(status),
                    response_headers: Some(response_headers.clone()),
                    matched_rules: matched_rule_ids.clone(),
                    protocol: "sse".to_string(),
                    content_type,
                    request_size,
                    ..Default::default()
                },
            });

            // For SSE, we need to stream the body while capturing events
            // Use a wrapping stream that captures data as it passes through
            let request_id_clone = request_id.clone();
            let ctx_clone = ctx.clone();

            // Create a wrapper that captures SSE events while streaming
            let wrapped_body = SseCapturingBody::new(body, request_id_clone, ctx_clone.app_state.clone());

            // Build streaming response
            let mut builder = Response::builder().status(status);
            for (k, v) in &response_headers {
                if let (Ok(name), Ok(value)) = (
                    hyper::header::HeaderName::from_bytes(k.as_bytes()),
                    hyper::header::HeaderValue::from_str(v),
                ) {
                    builder = builder.header(name, value);
                }
            }
            
            // Convert the wrapped body to BoxBody
            let boxed_body = wrapped_body.boxed();
            Ok(builder.body(boxed_body).unwrap())
        }
        Ok(ForwardResult::Normal { response: resp, body: response_body }) => {
            let status = resp.status().as_u16();
            let response_headers: HashMap<String, String> = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
                .collect();

            let response_content_type = response_headers.get("content-type").cloned();

            // Apply response rules
            let response_modification = apply_response_rules_with_values(
                &matched_rules,
                &uri,
                &method,
                &request_headers,
                &response_headers,
                Some(&response_body.data),
                response_content_type.as_deref(),
                &resolve_ctx,
            );

            // Apply plugin handleResponse if a plugin was matched
            let (plugin_modified_body, plugin_modified_headers) = if let Some(ref plugin_info) = request_modification.plugin {
                // Build PluginRequest from request data
                let plugin_request = PluginRequest {
                    id: request_id.clone(),
                    method: method.clone(),
                    url: uri.clone(),
                    host: host.clone(),
                    path: path.clone(),
                    query: extract_query_params(&uri),
                    headers: request_headers.clone(),
                    body: Some(base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &request_body.data,
                    )),
                    body_base64: true,
                    timestamp,
                };

                // Build PluginResponse from response data
                let plugin_response = PluginResponse {
                    status,
                    headers: response_headers.clone(),
                    body: Some(base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &response_body.data,
                    )),
                    body_base64: true,
                };

                // Build context from rule config
                let plugin_context = PluginRequestContext {
                    rule_config: match &plugin_info.config {
                        serde_json::Value::Object(map) => map.iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                        _ => std::collections::HashMap::new(),
                    },
                    matched_pattern: uri.clone(),
                };

                // Call plugin handleResponse
                let plugin_manager = ctx.app_state.plugin_manager.read().await;
                match plugin_manager.handle_response(&plugin_info.name, plugin_request, plugin_response, plugin_context).await {
                    Ok(modified) => {
                        // Decode the modified body if base64 encoded
                        let decoded_body = if modified.body_base64 {
                            modified.body.as_ref()
                                .and_then(|b| base64::Engine::decode(
                                    &base64::engine::general_purpose::STANDARD,
                                    b,
                                ).ok())
                                .map(Bytes::from)
                        } else {
                            modified.body.as_ref().map(|b| Bytes::from(b.clone()))
                        };
                        (decoded_body, Some(modified.headers))
                    }
                    Err(e) => {
                        tracing::warn!("Plugin handleResponse failed for {}: {}", plugin_info.name, e);
                        (None, None)
                    }
                }
            } else {
                (None, None)
            };

            // Apply response delay if specified
            if let Some(delay_ms) = response_modification.delay_ms {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            // Use modified body and headers (plugin modifications take precedence)
            let mut final_body = plugin_modified_body
                .or(response_modification.body)
                .unwrap_or(response_body.data.clone());
            let mut final_headers = plugin_modified_headers
                .unwrap_or(response_modification.headers);

            // If the body was replaced/modified relative to what upstream
            // sent, the upstream's `content-length` / `content-encoding` /
            // `transfer-encoding` headers no longer describe the new body.
            // Leaving them in place results in ERR_EMPTY_RESPONSE in the
            // browser (content-length mismatch) or decoding errors when
            // upstream sent gzip/br and we replaced it with plain text.
            let body_was_modified = final_body != response_body.data;
            if body_was_modified {
                final_headers.remove("content-encoding");
                final_headers.remove("transfer-encoding");
                final_headers.insert(
                    "content-length".to_string(),
                    final_body.len().to_string(),
                );
            }

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
            ctx.body_storage.store_response_body(&request_id, stored_body.clone()).await;
            // Persist response body
            ctx.app_state.persist_body(request_id.clone(), stored_body.data.clone(), false);

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
    // Validate and extract hostname
    let host = match req.uri().host() {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => {
            tracing::warn!("CONNECT request with missing or empty hostname");
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Missing hostname in CONNECT request"))
                    .map_err(|_: std::convert::Infallible| unreachable!())
                    .boxed())
                .unwrap());
        }
    };

    // Basic hostname validation (allow localhost, IPs, and valid domain names)
    if !is_valid_hostname(&host) {
        tracing::warn!("CONNECT request with invalid hostname: {}", host);
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Full::new(Bytes::from("Invalid hostname"))
                .map_err(|_: std::convert::Infallible| unreachable!())
                .boxed())
            .unwrap());
    }

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
                // Create TLS acceptor for MITM. ALPN advertisement must
                // reflect `enable_http2` so disabling h2 actually prevents
                // clients from negotiating it.
                match TlsAcceptor::new(&ca, &host, ctx_clone.enable_http2) {
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

/// Forward HTTP request and check if response is SSE
/// Returns either (response, body) for normal responses or starts SSE streaming
async fn forward_http_request_check_sse(
    req: Request<BoxBody<Bytes, hyper::Error>>,
    _request_id: &str,
    ctx: &Arc<ProxyContext>,
) -> std::result::Result<ForwardResult, Box<dyn std::error::Error + Send + Sync>> {
    // Use the shared, pooled upstream client. Building one per request would
    // discard the connection pool and force a fresh TCP + TLS handshake each
    // time, which dominated wall-clock time on page loads.
    let resp = ctx.upstream_client.request(req).await?;
    
    // Check if this is an SSE response
    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    
    if sse::is_sse_response(content_type.as_deref()) {
        // Return SSE info for streaming
        let (parts, body) = resp.into_parts();
        return Ok(ForwardResult::Sse {
            parts,
            body,
            content_type,
        });
    }
    
    // Normal response - collect body
    let (parts, body) = resp.into_parts();
    let incoming_body: Incoming = body;
    let captured_body = collect_body(incoming_body, MAX_BODY_SIZE).await?;
    let resp_without_body = Response::from_parts(parts, ());
    
    Ok(ForwardResult::Normal {
        response: resp_without_body,
        body: captured_body,
    })
}

/// Result of forwarding an HTTP request
enum ForwardResult {
    /// Normal HTTP response with collected body
    Normal {
        response: Response<()>,
        body: CapturedBody,
    },
    /// SSE response that needs streaming
    Sse {
        parts: hyper::http::response::Parts,
        body: Incoming,
        content_type: Option<String>,
    },
}

/// Extract query parameters from URL
fn extract_query_params(uri: &str) -> HashMap<String, String> {
    url::Url::parse(uri)
        .map(|u| {
            u.query_pairs()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default()
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

/// Validate hostname for CONNECT requests
/// Allows localhost, IP addresses, and valid domain names
fn is_valid_hostname(host: &str) -> bool {
    // Allow localhost
    if host == "localhost" {
        return true;
    }

    // Allow IP addresses (both IPv4 and IPv6)
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }

    // Basic domain name validation
    // - Must not be empty
    // - Must not start or end with a dot or hyphen
    // - Labels must be 1-63 characters
    // - Total length must be <= 253 characters
    if host.is_empty() || host.len() > 253 {
        return false;
    }

    if host.starts_with('.') || host.ends_with('.') || host.starts_with('-') || host.ends_with('-') {
        return false;
    }

    // Check each label
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        // Allow alphanumeric and hyphens
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
    }

    true
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

/// Handle WebSocket upgrade request
async fn handle_websocket_upgrade(
    req: Request<Incoming>,
    request_id: &str,
    timestamp: i64,
    host: &str,
    path: &str,
    request_headers: &HashMap<String, String>,
    peer_addr: SocketAddr,
    ctx: Arc<ProxyContext>,
) -> std::result::Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let url = format!("ws://{}{}", host, path);
    
    // Emit request started event with websocket protocol
    ctx.app_state.emit_request_event(&CapturedRequestEvent {
        id: request_id.to_string(),
        event_type: RequestEventType::Started,
        data: CapturedRequestData {
            id: request_id.to_string(),
            timestamp,
            method: "GET".to_string(),
            url: url.clone(),
            host: host.to_string(),
            path: path.to_string(),
            request_headers: Some(request_headers.clone()),
            protocol: "websocket".to_string(),
            remote_addr: Some(peer_addr.to_string()),
            ..Default::default()
        },
    });

    let request_id = request_id.to_string();
    let host = host.to_string();
    let path = path.to_string();
    let ctx_clone = ctx.clone();
    let target_url = websocket::build_ws_url(&host, 80, &path, false);

    // Spawn the WebSocket upgrade handling
    tokio::task::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                let io = TokioIo::new(upgraded);
                let ws_proxy = websocket::WebSocketProxy::new(request_id.clone(), ctx_clone.app_state.clone());
                
                if let Err(e) = ws_proxy.proxy(io, &target_url).await {
                    tracing::debug!("WebSocket proxy error for {}: {}", request_id, e);
                }
            }
            Err(e) => {
                tracing::error!("WebSocket upgrade error: {}", e);
            }
        }
    });

    // Return 101 Switching Protocols
    Ok(Response::builder()
        .status(101)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .body(Empty::<Bytes>::new().map_err(|_: std::convert::Infallible| unreachable!()).boxed())
        .unwrap())
}

// ==================== SSE Capturing Body ====================

use std::pin::Pin;
use std::task::{Context, Poll};
use hyper::body::{Body, Frame, SizeHint};

/// A body wrapper that captures SSE events as data flows through
struct SseCapturingBody {
    inner: Incoming,
    connection_id: String,
    app_state: Arc<AppState>,
    parser: sse::SseParser,
    message_count: u64,
    total_bytes: u64,
    start_time: std::time::Instant,
    ended: bool,
}

impl SseCapturingBody {
    fn new(inner: Incoming, connection_id: String, app_state: Arc<AppState>) -> Self {
        Self {
            inner,
            connection_id,
            app_state,
            parser: sse::SseParser::new(),
            message_count: 0,
            total_bytes: 0,
            start_time: std::time::Instant::now(),
            ended: false,
        }
    }

    fn emit_stream_ended(&self) {
        self.app_state.emit_stream_ended(&crate::state::StreamEndedEvent {
            connection_id: self.connection_id.clone(),
            message_count: self.message_count,
            total_bytes: self.total_bytes,
            duration_ms: self.start_time.elapsed().as_millis() as u64,
            close_reason: None,
        });
    }
}

impl Body for SseCapturingBody {
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
                    let chunk = data.clone();
                    self.total_bytes += chunk.len() as u64;

                    // Parse SSE events from the chunk
                    let events = self.parser.feed(&chunk);
                    for event in events {
                        self.message_count += 1;
                        
                        let event_data = if let Some(ref event_type) = event.event_type {
                            format!("event: {}\ndata: {}", event_type, event.data)
                        } else {
                            event.data.clone()
                        };

                        let stream_msg = crate::state::StreamMessage {
                            id: uuid::Uuid::new_v4().to_string(),
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            direction: crate::state::StreamDirection::Inbound,
                            message_type: crate::state::StreamMessageType::SseEvent,
                            data: event_data,
                            is_base64: false,
                            size: event.data.len(),
                        };

                        self.app_state.emit_stream_message(&crate::state::StreamMessageEvent {
                            connection_id: self.connection_id.clone(),
                            message: stream_msg,
                        });
                    }
                }
                
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(e))) => {
                if !self.ended {
                    self.ended = true;
                    self.emit_stream_ended();
                }
                Poll::Ready(Some(std::result::Result::Err(e)))
            }
            Poll::Ready(None) => {
                if !self.ended {
                    self.ended = true;
                    self.emit_stream_ended();
                }
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

/// Helpers reused by `tunnel.rs` for whistle `{name}` reference resolution.
pub mod tunnel_value_helpers {
    use std::collections::HashMap;

    /// Parse `?foo=bar&baz=qux` out of a URL into a flat map.
    pub fn query_map(url: &str) -> HashMap<String, String> {
        super::build_query_map(url)
    }

    /// Parse a `Cookie:` header into a flat map.
    pub fn cookie_map(headers: &HashMap<String, String>) -> HashMap<String, String> {
        super::build_cookie_map(headers)
    }
}

/// Parse `?foo=bar&baz=qux` out of a URL into a flat map used by the values
/// template interpolator (`${query.foo}`).
fn build_query_map(url: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(qs) = url.split_once('?').map(|(_, q)| q) {
        let qs = qs.split('#').next().unwrap_or(qs);
        for pair in qs.split('&').filter(|s| !s.is_empty()) {
            match pair.split_once('=') {
                Some((k, v)) => {
                    map.insert(
                        urlencoding::decode(k).map(|s| s.into_owned()).unwrap_or_else(|_| k.to_string()),
                        urlencoding::decode(v).map(|s| s.into_owned()).unwrap_or_else(|_| v.to_string()),
                    );
                }
                None => {
                    map.insert(pair.to_string(), String::new());
                }
            }
        }
    }
    map
}

/// Parse a `Cookie:` header into a flat map (`${reqCookie.session}`).
fn build_cookie_map(headers: &HashMap<String, String>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(raw) = headers.get("cookie") {
        for pair in raw.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            match pair.split_once('=') {
                Some((k, v)) => {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
                None => {
                    map.insert(pair.to_string(), String::new());
                }
            }
        }
    }
    map
}
