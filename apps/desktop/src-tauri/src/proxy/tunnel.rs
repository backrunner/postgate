use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::plugin::{PluginRequest, PluginRequestContext, PluginResponse};
use crate::proxy::body::{collect_body, CapturedBody, RequestCapturingBody, MAX_BODY_SIZE};
use crate::proxy::error_page::ProxyErrorKind;
use crate::proxy::forward::{
    apply_request_url_modifications, path_and_query_from_url, ForwardTarget,
};
use crate::proxy::handler::{prefetch_response_remote_resources, ProxyContext};
use crate::proxy::headers::{
    apply_headers_to_response_builder, build_forward_request_headers,
    build_forward_response_headers, flat_to_headermap, headermap_to_flat,
    sync_request_body_headers,
};
use crate::proxy::websocket;
use crate::rules::{
    apply_request_rules_with_values, apply_response_rules_with_values, feature, is_enabled,
    persist_request_writes, persist_response_writes, remote_resource_urls_for_request,
    rules_may_short_circuit_request, rules_require_request_body, RequestWriteContext, ResolveCtx,
    ResolvedResources, ResponseModification, ResponseWriteContext,
};
use crate::state::{CapturedRequestData, CapturedRequestEvent, RequestEventType};
use crate::values::RequestCtx;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::HeaderMap;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
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
    client_ip: String,
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
        super::http2::handle_http2_connection(tls_stream, host, port, ctx, tls_version, client_ip)
            .await
    } else {
        let io = TokioIo::new(tls_stream);
        let ctx_clone = ctx.clone();
        let client_ip = client_ip.clone();

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
                    let client_ip = client_ip.clone();
                    async move {
                        handle_https_request(req, &host, port, ctx, &tls_ver, &client_ip).await
                    }
                }),
            )
            .with_upgrades()
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
    client_ip: &str,
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
    //
    // Preserve the original `HeaderMap` so multi-value entries (cookies
    // sent as separate headers over h2-then-downgraded, repeated `via`,
    // etc.) can be replayed upstream with their original cardinality.
    // The flat `HashMap` is the lossy view used for rule matching / UI.
    let original_request_headers: HeaderMap = req.headers().clone();
    let request_headers: HashMap<String, String> = headermap_to_flat(&original_request_headers);

    let content_type = request_headers.get("content-type").cloned();
    let is_websocket = websocket::is_websocket_upgrade(&request_headers);

    // Match rules - now returns MatchedRule with remaining_path
    let matched_rules = ctx.rule_engine.match_request_with_client_ip(
        &method_str,
        host,
        &path,
        if is_websocket { "wss" } else { "https" },
        port,
        &request_headers,
        Some(client_ip),
    );
    let matched_rule_ids: Vec<String> = matched_rules
        .iter()
        .map(|r| r.rule.raw_line.clone())
        .collect();

    if is_websocket {
        let target_url = websocket::build_ws_url(host, port, &path, true);

        let _ = ctx.app_state.ensure_values_loaded().await;
        let query_map = super::handler::tunnel_value_helpers::query_map(&target_url);
        let cookie_map = super::handler::tunnel_value_helpers::cookie_map(&request_headers);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let values_ctx = RequestCtx {
            url: &target_url,
            method: &method_str,
            client_ip,
            req_headers: &request_headers,
            query: &query_map,
            req_cookies: &cookie_map,
            now_ms,
        };
        let remote_resources = ResolvedResources::new();
        let request_modification = {
            let resolve_ctx = ResolveCtx {
                store: Some(&ctx.app_state.values_store),
                ctx: Some(&values_ctx),
                remote_resources: Some(&remote_resources),
            };
            apply_request_rules_with_values(
                &matched_rules,
                &target_url,
                &method_str,
                &request_headers,
                None,
                &resolve_ctx,
            )
        };

        if crate::rules::should_abort(&request_modification) {
            return Ok(Response::builder()
                .status(444)
                .header("connection", "close")
                .body(Full::new(Bytes::new()).map_err(|_| unreachable!()).boxed())
                .unwrap());
        }

        if let Some(delay_ms) = request_modification.delay_ms {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
        }

        let capture = crate::rules::capture_enabled(&request_modification);
        let meta = websocket::WebSocketCaptureMeta {
            request_id,
            timestamp,
            method: method_str,
            url: target_url.clone(),
            host: host.to_string(),
            path,
            request_headers: request_headers.clone(),
            matched_rules: matched_rule_ids,
            tls_version: Some(tls_version.to_string()),
            remote_addr: Some(client_ip.to_string()),
            capture,
        };

        return match websocket::handle_hyper_upgrade(
            req,
            target_url,
            original_request_headers,
            ctx.app_state.clone(),
            meta,
        )
        .await
        {
            Ok(response) => Ok(response),
            Err(e) => Ok(html_error_response(
                502,
                ProxyErrorKind::Upstream,
                &e.to_string(),
            )),
        };
    }

    // Buffer only when request-stage actions need body bytes. Otherwise the
    // client body stays streaming all the way into the pooled upstream client.
    let (parts, body) = req.into_parts();
    let buffer_request_body = rules_require_request_body(&matched_rules)
        || rules_may_short_circuit_request(&matched_rules);
    let (request_body, streaming_body) = if buffer_request_body {
        let request_body = match collect_body(body, MAX_BODY_SIZE).await {
            Ok(body) => body,
            Err(error) => {
                tracing::error!("Failed to collect request body: {error}");
                return Ok(html_error_response(
                    502,
                    ProxyErrorKind::Request,
                    &format!("Failed to read request body: {error}"),
                ));
            }
        };
        (Some(request_body), None)
    } else {
        (None, Some(body))
    };

    let request_size = Arc::new(AtomicU64::new(
        request_body
            .as_ref()
            .map(|body| body.size as u64)
            .unwrap_or(0),
    ));

    // Build resolver context for whistle `{name}` references.
    let _ = ctx.app_state.ensure_values_loaded().await;
    let query_map = super::handler::tunnel_value_helpers::query_map(&url);
    let cookie_map = super::handler::tunnel_value_helpers::cookie_map(&request_headers);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let values_ctx = RequestCtx {
        url: &url,
        method: &method_str,
        client_ip,
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

    // Apply request rules to get modifications
    let mut request_modification = {
        let resolve_ctx = ResolveCtx {
            store: Some(&ctx.app_state.values_store),
            ctx: Some(&values_ctx),
            remote_resources: Some(&remote_resources),
        };
        apply_request_rules_with_values(
            &matched_rules,
            &url,
            &method_str,
            &request_headers,
            request_body.as_ref().map(|body| &body.data),
            &resolve_ctx,
        )
    };

    // `enable://abort` — drop the HTTPS connection without responding. The
    // browser will see a closed TLS tunnel and report ERR_CONNECTION_CLOSED
    // or similar, which matches whistle's abort semantics.
    if crate::rules::should_abort(&request_modification) {
        tracing::debug!("abort:// matched for {}; closing h1-TLS stream", url);
        return Ok(Response::builder()
            .status(444)
            .header("connection", "close")
            .body(Full::new(Bytes::new()).map_err(|_| unreachable!()).boxed())
            .unwrap());
    }

    let capture = crate::rules::capture_enabled(&request_modification);
    let force_res_write = is_enabled(&request_modification, feature::FORCE_RES_WRITE);

    let final_method = request_modification
        .method
        .as_deref()
        .and_then(|m| Method::from_bytes(m.as_bytes()).ok())
        .unwrap_or_else(|| method.clone());
    let final_method_str = final_method.to_string();

    let mut body_to_send = request_modification
        .body
        .clone()
        .or_else(|| request_body.as_ref().map(|body| body.data.clone()))
        .unwrap_or_default();
    if let Some(speed_kbps) = request_modification.speed_kbps {
        body_to_send = super::throttle::apply_throttle(body_to_send, Some(speed_kbps)).await;
    }
    if let Some(request_body) = &request_body {
        sync_request_body_headers(
            &mut request_modification.headers,
            &request_body.data,
            &body_to_send,
        );
    }

    let base_absolute_url: std::result::Result<String, PostGateError> =
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

    let absolute_url = base_absolute_url.and_then(|base_url| {
        apply_request_url_modifications(
            &base_url,
            request_modification.path.as_deref(),
            request_modification.query_params.as_deref(),
        )
    });
    let display_url = absolute_url.as_ref().cloned().unwrap_or_else(|e| {
        tracing::error!("Failed to apply HTTPS URL modifications for {}: {}", url, e);
        url.clone()
    });
    let display_path = path_and_query_from_url(&display_url).unwrap_or_else(|| path.clone());

    // Build the upstream HeaderMap once the final URL/body are known so
    // reqWrite:// also fires for short-circuit/plugin responses.
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
            url: &display_url,
            headers: &forward_headers,
            body: &body_to_send,
            force: is_enabled(&request_modification, feature::FORCE_REQ_WRITE),
        },
    );

    if let (true, Some(request_body)) = (capture, request_body.as_ref()) {
        ctx.body_storage
            .store_request_body(&request_id, request_body.clone())
            .await;
        ctx.app_state
            .persist_body(request_id.clone(), request_body.capture_bytes(), true);
    }

    if capture {
        ctx.app_state.emit_request_event(&CapturedRequestEvent {
            id: request_id.clone(),
            event_type: RequestEventType::Started,
            data: CapturedRequestData {
                id: request_id.clone(),
                timestamp,
                method: final_method_str.clone(),
                url: display_url.clone(),
                host: host.to_string(),
                path: display_path.clone(),
                request_headers: Some(request_headers.clone()),
                protocol: "https".to_string(),
                content_type: content_type.clone(),
                matched_rules: matched_rule_ids.clone(),
                tls_version: Some(tls_version.to_string()),
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
            &display_url,
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
                url: &display_url,
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
                .persist_body(request_id.clone(), response_body.capture_bytes(), false);

            ctx.app_state.emit_request_event(&CapturedRequestEvent {
                id: request_id.clone(),
                event_type: RequestEventType::Completed,
                data: CapturedRequestData {
                    id: request_id,
                    timestamp,
                    method: final_method_str,
                    url: display_url,
                    host: host.to_string(),
                    path: display_path,
                    request_headers: Some(request_headers),
                    response_status: Some(final_response.status),
                    response_headers: Some(final_response.flat_headers.clone()),
                    duration_ms: Some(duration),
                    matched_rules: matched_rule_ids,
                    protocol: "https".to_string(),
                    content_type: final_response.flat_headers.get("content-type").cloned(),
                    request_size: request_size.load(Ordering::Relaxed),
                    response_size: Some(final_response.body.len() as u64),
                    tls_version: Some(tls_version.to_string()),
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
            url: display_url.clone(),
            host: host.to_string(),
            path: display_path.clone(),
            query: extract_query_params(&display_url),
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
            matched_pattern: display_url.clone(),
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
                    &display_url,
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
                        url: &display_url,
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

                if capture {
                    let response_body = CapturedBody {
                        data: final_response.body.clone(),
                        size: final_response.body.len(),
                        truncated: false,
                    };
                    ctx.body_storage
                        .store_response_body(&request_id, response_body.clone())
                        .await;
                    ctx.app_state.persist_body(
                        request_id.clone(),
                        response_body.capture_bytes(),
                        false,
                    );

                    ctx.app_state.emit_request_event(&CapturedRequestEvent {
                        id: request_id.clone(),
                        event_type: RequestEventType::Completed,
                        data: CapturedRequestData {
                            id: request_id,
                            timestamp,
                            method: final_method_str,
                            url: display_url,
                            host: host.to_string(),
                            path: display_path,
                            request_headers: Some(request_headers),
                            response_status: Some(final_response.status),
                            response_headers: Some(final_response.flat_headers.clone()),
                            duration_ms: Some(duration),
                            matched_rules: matched_rule_ids,
                            protocol: "https".to_string(),
                            content_type: final_response.flat_headers.get("content-type").cloned(),
                            request_size: request_size.load(Ordering::Relaxed),
                            response_size: Some(final_response.body.len() as u64),
                            tls_version: Some(tls_version.to_string()),
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

    let mut upstream_body = Some(match streaming_body {
        Some(body) => RequestCapturingBody::new(
            body,
            request_id.clone(),
            ctx.app_state.clone(),
            ctx.body_storage.clone(),
            request_size.clone(),
            capture,
        )
        .boxed(),
        None => Full::new(body_to_send.clone())
            .map_err(|_| unreachable!())
            .boxed(),
    });

    // Decide whether to buffer the response body. Streaming is the common
    // case (no resBody://, no htmlAppend://, no debug, no plugin) and matches
    // whistle's default behaviour — TTFB tracks upstream directly.
    let buffer_response_body = crate::rules::rules_require_response_body(&matched_rules);

    // --- Streaming path ---------------------------------------------------
    // Streaming bypass requires the shared pooled client; chained proxy
    // routes don't go through it, so they always take the buffering path.
    if !buffer_response_body && request_modification.upstream_proxy.is_none() {
        let stream_result = match absolute_url {
            Ok(target_url) => {
                super::upstream::forward_stream_headermap_body(
                    &ctx.upstream_client,
                    final_method.clone(),
                    &target_url,
                    &forward_header_map,
                    upstream_body.take().expect("request body available"),
                    request_modification.timeout_ms,
                )
                .await
            }
            Err(e) => Err(e),
        };

        return match stream_result {
            Ok((parts, body)) => {
                let status = parts.status.as_u16();
                let original_resp_headers: HeaderMap = parts.headers.clone();
                let upstream_headers: HashMap<String, String> =
                    headermap_to_flat(&original_resp_headers);

                let response_content_type = upstream_headers.get("content-type").cloned();
                prefetch_response_remote_resources(
                    &ctx,
                    &mut remote_resources,
                    &matched_rules,
                    &display_url,
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
                    &display_url,
                    &final_method_str,
                    &request_headers,
                    &upstream_headers,
                    status,
                    None,
                    response_content_type.as_deref(),
                    &resolve_ctx,
                );
                let final_status = response_modification.status_code.unwrap_or(status);
                // HeaderMap-preserving finalization — multi-value Set-Cookie
                // from upstream survives intact.
                let final_header_map: HeaderMap = build_forward_response_headers(
                    &original_resp_headers,
                    &upstream_headers,
                    &response_modification,
                    None,
                );
                let final_headers = headermap_to_flat(&final_header_map);

                // resDelay:// — delay before first byte.
                if let Some(delay_ms) = response_modification.delay_ms {
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }

                let wrapped = super::passthrough::PassthroughCapturingBody::new(
                    body,
                    super::passthrough::PassthroughMeta {
                        request_id: request_id.clone(),
                        timestamp,
                        method: final_method_str.clone(),
                        url: display_url.clone(),
                        host: host.to_string(),
                        path: display_path.clone(),
                        request_headers: request_headers.clone(),
                        response_status: final_status,
                        response_headers: final_headers.clone(),
                        matched_rules: matched_rule_ids.clone(),
                        protocol: "https".to_string(),
                        content_type: response_content_type,
                        request_size: request_size.load(Ordering::Relaxed),
                        start_time,
                        persistence_enabled: ctx.app_state.is_persistence_enabled(),
                        tls_version: Some(tls_version.to_string()),
                        capture,
                    },
                    ctx.app_state.clone(),
                    ctx.body_storage.clone(),
                );

                let builder = Response::builder().status(final_status);
                let builder = apply_headers_to_response_builder(builder, &final_header_map);
                Ok(builder.body(wrapped.boxed()).unwrap())
            }
            Err(e) => {
                let duration = start_time.elapsed().as_millis() as u64;
                tracing::error!("Forward error: {}", e);

                if capture {
                    ctx.app_state.emit_request_event(&CapturedRequestEvent {
                        id: request_id.clone(),
                        event_type: RequestEventType::Error,
                        data: CapturedRequestData {
                            id: request_id,
                            timestamp,
                            method: final_method_str,
                            url: display_url,
                            host: host.to_string(),
                            path: display_path,
                            request_headers: Some(request_headers),
                            duration_ms: Some(duration),
                            matched_rules: matched_rule_ids,
                            protocol: "https".to_string(),
                            request_size: request_size.load(Ordering::Relaxed),
                            error: Some(e.to_string()),
                            tls_version: Some(tls_version.to_string()),
                            ..Default::default()
                        },
                    });
                }

                Ok(html_error_response(
                    502,
                    ProxyErrorKind::Upstream,
                    &e.to_string(),
                ))
            }
        };
    }

    // --- Buffering path (body-modifying rules) ---------------------------
    let forward_result = match absolute_url {
        Ok(url) => {
            if let Some(proxy) = request_modification.upstream_proxy.as_ref() {
                // Chained proxies still run through the flat-map path. Use
                // our multi-value-aware flattener so cookies don't collapse.
                let flat_for_chain = headermap_to_flat(&forward_header_map);
                super::upstream::forward_collect_with_proxy(
                    &ctx.upstream_client,
                    final_method.clone(),
                    &url,
                    &flat_for_chain,
                    body_to_send.clone(),
                    request_modification.timeout_ms,
                    Some(proxy),
                )
                .await
            } else {
                super::upstream::forward_collect_headermap_body(
                    &ctx.upstream_client,
                    final_method.clone(),
                    &url,
                    &forward_header_map,
                    upstream_body.take().expect("request body available"),
                    request_modification.timeout_ms,
                )
                .await
            }
        }
        Err(e) => Err(e),
    };

    match forward_result {
        Ok((resp, response_body)) => {
            let status = resp.status().as_u16();
            let original_resp_headers: HeaderMap = resp.headers().clone();
            let upstream_headers: HashMap<String, String> =
                headermap_to_flat(&original_resp_headers);

            let response_content_type = upstream_headers.get("content-type").cloned();

            // Apply response rules
            prefetch_response_remote_resources(
                &ctx,
                &mut remote_resources,
                &matched_rules,
                &display_url,
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
                &display_url,
                &final_method_str,
                &request_headers,
                &upstream_headers,
                status,
                Some(&response_body.data),
                response_content_type.as_deref(),
                &resolve_ctx,
            );

            let (plugin_modified_body, plugin_modified_headers) =
                if let Some(ref plugin_info) = request_modification.plugin {
                    let plugin_request = PluginRequest {
                        id: request_id.clone(),
                        method: final_method_str.clone(),
                        url: display_url.clone(),
                        host: host.to_string(),
                        path: display_path.clone(),
                        query: extract_query_params(&display_url),
                        headers: request_headers.clone(),
                        body: Some(base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            &body_to_send,
                        )),
                        body_base64: true,
                        timestamp,
                    };

                    let plugin_response = PluginResponse {
                        status,
                        headers: upstream_headers.clone(),
                        body: Some(base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            &response_body.data,
                        )),
                        body_base64: true,
                    };

                    let plugin_context = PluginRequestContext {
                        rule_config: match &plugin_info.config {
                            serde_json::Value::Object(map) => {
                                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                            }
                            _ => std::collections::HashMap::new(),
                        },
                        matched_pattern: display_url.clone(),
                    };

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

            // resDelay:// — applies before sending first byte back.
            if let Some(delay_ms) = response_modification.delay_ms {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            let mut final_body = plugin_modified_body
                .or(response_modification.body.clone())
                .unwrap_or(response_body.data.clone());
            let plugin_replaced_headers = plugin_modified_headers.is_some();
            let headers_source =
                plugin_modified_headers.unwrap_or_else(|| response_modification.headers.clone());

            // debug:// — inject CDP bridge into HTML responses.
            if response_modification.inject_debug {
                let injector = crate::debug::ScriptInjector::new(
                    ctx.app_state.debug_port_for_injection().await,
                );
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
            // HeaderMap-preserving finalization — upstream multi-value
            // Set-Cookie stays intact, resCookies:// entries are appended as
            // their own header lines.
            let mut modified_for_finalize = ResponseModification {
                headers: headers_source,
                headers_to_remove: response_modification.headers_to_remove.clone(),
                cookies: response_modification.cookies.clone(),
                ..Default::default()
            };
            modified_for_finalize.status_code = response_modification.status_code;
            let (base_headers, base_flat): (HeaderMap, HashMap<String, String>) =
                if plugin_replaced_headers {
                    let hm = flat_to_headermap(&modified_for_finalize.headers);
                    let flat = modified_for_finalize.headers.clone();
                    (hm, flat)
                } else {
                    (original_resp_headers.clone(), upstream_headers.clone())
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
                    status: final_status,
                    headers: &final_headers,
                    body: &final_body,
                    force: is_enabled(&request_modification, feature::FORCE_RES_WRITE),
                },
            );

            let response_size = final_body.len() as u64;
            let duration = start_time.elapsed().as_millis() as u64;

            let final_body_captured = CapturedBody {
                data: final_body.clone(),
                size: final_body.len(),
                truncated: false,
            };
            if capture {
                ctx.body_storage
                    .store_response_body(&request_id, final_body_captured)
                    .await;
                ctx.app_state.persist_body(
                    request_id.clone(),
                    crate::proxy::body::capture_bytes(&final_body),
                    false,
                );

                ctx.app_state.emit_request_event(&CapturedRequestEvent {
                    id: request_id.clone(),
                    event_type: RequestEventType::Completed,
                    data: CapturedRequestData {
                        id: request_id,
                        timestamp,
                        method: final_method_str,
                        url: display_url,
                        host: host.to_string(),
                        path: display_path,
                        request_headers: Some(request_headers),
                        response_status: Some(final_status),
                        response_headers: Some(final_headers.clone()),
                        duration_ms: Some(duration),
                        matched_rules: matched_rule_ids,
                        protocol: "https".to_string(),
                        content_type: final_headers.get("content-type").cloned(),
                        request_size: request_size.load(Ordering::Relaxed),
                        response_size: Some(response_size),
                        tls_version: Some(tls_version.to_string()),
                        ..Default::default()
                    },
                });
            }

            // Build final response with multi-value-preserving headers.
            let builder = Response::builder().status(final_status);
            let builder = apply_headers_to_response_builder(builder, &final_header_map);

            Ok(builder
                .body(Full::new(final_body).map_err(|_| unreachable!()).boxed())
                .unwrap())
        }
        Err(e) => {
            let duration = start_time.elapsed().as_millis() as u64;
            tracing::error!("Forward error: {}", e);

            if capture {
                ctx.app_state.emit_request_event(&CapturedRequestEvent {
                    id: request_id.clone(),
                    event_type: RequestEventType::Error,
                    data: CapturedRequestData {
                        id: request_id,
                        timestamp,
                        method: final_method_str,
                        url: display_url,
                        host: host.to_string(),
                        path: display_path,
                        request_headers: Some(request_headers),
                        duration_ms: Some(duration),
                        matched_rules: matched_rule_ids,
                        protocol: "https".to_string(),
                        request_size: request_size.load(Ordering::Relaxed),
                        error: Some(e.to_string()),
                        tls_version: Some(tls_version.to_string()),
                        ..Default::default()
                    },
                });
            }

            Ok(html_error_response(
                502,
                ProxyErrorKind::Upstream,
                &e.to_string(),
            ))
        }
    }
}

fn html_error_response(
    status: u16,
    kind: ProxyErrorKind,
    message: &str,
) -> Response<BoxBody<Bytes, hyper::Error>> {
    let body = crate::proxy::error_page::html_error_body(status, kind, message);
    let headers = crate::proxy::error_page::html_error_headers(body.len());
    let mut builder = Response::builder().status(status);
    for (key, value) in headers {
        builder = builder.header(key, value);
    }
    builder
        .body(Full::new(body).map_err(|_| unreachable!()).boxed())
        .unwrap()
}

fn extract_query_params(uri: &str) -> HashMap<String, String> {
    url::Url::parse(uri)
        .map(|u| {
            u.query_pairs()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default()
}
