use crate::cert::CertificateAuthority;
use crate::debug::ScriptInjector;
use crate::error::Result;
use crate::plugin::{PluginRequest, PluginRequestContext, PluginResponse};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use crate::proxy::error_page::ProxyErrorKind;
use crate::proxy::headers::{
    apply_headers_to_response_builder, build_forward_request_headers,
    build_forward_response_headers, flat_to_headermap, headermap_to_flat,
    sync_request_body_headers,
};
use crate::proxy::pool::ConnectionPool;
use crate::proxy::sse;
use crate::proxy::websocket;
use crate::proxy::BodyStorage;
use crate::rules::{
    apply_request_rules_with_values, apply_response_rules_with_values, feature, is_enabled,
    persist_request_writes, persist_response_writes, remote_resource_urls_for_request,
    remote_resource_urls_for_response_context, MatchedRule, RequestWriteContext, ResolveCtx,
    ResolvedResources, ResponseModification, ResponseWriteContext, RuleEngine,
};
use crate::state::{AppState, CapturedRequestData, CapturedRequestEvent, RequestEventType};
use crate::values::RequestCtx;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::header::HeaderMap;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use uuid::Uuid;

use super::resource::RemoteResourceCache;
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
    /// Shared cache for remote `http(s)` resources referenced by whistle rules.
    pub remote_resource_cache: RemoteResourceCache,
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
        return handle_connect(req, &request_id, timestamp, peer_addr, ctx).await;
    }

    // Extract request info
    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let host = extract_host(&req);
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());

    // Extract headers
    //
    // We keep the ORIGINAL `HeaderMap` (with full multi-value fidelity) for
    // rebuilding the upstream request and the client response later on —
    // folding multi-value headers like `cookie` (HTTP/2) or repeated `via`
    // into a single string would silently drop data we need to forward.
    // The flat `HashMap` is still produced for rule matching, UI events
    // and template resolution, which only need a representative value.
    let original_request_headers: HeaderMap = req.headers().clone();
    let request_headers: HashMap<String, String> = headermap_to_flat(&original_request_headers);

    let content_type = request_headers.get("content-type").cloned();

    // Check if this is a WebSocket upgrade request
    if websocket::is_websocket_upgrade(&request_headers) {
        return handle_websocket_upgrade(
            req,
            &request_id,
            timestamp,
            &host,
            &path,
            &request_headers,
            peer_addr,
            ctx,
        )
        .await;
    }

    // Match rules for this request
    // Extract protocol and port for filter matching
    let protocol = req.uri().scheme_str().unwrap_or("http").to_string();
    let port = req
        .uri()
        .port_u16()
        .unwrap_or(if protocol == "https" { 443 } else { 80 });
    let client_ip = peer_addr.ip().to_string();
    let matched_rules = ctx.rule_engine.match_request_with_client_ip(
        &method,
        &host,
        &path,
        &protocol,
        port,
        &request_headers,
        Some(&client_ip),
    );
    let matched_rule_ids: Vec<String> = matched_rules
        .iter()
        .map(|r| r.rule.raw_line.clone())
        .collect();

    // Collect request body first (needed for rule application)
    let (parts, body) = req.into_parts();
    let request_body = match collect_body(body, MAX_BODY_SIZE).await {
        Ok(b) => b,
        Err(e) => {
            emit_error_event(
                &ctx,
                &request_id,
                timestamp,
                &method,
                &uri,
                &host,
                &path,
                &request_headers,
                start_time.elapsed().as_millis() as u64,
                &e.to_string(),
            );
            return Ok(error_response(
                502,
                ProxyErrorKind::Request,
                &format!("Failed to read request body: {}", e),
            ));
        }
    };

    let request_size = request_body.size as u64;

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
        client_ip: &client_ip,
        req_headers: &request_headers,
        query: &query_map,
        req_cookies: &cookie_map,
        now_ms,
    };
    let mut remote_resources: ResolvedResources = ctx
        .remote_resource_cache
        .fetch_all(
            &ctx.upstream_client,
            &remote_resource_urls_for_request(&matched_rules),
        )
        .await;

    // Apply request rules
    let mut request_modification = {
        let resolve_ctx = ResolveCtx {
            store: Some(&ctx.app_state.values_store),
            ctx: Some(&values_ctx),
            remote_resources: Some(&remote_resources),
        };
        apply_request_rules_with_values(
            &matched_rules,
            &uri,
            &method,
            &request_headers,
            Some(&request_body.data),
            &resolve_ctx,
        )
    };

    // `enable://abort` — tear down the connection without a normal response.
    // Maps to whistle's abort feature, used to simulate peer resets during
    // testing. We send `Connection: close` with an empty body and 444; hyper
    // then closes the TCP connection after flushing.
    if crate::rules::should_abort(&request_modification) {
        tracing::debug!("abort:// matched for {}; closing connection", uri);
        return Ok(Response::builder()
            .status(444) // nginx convention: "no response"
            .header("connection", "close")
            .body(
                Empty::<Bytes>::new()
                    .map_err(|_: std::convert::Infallible| unreachable!())
                    .boxed(),
            )
            .unwrap());
    }

    let final_method = request_modification
        .method
        .as_deref()
        .and_then(|m| Method::from_bytes(m.as_bytes()).ok())
        .unwrap_or_else(|| parts.method.clone());
    let final_method_str = final_method.to_string();

    let mut body_to_send = request_modification
        .body
        .clone()
        .unwrap_or(request_body.data.clone());
    if let Some(speed_kbps) = request_modification.speed_kbps {
        body_to_send = super::throttle::apply_throttle(body_to_send, Some(speed_kbps)).await;
    }
    sync_request_body_headers(
        &mut request_modification.headers,
        &request_body.data,
        &body_to_send,
    );

    // Build the target URI using ForwardTarget for whistle-compatible path forwarding.
    let base_target_uri = if let Some(target) = &request_modification.target_host {
        let remaining_path = request_modification.remaining_path.as_deref().unwrap_or("");

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
        parts.uri.to_string()
    };
    let target_uri = match super::forward::apply_request_url_modifications(
        &base_target_uri,
        request_modification.path.as_deref(),
        request_modification.query_params.as_deref(),
    ) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!("Failed to apply URL modifications for {}: {}", uri, e);
            base_target_uri
        }
    };
    let display_uri = target_uri.clone();
    let display_path = path_and_query_from_url(&display_uri).unwrap_or_else(|| path.clone());

    // Build the outgoing request headers as soon as the final URL/body are
    // known so reqWrite:// also fires for short-circuit/plugin responses.
    let forward_header_map: HeaderMap = build_forward_request_headers(
        &original_request_headers,
        &request_headers,
        &request_modification.headers,
        &request_modification.headers_to_remove,
    );
    let forward_headers = headermap_to_flat(&forward_header_map);
    persist_request_writes(
        &request_modification.write_files,
        RequestWriteContext {
            method: &final_method_str,
            url: &target_uri,
            headers: &forward_headers,
            body: &body_to_send,
            force: is_enabled(&request_modification, feature::FORCE_REQ_WRITE),
        },
    );

    // Emit request started event (after rule match so `disable://capture`
    // can suppress it). Users typically care about the Completed event in
    // the UI, but we still want to surface Started for long-running requests.
    let capture = crate::rules::capture_enabled(&request_modification);
    let force_res_write = is_enabled(&request_modification, feature::FORCE_RES_WRITE);
    if capture {
        ctx.body_storage
            .store_request_body(&request_id, request_body.clone())
            .await;
        ctx.app_state
            .persist_body(request_id.clone(), request_body.data.clone(), true);
    }
    if capture {
        ctx.app_state.emit_request_event(&CapturedRequestEvent {
            id: request_id.clone(),
            event_type: RequestEventType::Started,
            data: CapturedRequestData {
                id: request_id.clone(),
                timestamp,
                method: final_method_str.clone(),
                url: display_uri.clone(),
                host: host.clone(),
                path: display_path.clone(),
                request_headers: Some(request_headers.clone()),
                protocol: "http1".to_string(),
                content_type: content_type.clone(),
                remote_addr: Some(peer_addr.to_string()),
                matched_rules: matched_rule_ids.clone(),
                ..Default::default()
            },
        });
    }

    // Handle short-circuit responses (e.g., redirect, file, statusCode)
    if let Some(short_circuit) = request_modification.short_circuit {
        let response_headers = short_circuit.headers;
        let response_content_type = response_headers.get("content-type").cloned();
        prefetch_response_remote_resources(
            &ctx,
            &mut remote_resources,
            &matched_rules,
            &display_uri,
            &final_method_str,
            &request_headers,
            &response_headers,
            short_circuit.status,
            response_content_type.as_deref(),
        )
        .await;
        let resolve_ctx = ResolveCtx {
            store: Some(&ctx.app_state.values_store),
            ctx: Some(&values_ctx),
            remote_resources: Some(&remote_resources),
        };
        let final_response = super::direct_response::finalize_direct_response(
            super::direct_response::DirectResponseContext {
                matched_rules: &matched_rules,
                url: &display_uri,
                method: &final_method_str,
                request_headers: &request_headers,
                status: short_circuit.status,
                response_headers,
                body: short_circuit.body,
                resolve_ctx: &resolve_ctx,
                force_res_write,
                debug_port: ctx.app_state.debug_port_for_injection().await,
            },
        )
        .await;
        let duration = start_time.elapsed().as_millis() as u64;

        let response_body = CapturedBody {
            data: final_response.body.clone(),
            size: final_response.body.len(),
            truncated: false,
        };
        if capture {
            ctx.body_storage
                .store_response_body(&request_id, response_body.clone())
                .await;
            ctx.app_state
                .persist_body(request_id.clone(), response_body.data.clone(), false);
        }

        // Emit completion event
        if capture {
            ctx.app_state.emit_request_event(&CapturedRequestEvent {
                id: request_id.clone(),
                event_type: RequestEventType::Completed,
                data: CapturedRequestData {
                    id: request_id,
                    timestamp,
                    method: final_method_str,
                    url: display_uri,
                    host,
                    path: display_path,
                    request_headers: Some(request_headers),
                    response_status: Some(final_response.status),
                    response_headers: Some(final_response.flat_headers.clone()),
                    duration_ms: Some(duration),
                    matched_rules: matched_rule_ids,
                    protocol: "http1".to_string(),
                    content_type: final_response.flat_headers.get("content-type").cloned(),
                    request_size,
                    response_size: Some(final_response.body.len() as u64),
                    ..Default::default()
                },
            });
        }

        let builder = Response::builder().status(final_response.status);
        let builder = apply_headers_to_response_builder(builder, &final_response.headers);
        return Ok(builder
            .body(
                Full::new(final_response.body)
                    .map_err(|_| unreachable!())
                    .boxed(),
            )
            .unwrap());
    }

    // Apply plugin handleRequest if a plugin was matched
    if let Some(ref plugin_info) = request_modification.plugin {
        let plugin_request = PluginRequest {
            id: request_id.clone(),
            method: final_method_str.clone(),
            url: display_uri.clone(),
            host: host.clone(),
            path: display_path.clone(),
            query: extract_query_params(&display_uri),
            headers: request_headers.clone(),
            body: Some(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &body_to_send,
            )),
            body_base64: true,
            timestamp,
        };

        let plugin_context = PluginRequestContext {
            rule_config: match &plugin_info.config {
                serde_json::Value::Object(map) => {
                    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                }
                _ => std::collections::HashMap::new(),
            },
            matched_pattern: display_uri.clone(),
        };

        let plugin_manager = ctx.app_state.plugin_manager.read().await;
        match plugin_manager
            .handle_request(&plugin_info.name, plugin_request, plugin_context)
            .await
        {
            Ok(Some(modified)) => {
                let decoded_body = if modified.body_base64 {
                    modified
                        .body
                        .as_ref()
                        .and_then(|b| {
                            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b)
                                .ok()
                        })
                        .map(Bytes::from)
                } else {
                    modified.body.as_ref().map(|b| Bytes::from(b.clone()))
                };
                let body_bytes = decoded_body.unwrap_or_default();
                let response_headers = modified.headers;
                let response_content_type = response_headers.get("content-type").cloned();
                prefetch_response_remote_resources(
                    &ctx,
                    &mut remote_resources,
                    &matched_rules,
                    &display_uri,
                    &final_method_str,
                    &request_headers,
                    &response_headers,
                    modified.status,
                    response_content_type.as_deref(),
                )
                .await;
                let resolve_ctx = ResolveCtx {
                    store: Some(&ctx.app_state.values_store),
                    ctx: Some(&values_ctx),
                    remote_resources: Some(&remote_resources),
                };
                let final_response = super::direct_response::finalize_direct_response(
                    super::direct_response::DirectResponseContext {
                        matched_rules: &matched_rules,
                        url: &display_uri,
                        method: &final_method_str,
                        request_headers: &request_headers,
                        status: modified.status,
                        response_headers,
                        body: body_bytes,
                        resolve_ctx: &resolve_ctx,
                        force_res_write,
                        debug_port: ctx.app_state.debug_port_for_injection().await,
                    },
                )
                .await;
                let duration = start_time.elapsed().as_millis() as u64;

                let response_body = CapturedBody {
                    data: final_response.body.clone(),
                    size: final_response.body.len(),
                    truncated: false,
                };
                if capture {
                    ctx.body_storage
                        .store_response_body(&request_id, response_body.clone())
                        .await;
                    ctx.app_state.persist_body(
                        request_id.clone(),
                        response_body.data.clone(),
                        false,
                    );
                }

                if capture {
                    ctx.app_state.emit_request_event(&CapturedRequestEvent {
                        id: request_id.clone(),
                        event_type: RequestEventType::Completed,
                        data: CapturedRequestData {
                            id: request_id,
                            timestamp,
                            method: final_method_str,
                            url: display_uri,
                            host,
                            path: display_path,
                            request_headers: Some(request_headers),
                            response_status: Some(final_response.status),
                            response_headers: Some(final_response.flat_headers.clone()),
                            duration_ms: Some(duration),
                            matched_rules: matched_rule_ids,
                            protocol: "http1".to_string(),
                            content_type: final_response.flat_headers.get("content-type").cloned(),
                            request_size,
                            response_size: Some(final_response.body.len() as u64),
                            ..Default::default()
                        },
                    });
                }

                let builder = Response::builder().status(final_response.status);
                let builder = apply_headers_to_response_builder(builder, &final_response.headers);
                return Ok(builder
                    .body(
                        Full::new(final_response.body)
                            .map_err(|_| unreachable!())
                            .boxed(),
                    )
                    .unwrap());
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    "Plugin handleRequest failed for {}: {}",
                    plugin_info.name,
                    e
                );
            }
        }
    }

    // Apply request delay if specified
    if let Some(delay_ms) = request_modification.delay_ms {
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
    }

    // Rebuild request with modified body and headers
    let mut new_req = Request::builder()
        .method(final_method.clone())
        .uri(&target_uri);

    // Apply the preserved/merged header map.
    {
        if let Some(dst) = new_req.headers_mut() {
            for (name, value) in forward_header_map.iter() {
                dst.append(name.clone(), value.clone());
            }
        }
    }

    let req = new_req
        .body(
            Full::new(body_to_send.clone())
                .map_err(|_| unreachable!())
                .boxed(),
        )
        .unwrap();

    // Forward request to upstream. If no matched rule needs the response body,
    // we stream it straight back to the client — that makes TTFB match the
    // upstream instead of `upstream_ttfb + full_body_download`.
    //
    // When an upstream proxy rule matched we bypass the shared pooled client
    // (the pool can't route chained proxies) and always take the buffering
    // path via `forward_collect_with_proxy`.
    let buffer_response_body = crate::rules::rules_require_response_body(&matched_rules);
    tracing::trace!(
        target: "postgate::perf",
        "[{}] {} {} — {} matched rules, {}",
        request_id,
        method,
        uri,
        matched_rules.len(),
        if buffer_response_body { "BUFFERING body" } else { "streaming body" }
    );
    let forward_start = std::time::Instant::now();
    let response = if let Some(ref proxy) = request_modification.upstream_proxy {
        // Detached path: build the (method, url, headers, body) tuple by
        // tearing `req` apart and handing off to chain::forward_via_proxy.
        let (p, b) = req.into_parts();
        let body_bytes = match b.collect().await {
            Ok(c) => c.to_bytes(),
            Err(e) => {
                emit_error_event(
                    &ctx,
                    &request_id,
                    timestamp,
                    &method,
                    &uri,
                    &host,
                    &path,
                    &request_headers,
                    start_time.elapsed().as_millis() as u64,
                    &format!("request body read failed: {}", e),
                );
                return Ok(error_response(
                    502,
                    ProxyErrorKind::Request,
                    &format!("Failed to read proxied request body: {}", e),
                ));
            }
        };
        // Use the multi-value-aware flattener so chained proxies (which
        // expect a `HashMap<String,String>`) at least receive a single
        // `cookie` header with all crumbles joined, rather than a single
        // random crumble from the last insert.
        let hdr_map: HashMap<String, String> = headermap_to_flat(&p.headers);
        super::upstream::forward_collect_with_proxy(
            &ctx.upstream_client,
            p.method,
            &p.uri.to_string(),
            &hdr_map,
            body_bytes,
            request_modification.timeout_ms,
            Some(proxy),
        )
        .await
        .map(|(resp, body)| ForwardResult::Normal {
            response: resp,
            body,
        })
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })
    } else {
        forward_http_request_check_sse(
            req,
            &request_id,
            &ctx,
            buffer_response_body,
            request_modification.timeout_ms,
        )
        .await
    };
    tracing::trace!(
        target: "postgate::perf",
        "[{}] upstream first byte after {:?}",
        request_id,
        forward_start.elapsed()
    );

    match response {
        Ok(ForwardResult::Sse {
            parts,
            body,
            content_type,
        }) => {
            // Handle SSE streaming response
            let status = parts.status.as_u16();
            let original_resp_headers: HeaderMap = parts.headers.clone();
            let response_headers: HashMap<String, String> =
                headermap_to_flat(&original_resp_headers);

            // Emit started event with SSE protocol
            if capture {
                ctx.app_state.emit_request_event(&CapturedRequestEvent {
                    id: request_id.clone(),
                    event_type: RequestEventType::Started,
                    data: CapturedRequestData {
                        id: request_id.clone(),
                        timestamp,
                        method: final_method_str.clone(),
                        url: display_uri.clone(),
                        host: host.clone(),
                        path: display_path.clone(),
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
            }

            // For SSE, we need to stream the body while capturing events
            // Use a wrapping stream that captures data as it passes through
            let request_id_clone = request_id.clone();
            let ctx_clone = ctx.clone();

            // Create a wrapper that captures SSE events while streaming
            let wrapped_body =
                SseCapturingBody::new(body, request_id_clone, ctx_clone.app_state.clone(), capture);

            // Build streaming response — preserve multi-value headers
            // (particularly `Set-Cookie`) by starting from the original
            // upstream HeaderMap rather than a flattened map.
            let builder = Response::builder().status(status);
            let builder = apply_headers_to_response_builder(builder, &original_resp_headers);

            // Convert the wrapped body to BoxBody
            let boxed_body = wrapped_body.boxed();
            Ok(builder.body(boxed_body).unwrap())
        }
        Ok(ForwardResult::Streaming { parts, body }) => {
            // Fast path: no matched rule rewrites the body, so stream straight
            // through. The client's TTFB then matches the upstream's TTFB.
            let status = parts.status.as_u16();
            let original_resp_headers: HeaderMap = parts.headers.clone();
            let upstream_headers: HashMap<String, String> =
                headermap_to_flat(&original_resp_headers);

            // Apply non-body response rules (headers, status, cookies,
            // header-removes, delay). `response_modification.body` is
            // guaranteed None here because we only took this path when no
            // rule requires the body.
            let response_content_type = upstream_headers.get("content-type").cloned();
            prefetch_response_remote_resources(
                &ctx,
                &mut remote_resources,
                &matched_rules,
                &display_uri,
                &final_method_str,
                &request_headers,
                &upstream_headers,
                status,
                response_content_type.as_deref(),
            )
            .await;
            let resolve_ctx = ResolveCtx {
                store: Some(&ctx.app_state.values_store),
                ctx: Some(&values_ctx),
                remote_resources: Some(&remote_resources),
            };
            let response_modification = apply_response_rules_with_values(
                &matched_rules,
                &display_uri,
                &final_method_str,
                &request_headers,
                &upstream_headers,
                status,
                None,
                response_content_type.as_deref(),
                &resolve_ctx,
            );

            let final_status = response_modification.status_code.unwrap_or(status);
            // Rebuild the outgoing response HeaderMap while preserving the
            // upstream's multi-value headers (notably multiple Set-Cookie).
            // This replaces the legacy flat-map finalization which would
            // collapse multiple Set-Cookie entries into one joined string.
            let final_header_map: HeaderMap = build_forward_response_headers(
                &original_resp_headers,
                &upstream_headers,
                &response_modification,
                None,
            );
            let final_headers = headermap_to_flat(&final_header_map);

            // Apply response delay (resDelay://) before sending the first
            // byte so the client observes the full TTFB penalty, not just
            // the inter-chunk gap.
            if let Some(delay_ms) = response_modification.delay_ms {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            // Wrap body so we can capture bytes for the UI and emit the
            // Completed event when the stream ends, without blocking delivery.
            let wrapped = PassthroughCapturingBody::new(
                body,
                PassthroughMeta {
                    request_id: request_id.clone(),
                    timestamp,
                    method: final_method_str.clone(),
                    url: display_uri.clone(),
                    host: host.clone(),
                    path: display_path.clone(),
                    request_headers: request_headers.clone(),
                    response_status: final_status,
                    response_headers: final_headers.clone(),
                    matched_rules: matched_rule_ids.clone(),
                    protocol: "http1".to_string(),
                    content_type: response_content_type,
                    request_size,
                    start_time,
                    persistence_enabled: ctx.app_state.is_persistence_enabled(),
                    tls_version: None,
                    capture,
                },
                ctx.app_state.clone(),
                ctx.body_storage.clone(),
            );

            let builder = Response::builder().status(final_status);
            let builder = apply_headers_to_response_builder(builder, &final_header_map);
            Ok(builder.body(wrapped.boxed()).unwrap())
        }
        Ok(ForwardResult::Normal {
            response: resp,
            body: response_body,
        }) => {
            let status = resp.status().as_u16();
            let original_resp_headers: HeaderMap = resp.headers().clone();
            let response_headers: HashMap<String, String> =
                headermap_to_flat(&original_resp_headers);

            let response_content_type = response_headers.get("content-type").cloned();

            // Apply response rules
            prefetch_response_remote_resources(
                &ctx,
                &mut remote_resources,
                &matched_rules,
                &display_uri,
                &final_method_str,
                &request_headers,
                &response_headers,
                status,
                response_content_type.as_deref(),
            )
            .await;
            let resolve_ctx = ResolveCtx {
                store: Some(&ctx.app_state.values_store),
                ctx: Some(&values_ctx),
                remote_resources: Some(&remote_resources),
            };
            let response_modification = apply_response_rules_with_values(
                &matched_rules,
                &display_uri,
                &final_method_str,
                &request_headers,
                &response_headers,
                status,
                Some(&response_body.data),
                response_content_type.as_deref(),
                &resolve_ctx,
            );

            // Apply plugin handleResponse if a plugin was matched
            let (plugin_modified_body, plugin_modified_headers) =
                if let Some(ref plugin_info) = request_modification.plugin {
                    // Build PluginRequest from request data
                    let plugin_request = PluginRequest {
                        id: request_id.clone(),
                        method: final_method_str.clone(),
                        url: display_uri.clone(),
                        host: host.clone(),
                        path: display_path.clone(),
                        query: extract_query_params(&display_uri),
                        headers: request_headers.clone(),
                        body: Some(base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            &body_to_send,
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
                            serde_json::Value::Object(map) => {
                                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                            }
                            _ => std::collections::HashMap::new(),
                        },
                        matched_pattern: display_uri.clone(),
                    };

                    // Call plugin handleResponse
                    let plugin_manager = ctx.app_state.plugin_manager.read().await;
                    match plugin_manager
                        .handle_response(
                            &plugin_info.name,
                            plugin_request,
                            plugin_response,
                            plugin_context,
                        )
                        .await
                    {
                        Ok(modified) => {
                            // Decode the modified body if base64 encoded
                            let decoded_body = if modified.body_base64 {
                                modified
                                    .body
                                    .as_ref()
                                    .and_then(|b| {
                                        base64::Engine::decode(
                                            &base64::engine::general_purpose::STANDARD,
                                            b,
                                        )
                                        .ok()
                                    })
                                    .map(Bytes::from)
                            } else {
                                modified.body.as_ref().map(|b| Bytes::from(b.clone()))
                            };
                            (decoded_body, Some(modified.headers))
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Plugin handleResponse failed for {}: {}",
                                plugin_info.name,
                                e
                            );
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
                .or(response_modification.body.clone())
                .unwrap_or(response_body.data.clone());
            let plugin_replaced_headers = plugin_modified_headers.is_some();
            let headers_source =
                plugin_modified_headers.unwrap_or(response_modification.headers.clone());

            // Inject debug script if enabled (this changes the body).
            if response_modification.inject_debug {
                let injector = ScriptInjector::new(ctx.app_state.debug_port_for_injection().await);
                if let Ok(html) = String::from_utf8(final_body.to_vec()) {
                    if !ScriptInjector::is_already_injected(&html) {
                        let injected = injector.inject_into_html(&html);
                        final_body = Bytes::from(injected);
                    }
                }
            }

            // Apply response speed throttling if specified
            if let Some(speed_kbps) = response_modification.speed_kbps {
                final_body = super::throttle::apply_throttle(final_body, Some(speed_kbps)).await;
            }
            let body_was_modified = final_body != response_body.data;

            // Finalize headers: apply `headers_to_remove`, fold in cookies,
            // strip stale content-encoding/transfer-encoding and set
            // content-length when the body was replaced. We now use the
            // HeaderMap-preserving builder so multiple upstream Set-Cookie
            // entries survive intact.
            let mut modified_for_finalize = ResponseModification {
                headers: headers_source,
                headers_to_remove: response_modification.headers_to_remove.clone(),
                cookies: response_modification.cookies.clone(),
                ..Default::default()
            };
            // The helper only needs headers / removals / cookies to do its
            // job; zero the rest.
            modified_for_finalize.status_code = response_modification.status_code;
            // If a plugin replaced the headers wholesale we can't preserve
            // upstream multi-value entries — the plugin has authored a final
            // header map already, so we materialize a fresh HeaderMap from
            // its flat form. Otherwise we keep the upstream HeaderMap as the
            // base so multiple Set-Cookie entries pass through intact.
            let (base_headers, base_flat): (HeaderMap, HashMap<String, String>) =
                if plugin_replaced_headers {
                    let hm = flat_to_headermap(&modified_for_finalize.headers);
                    let flat = modified_for_finalize.headers.clone();
                    (hm, flat)
                } else {
                    (original_resp_headers.clone(), response_headers.clone())
                };
            let final_header_map: HeaderMap = build_forward_response_headers(
                &base_headers,
                &base_flat,
                &modified_for_finalize,
                if body_was_modified {
                    Some(final_body.len())
                } else {
                    None
                },
            );
            let final_headers = headermap_to_flat(&final_header_map);
            persist_response_writes(
                &response_modification.write_files,
                ResponseWriteContext {
                    method: &final_method_str,
                    status: response_modification.status_code.unwrap_or(status),
                    headers: &final_headers,
                    body: &final_body,
                    force: is_enabled(&request_modification, feature::FORCE_RES_WRITE),
                },
            );

            let final_size = final_body.len() as u64;

            // Store the final response body
            let stored_body = CapturedBody {
                data: final_body.clone(),
                size: final_body.len(),
                truncated: response_body.truncated,
            };
            if capture {
                ctx.body_storage
                    .store_response_body(&request_id, stored_body.clone())
                    .await;
                ctx.app_state
                    .persist_body(request_id.clone(), stored_body.data.clone(), false);
            }

            let final_duration = start_time.elapsed().as_millis() as u64;
            let final_status = response_modification.status_code.unwrap_or(status);

            if capture {
                ctx.app_state.emit_request_event(&CapturedRequestEvent {
                    id: request_id.clone(),
                    event_type: RequestEventType::Completed,
                    data: CapturedRequestData {
                        id: request_id,
                        timestamp,
                        method: final_method_str,
                        url: display_uri,
                        host,
                        path: display_path,
                        request_headers: Some(request_headers),
                        response_status: Some(final_status),
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
            }

            // Build final response with modified headers and body
            let builder = Response::builder().status(final_status);
            let builder = apply_headers_to_response_builder(builder, &final_header_map);
            let new_resp = builder
                .body(Full::new(final_body).map_err(|_| unreachable!()).boxed())
                .unwrap();

            Ok(new_resp)
        }
        Err(e) => {
            let error_duration = start_time.elapsed().as_millis() as u64;
            emit_error_event(
                &ctx,
                &request_id,
                timestamp,
                &method,
                &uri,
                &host,
                &path,
                &request_headers,
                error_duration,
                &e.to_string(),
            );
            Ok(error_response(
                502,
                ProxyErrorKind::Upstream,
                &e.to_string(),
            ))
        }
    }
}

/// Handle CONNECT request for HTTPS tunneling
async fn handle_connect(
    req: Request<Incoming>,
    request_id: &str,
    timestamp: i64,
    peer_addr: SocketAddr,
    ctx: Arc<ProxyContext>,
) -> std::result::Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Validate and extract hostname
    let host = match req.uri().host() {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => {
            tracing::warn!("CONNECT request with missing or empty hostname");
            return Ok(error_response(
                400,
                ProxyErrorKind::Tunnel,
                "Missing hostname in CONNECT request",
            ));
        }
    };

    // Basic hostname validation (allow localhost, IPs, and valid domain names)
    if !is_valid_hostname(&host) {
        tracing::warn!("CONNECT request with invalid hostname: {}", host);
        return Ok(error_response(
            400,
            ProxyErrorKind::Tunnel,
            &format!("Invalid hostname: {}", host),
        ));
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
    let client_ip = peer_addr.ip().to_string();

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
                        if let Err(e) =
                            tunnel_connection(io, acceptor, &host, port, ca, ctx_clone, client_ip)
                                .await
                        {
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
        .body(
            Empty::<Bytes>::new()
                .map_err(|_: std::convert::Infallible| unreachable!())
                .boxed(),
        )
        .unwrap())
}

/// Forward HTTP request and decide how to handle the response.
///
/// When `buffer_body` is false AND the response isn't SSE, we return
/// `ForwardResult::Streaming` with the hyper `Incoming` body intact — the
/// caller wires it straight to the client, so TTFB matches the upstream.
/// When `buffer_body` is true, the body is drained into memory so rule
/// actions like `resBody://` and `htmlAppend://` can rewrite it.
async fn forward_http_request_check_sse(
    req: Request<BoxBody<Bytes, hyper::Error>>,
    _request_id: &str,
    ctx: &Arc<ProxyContext>,
    buffer_body: bool,
    timeout_ms: Option<u64>,
) -> std::result::Result<ForwardResult, Box<dyn std::error::Error + Send + Sync>> {
    // Use the shared, pooled upstream client. Building one per request would
    // discard the connection pool and force a fresh TCP + TLS handshake each
    // time, which dominated wall-clock time on page loads.
    //
    // `timeout_ms` (whistle `timeout://<ms>`) only bounds the time until
    // response headers come back; streaming bodies can take as long as they
    // like, matching whistle semantics.
    let request_fut = ctx.upstream_client.request(req);
    let resp = match timeout_ms {
        Some(ms) => {
            match tokio::time::timeout(std::time::Duration::from_millis(ms), request_fut).await {
                Ok(r) => r?,
                Err(_) => {
                    return Err(format!("Upstream request timed out after {} ms", ms).into());
                }
            }
        }
        None => request_fut.await?,
    };

    // Check if this is an SSE response
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if sse::is_sse_response(content_type.as_deref()) && !buffer_body {
        // Return SSE info for streaming
        let (parts, body) = resp.into_parts();
        return Ok(ForwardResult::Sse {
            parts,
            body,
            content_type,
        });
    }

    if !buffer_body {
        // No matched rule needs the body bytes — stream through so the client
        // gets the first byte as soon as the upstream sends it. This is the
        // main lever for bringing TTFB in line with whistle.
        let (parts, body) = resp.into_parts();
        return Ok(ForwardResult::Streaming { parts, body });
    }

    // Buffering path: collect the body so we can run body-modifying rules.
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
    /// Generic streaming pass-through: the body flows straight to the client
    /// without buffering. Used when no matched rule rewrites the body.
    Streaming {
        parts: hyper::http::response::Parts,
        body: Incoming,
    },
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn prefetch_response_remote_resources(
    ctx: &ProxyContext,
    remote_resources: &mut ResolvedResources,
    matched_rules: &[MatchedRule],
    url: &str,
    method: &str,
    request_headers: &HashMap<String, String>,
    response_headers: &HashMap<String, String>,
    status_code: u16,
    content_type: Option<&str>,
) {
    remote_resources.extend(
        ctx.remote_resource_cache
            .fetch_all(
                &ctx.upstream_client,
                &remote_resource_urls_for_response_context(
                    matched_rules,
                    url,
                    method,
                    request_headers,
                    response_headers,
                    status_code,
                    content_type,
                ),
            )
            .await,
    );
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

fn path_and_query_from_url(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri).ok()?;
    let mut path = url.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    Some(path)
}

/// Extract host from request
fn extract_host(req: &Request<Incoming>) -> String {
    req.uri().host().map(|h| h.to_string()).unwrap_or_else(|| {
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

    if host.starts_with('.') || host.ends_with('.') || host.starts_with('-') || host.ends_with('-')
    {
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
fn error_response(
    status: u16,
    kind: ProxyErrorKind,
    message: &str,
) -> Response<BoxBody<Bytes, hyper::Error>> {
    let body = crate::proxy::error_page::html_error_body(status, kind, message);
    let headers = crate::proxy::error_page::html_error_headers(body.len());
    let mut builder =
        Response::builder().status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY));
    for (key, value) in headers {
        builder = builder.header(key, value);
    }
    builder
        .body(Full::new(body).map_err(|_| unreachable!()).boxed())
        .unwrap()
}

/// Emit an error event
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
                let ws_proxy =
                    websocket::WebSocketProxy::new(request_id.clone(), ctx_clone.app_state.clone());

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
        .body(
            Empty::<Bytes>::new()
                .map_err(|_: std::convert::Infallible| unreachable!())
                .boxed(),
        )
        .unwrap())
}

// ==================== SSE Capturing Body ====================

use hyper::body::{Body, Frame, SizeHint};
use std::pin::Pin;
use std::task::{Context, Poll};

/// A body wrapper that captures SSE events as data flows through
struct SseCapturingBody {
    inner: Incoming,
    connection_id: String,
    app_state: Arc<AppState>,
    capture: bool,
    parser: sse::SseParser,
    message_count: u64,
    total_bytes: u64,
    start_time: std::time::Instant,
    ended: bool,
}

impl SseCapturingBody {
    fn new(
        inner: Incoming,
        connection_id: String,
        app_state: Arc<AppState>,
        capture: bool,
    ) -> Self {
        Self {
            inner,
            connection_id,
            app_state,
            capture,
            parser: sse::SseParser::new(),
            message_count: 0,
            total_bytes: 0,
            start_time: std::time::Instant::now(),
            ended: false,
        }
    }

    fn emit_stream_ended(&self) {
        if !self.capture {
            return;
        }
        self.app_state
            .emit_stream_ended(&crate::state::StreamEndedEvent {
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

                    if !self.capture {
                        return Poll::Ready(Some(Ok(frame)));
                    }

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

                        self.app_state
                            .emit_stream_message(&crate::state::StreamMessageEvent {
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

// Passthrough streaming body lives in proxy/passthrough.rs — shared across
// h1, h1-over-TLS and h2 code paths.
use super::passthrough::{PassthroughCapturingBody, PassthroughMeta};

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
                        urlencoding::decode(k)
                            .map(|s| s.into_owned())
                            .unwrap_or_else(|_| k.to_string()),
                        urlencoding::decode(v)
                            .map(|s| s.into_owned())
                            .unwrap_or_else(|_| v.to_string()),
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
