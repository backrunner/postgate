//! HTTP/2 support for proxy connections
//!
//! This module provides HTTP/2 server-side handling for MITM'd TLS tunnels.
//!
//! It mirrors the HTTP/1.1-over-TLS path in `tunnel.rs`: rule matching, value
//! resolution, short-circuit responses, plugin `handleRequest` /
//! `handleResponse`, debug script injection and response body/header rewriting
//! all apply here too — otherwise ALPN-picked h2 silently bypasses the entire
//! PostGate feature set.

use crate::debug::ScriptInjector;
use crate::error::{PostGateError, Result};
use crate::plugin::{PluginRequest, PluginRequestContext, PluginResponse};
use crate::proxy::body::{CapturedBody, MAX_BODY_SIZE};
use crate::proxy::error_page::ProxyErrorKind;
use crate::proxy::forward::{
    apply_request_url_modifications, path_and_query_from_url, ForwardTarget,
};
use crate::proxy::handler::{prefetch_response_remote_resources, ProxyContext};
use crate::proxy::headers::{
    apply_headers_to_response_builder_h2, build_forward_request_headers,
    build_forward_response_headers, flat_to_headermap, headermap_to_flat,
    sync_request_body_headers,
};
use crate::rules::{
    apply_request_rules_with_values, apply_response_rules_with_values, feature, is_enabled,
    persist_request_writes, persist_response_writes, remote_resource_urls_for_request,
    RequestWriteContext, ResolveCtx, ResolvedResources, ResponseWriteContext,
};
use crate::state::{CapturedRequestData, CapturedRequestEvent, RequestEventType};
use crate::values::RequestCtx;
use bytes::Bytes;
use h2::server::SendResponse;
use h2::RecvStream;
use hyper::{HeaderMap, Method, Request, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

/// Check if a connection should use HTTP/2
pub fn should_use_http2(alpn: Option<&[u8]>) -> bool {
    alpn.map(|p| p == b"h2").unwrap_or(false)
}

/// HTTP/2 forbids a handful of hop-by-hop request / response headers
/// (RFC 7540 §8.1.2.2). Sending them upstream or back to the client yields
/// stream resets or GOAWAY frames.
fn is_h2_forbidden_header(name: &str) -> bool {
    matches!(
        name,
        "connection" | "keep-alive" | "proxy-connection" | "transfer-encoding" | "upgrade" | "host" // h2 uses :authority instead
    )
}

/// `TE` may only appear with the single value `trailers` in HTTP/2.
fn sanitize_te_header(value: &str) -> Option<&str> {
    if value.eq_ignore_ascii_case("trailers") {
        Some("trailers")
    } else {
        None
    }
}

/// Handle an HTTP/2 connection from client
pub async fn handle_http2_connection<S>(
    stream: S,
    host: String,
    port: u16,
    ctx: Arc<ProxyContext>,
    tls_version: String,
    client_ip: String,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut connection = h2::server::handshake(stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("H2 handshake error: {}", e)))?;

    while let Some(result) = connection.accept().await {
        let (request, respond) =
            result.map_err(|e| PostGateError::Proxy(format!("H2 accept error: {}", e)))?;

        let host = host.clone();
        let ctx = ctx.clone();
        let tls_ver = tls_version.clone();
        let client_ip = client_ip.clone();

        tokio::spawn(async move {
            if let Err(e) =
                handle_http2_request(request, respond, &host, port, ctx, &tls_ver, &client_ip).await
            {
                tracing::error!("HTTP/2 request error: {}", e);
            }
        });
    }

    Ok(())
}

/// Handle a single HTTP/2 request through the full PostGate pipeline.
async fn handle_http2_request(
    request: Request<RecvStream>,
    mut respond: SendResponse<Bytes>,
    host: &str,
    port: u16,
    ctx: Arc<ProxyContext>,
    tls_version: &str,
    client_ip: &str,
) -> Result<()> {
    let request_id = Uuid::new_v4().to_string();
    let start_time = std::time::Instant::now();
    let timestamp = chrono::Utc::now().timestamp_millis();

    let method = request.method().clone();
    let method_str = method.to_string();
    let path = request
        .uri()
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());

    // Build URL — omit default port 443 for cleaner display (matches tunnel.rs).
    let url = if port == 443 {
        format!("https://{}{}", host, path)
    } else {
        format!("https://{}:{}{}", host, port, path)
    };

    // Extract headers (lowercased, h2 has these in `:authority` / `:path` etc.)
    //
    // Preserve the original `HeaderMap`. HTTP/2 browsers routinely split
    // cookies into one header per crumble (RFC 7540 §8.1.2.5) — flattening
    // to a `HashMap` here would discard every crumble except the last,
    // which is the exact failure mode users hit as "logged out after
    // going through the proxy". See `build_forward_request_headers`.
    let original_request_headers: HeaderMap = request.headers().clone();
    let request_headers: HashMap<String, String> = headermap_to_flat(&original_request_headers);

    let content_type = request_headers.get("content-type").cloned();

    // Match rules
    let matched_rules = ctx.rule_engine.match_request_with_client_ip(
        &method_str,
        host,
        &path,
        "https",
        port,
        &request_headers,
        Some(client_ip),
    );
    let matched_rule_ids: Vec<String> = matched_rules
        .iter()
        .map(|r| r.rule.raw_line.clone())
        .collect();

    // Collect request body from H2 stream
    let (parts, body) = request.into_parts();
    let request_body = collect_h2_body(body).await?;
    let request_size = request_body.size as u64;

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

    // Apply request rules
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
            Some(&request_body.data),
            &resolve_ctx,
        )
    };

    // `enable://abort` — tear down the h2 stream. Returning from the handler
    // closes the stream without sending a response.
    if crate::rules::should_abort(&request_modification) {
        tracing::debug!("abort:// matched for {}; closing h2 stream", url);
        respond.send_reset(h2::Reason::CANCEL);
        return Ok(());
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
        .unwrap_or(request_body.data.clone());
    if let Some(speed_kbps) = request_modification.speed_kbps {
        body_to_send = super::throttle::apply_throttle(body_to_send, Some(speed_kbps)).await;
    }
    sync_request_body_headers(
        &mut request_modification.headers,
        &request_body.data,
        &body_to_send,
    );

    let base_absolute_url: std::result::Result<String, PostGateError> =
        if let Some(target_host) = &request_modification.target_host {
            let remaining_path = request_modification.remaining_path.as_deref().unwrap_or("");
            match ForwardTarget::parse(target_host, remaining_path, "https") {
                Ok(target) => {
                    tracing::debug!(
                        "[h2] Forwarding {} to {} (remaining: {})",
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
        tracing::error!("Failed to apply h2 URL modifications for {}: {}", url, e);
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

    if capture {
        ctx.body_storage
            .store_request_body(&request_id, request_body.clone())
            .await;
        ctx.app_state
            .persist_body(request_id.clone(), request_body.capture_bytes(), true);

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
                protocol: "h2".to_string(),
                content_type: content_type.clone(),
                matched_rules: matched_rule_ids.clone(),
                tls_version: Some(tls_version.to_string()),
                ..Default::default()
            },
        });
    }

    // Handle short-circuit responses (redirect / file / statusCode / ...)
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
                    id: request_id.clone(),
                    timestamp,
                    method: final_method_str.clone(),
                    url: display_url.clone(),
                    host: host.to_string(),
                    path: display_path.clone(),
                    request_headers: Some(request_headers.clone()),
                    response_status: Some(final_response.status),
                    response_headers: Some(final_response.flat_headers.clone()),
                    duration_ms: Some(duration),
                    matched_rules: matched_rule_ids.clone(),
                    protocol: "h2".to_string(),
                    content_type: final_response.flat_headers.get("content-type").cloned(),
                    request_size,
                    response_size: Some(final_response.body.len() as u64),
                    tls_version: Some(tls_version.to_string()),
                    ..Default::default()
                },
            });
        }

        send_h2_response_headermap(
            &mut respond,
            final_response.status,
            &final_response.headers,
            final_response.body,
        )?;
        return Ok(());
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
                            id: request_id.clone(),
                            timestamp,
                            method: final_method_str.clone(),
                            url: display_url.clone(),
                            host: host.to_string(),
                            path: display_path.clone(),
                            request_headers: Some(request_headers.clone()),
                            response_status: Some(final_response.status),
                            response_headers: Some(final_response.flat_headers.clone()),
                            duration_ms: Some(duration),
                            matched_rules: matched_rule_ids.clone(),
                            protocol: "h2".to_string(),
                            content_type: final_response.flat_headers.get("content-type").cloned(),
                            request_size,
                            response_size: Some(final_response.body.len() as u64),
                            tls_version: Some(tls_version.to_string()),
                            ..Default::default()
                        },
                    });
                }

                send_h2_response_headermap(
                    &mut respond,
                    final_response.status,
                    &final_response.headers,
                    final_response.body,
                )?;
                return Ok(());
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

    // Streaming path: no rule rewrites the body — pump bytes to the h2 client
    // as they arrive instead of waiting for the full upstream body. This is
    // the main TTFB lever.
    let buffer_response_body = crate::rules::rules_require_response_body(&matched_rules);

    if !buffer_response_body && request_modification.upstream_proxy.is_none() {
        let stream_result = match absolute_url {
            Ok(target_url) => {
                super::upstream::forward_stream_headermap(
                    &ctx.upstream_client,
                    final_method.clone(),
                    &target_url,
                    &forward_header_map,
                    body_to_send.clone(),
                    request_modification.timeout_ms,
                )
                .await
            }
            Err(e) => Err(e),
        };

        match stream_result {
            Ok((up_parts, up_body)) => {
                let status = up_parts.status.as_u16();
                let original_resp_headers: HeaderMap = up_parts.headers.clone();
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
                let final_header_map: HeaderMap = build_forward_response_headers(
                    &original_resp_headers,
                    &upstream_headers,
                    &response_modification,
                    None,
                );
                let final_headers = headermap_to_flat(&final_header_map);

                if let Some(delay_ms) = response_modification.delay_ms {
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }

                let builder = Response::builder().status(final_status);
                let builder = apply_headers_to_response_builder_h2(builder, &final_header_map);
                let h2_response = builder.body(()).map_err(|e| {
                    PostGateError::Proxy(format!("Failed to build response: {}", e))
                })?;
                let mut send_stream = respond.send_response(h2_response, false).map_err(|e| {
                    PostGateError::Proxy(format!("Failed to send h2 headers: {}", e))
                })?;

                let pump_meta = super::passthrough::PassthroughMeta {
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
                    protocol: "h2".to_string(),
                    content_type: response_content_type,
                    request_size,
                    start_time,
                    persistence_enabled: ctx.app_state.is_persistence_enabled(),
                    tls_version: Some(tls_version.to_string()),
                    capture,
                };
                pump_incoming_into_h2(
                    up_body,
                    &mut send_stream,
                    pump_meta,
                    ctx.app_state.clone(),
                    ctx.body_storage.clone(),
                )
                .await?;

                return Ok(());
            }
            Err(e) => {
                let duration = start_time.elapsed().as_millis() as u64;
                tracing::error!("HTTP/2 forward error: {}", e);
                if capture {
                    ctx.app_state.emit_request_event(&CapturedRequestEvent {
                        id: request_id.clone(),
                        event_type: RequestEventType::Error,
                        data: CapturedRequestData {
                            id: request_id.clone(),
                            timestamp,
                            method: final_method_str.clone(),
                            url: display_url.clone(),
                            host: host.to_string(),
                            path: display_path.clone(),
                            request_headers: Some(request_headers.clone()),
                            duration_ms: Some(duration),
                            matched_rules: matched_rule_ids.clone(),
                            protocol: "h2".to_string(),
                            request_size,
                            error: Some(e.to_string()),
                            tls_version: Some(tls_version.to_string()),
                            ..Default::default()
                        },
                    });
                }

                let _ = send_h2_error_response(
                    &mut respond,
                    502,
                    ProxyErrorKind::Upstream,
                    &e.to_string(),
                );
                return Ok(());
            }
        }
    }

    // Buffering path (rules rewrite the body)
    let forward_result = match absolute_url {
        Ok(absolute) => {
            if let Some(proxy) = request_modification.upstream_proxy.as_ref() {
                let flat_for_chain = headermap_to_flat(&forward_header_map);
                super::upstream::forward_collect_with_proxy(
                    &ctx.upstream_client,
                    final_method.clone(),
                    &absolute,
                    &flat_for_chain,
                    body_to_send.clone(),
                    request_modification.timeout_ms,
                    Some(proxy),
                )
                .await
            } else {
                super::upstream::forward_collect_headermap(
                    &ctx.upstream_client,
                    final_method.clone(),
                    &absolute,
                    &forward_header_map,
                    body_to_send.clone(),
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

            let (plugin_modified_body, plugin_modified_headers, plugin_modified_status) =
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
                            (decoded_body, Some(modified.headers), Some(modified.status))
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Plugin handleResponse failed for {}: {}",
                                plugin_info.name,
                                e
                            );
                            (None, None, None)
                        }
                    }
                } else {
                    (None, None, None)
                };

            if let Some(delay_ms) = response_modification.delay_ms {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            let final_status = plugin_modified_status
                .or(response_modification.status_code)
                .unwrap_or(status);
            let mut final_body = plugin_modified_body
                .or(response_modification.body.clone())
                .unwrap_or(response_body.data.clone());
            let plugin_replaced_headers = plugin_modified_headers.is_some();
            let headers_source =
                plugin_modified_headers.unwrap_or_else(|| response_modification.headers.clone());

            if response_modification.inject_debug {
                let injector = ScriptInjector::new(ctx.app_state.debug_port_for_injection().await);
                if let Ok(html) = String::from_utf8(final_body.to_vec()) {
                    if !ScriptInjector::is_already_injected(&html) {
                        final_body = Bytes::from(injector.inject_into_html(&html));
                    }
                }
            }

            if let Some(speed_kbps) = response_modification.speed_kbps {
                final_body = super::throttle::apply_throttle(final_body, Some(speed_kbps)).await;
            }
            let body_was_modified = final_body != response_body.data;

            let mut modified_for_finalize = crate::rules::ResponseModification {
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

            let final_size = final_body.len() as u64;
            let final_duration = start_time.elapsed().as_millis() as u64;

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
                    .persist_body(request_id.clone(), stored_body.capture_bytes(), false);

                ctx.app_state.emit_request_event(&CapturedRequestEvent {
                    id: request_id.clone(),
                    event_type: RequestEventType::Completed,
                    data: CapturedRequestData {
                        id: request_id.clone(),
                        timestamp,
                        method: final_method_str.clone(),
                        url: display_url.clone(),
                        host: host.to_string(),
                        path: display_path.clone(),
                        request_headers: Some(request_headers.clone()),
                        response_status: Some(final_status),
                        response_headers: Some(final_headers.clone()),
                        duration_ms: Some(final_duration),
                        matched_rules: matched_rule_ids.clone(),
                        protocol: "h2".to_string(),
                        content_type: final_headers.get("content-type").cloned(),
                        request_size,
                        response_size: Some(final_size),
                        tls_version: Some(tls_version.to_string()),
                        ..Default::default()
                    },
                });
            }

            send_h2_response_headermap(&mut respond, final_status, &final_header_map, final_body)?;
        }
        Err(e) => {
            let duration = start_time.elapsed().as_millis() as u64;
            tracing::error!("HTTP/2 forward error: {}", e);

            if capture {
                ctx.app_state.emit_request_event(&CapturedRequestEvent {
                    id: request_id.clone(),
                    event_type: RequestEventType::Error,
                    data: CapturedRequestData {
                        id: request_id.clone(),
                        timestamp,
                        method: final_method_str.clone(),
                        url: display_url.clone(),
                        host: host.to_string(),
                        path: display_path.clone(),
                        request_headers: Some(request_headers.clone()),
                        duration_ms: Some(duration),
                        matched_rules: matched_rule_ids.clone(),
                        protocol: "h2".to_string(),
                        request_size,
                        error: Some(e.to_string()),
                        tls_version: Some(tls_version.to_string()),
                        ..Default::default()
                    },
                });
            }

            let _ =
                send_h2_error_response(&mut respond, 502, ProxyErrorKind::Upstream, &e.to_string());
        }
    }

    Ok(())
}

/// Write an HTTP/2 response from a flat header map, filtering out
/// hop-by-hop headers that HTTP/2 forbids and sanitizing `TE`.
fn send_h2_response(
    respond: &mut SendResponse<Bytes>,
    status: u16,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<()> {
    let mut builder = Response::builder().status(status);
    {
        let header_map = builder
            .headers_mut()
            .ok_or_else(|| PostGateError::Proxy("Invalid response builder state".into()))?;
        apply_flat_headers_to(header_map, headers);
    }

    let response = builder
        .body(())
        .map_err(|e| PostGateError::Proxy(format!("Failed to build h2 response: {}", e)))?;

    let end_stream = body.is_empty();
    let mut send_stream = respond
        .send_response(response, end_stream)
        .map_err(|e| PostGateError::Proxy(format!("Failed to send h2 response headers: {}", e)))?;

    if !end_stream {
        send_stream
            .send_data(body, true)
            .map_err(|e| PostGateError::Proxy(format!("Failed to send h2 response body: {}", e)))?;
    }
    Ok(())
}

fn send_h2_error_response(
    respond: &mut SendResponse<Bytes>,
    status: u16,
    kind: ProxyErrorKind,
    message: &str,
) -> Result<()> {
    let body = crate::proxy::error_page::html_error_body(status, kind, message);
    let headers = crate::proxy::error_page::html_error_headers(body.len());
    send_h2_response(respond, status, &headers, body)
}

/// Same as [`send_h2_response`] but takes a fully-built `HeaderMap` so
/// multi-value headers (e.g. multiple `Set-Cookie` lines) are emitted as
/// distinct h2 header entries instead of being collapsed through a flat
/// map. Used by the multi-value-preserving buffering path.
fn send_h2_response_headermap(
    respond: &mut SendResponse<Bytes>,
    status: u16,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<()> {
    let builder = Response::builder().status(status);
    let builder = apply_headers_to_response_builder_h2(builder, headers);

    let response = builder
        .body(())
        .map_err(|e| PostGateError::Proxy(format!("Failed to build h2 response: {}", e)))?;

    let end_stream = body.is_empty();
    let mut send_stream = respond
        .send_response(response, end_stream)
        .map_err(|e| PostGateError::Proxy(format!("Failed to send h2 response headers: {}", e)))?;

    if !end_stream {
        send_stream
            .send_data(body, true)
            .map_err(|e| PostGateError::Proxy(format!("Failed to send h2 response body: {}", e)))?;
    }
    Ok(())
}

/// Copy a flat `HashMap<String, String>` into a hyper `HeaderMap`, skipping
/// headers that HTTP/2 forbids.
fn apply_flat_headers_to(target: &mut HeaderMap, headers: &HashMap<String, String>) {
    for (key, value) in headers {
        let key_lower = key.to_lowercase();
        if is_h2_forbidden_header(&key_lower) {
            continue;
        }
        let value_to_use = if key_lower == "te" {
            match sanitize_te_header(value) {
                Some(v) => v.to_string(),
                None => continue,
            }
        } else {
            value.clone()
        };
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(key_lower.as_bytes()),
            hyper::header::HeaderValue::from_str(&value_to_use),
        ) {
            target.append(name, value);
        }
    }
}

/// Collect body from H2 RecvStream, releasing flow control even on overflow
/// to prevent the upstream stalling on WINDOW_UPDATE.
async fn collect_h2_body(mut body: RecvStream) -> Result<CapturedBody> {
    let mut collected = Vec::new();
    let mut truncated = false;
    let mut total_size = 0usize;

    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(|e| PostGateError::Proxy(format!("H2 body error: {}", e)))?;
        let chunk_len = chunk.len();
        total_size += chunk_len;
        collected.extend_from_slice(&chunk);
        if total_size > MAX_BODY_SIZE {
            truncated = true;
        }
        let _ = body.flow_control().release_capacity(chunk_len);
    }

    let data = Bytes::from(collected);
    Ok(CapturedBody {
        data,
        size: total_size,
        truncated,
    })
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

/// Pump an `Incoming` body from the upstream client into an h2 `SendStream`
/// while capturing bytes for the UI. Bytes flow to the client as they arrive,
/// which is the whole point of the streaming path — TTFB matches upstream.
async fn pump_incoming_into_h2(
    mut body: hyper::body::Incoming,
    send_stream: &mut h2::SendStream<Bytes>,
    meta: super::passthrough::PassthroughMeta,
    app_state: Arc<crate::state::AppState>,
    body_storage: Arc<crate::proxy::BodyStorage>,
) -> Result<()> {
    use bytes::BytesMut;
    use http_body_util::BodyExt;

    let mut collected = BytesMut::new();
    let mut total_bytes: u64 = 0;
    let mut truncated = false;

    while let Some(frame) = body.frame().await {
        let frame =
            frame.map_err(|e| PostGateError::Proxy(format!("Upstream body error: {}", e)))?;
        if let Some(chunk) = frame.data_ref() {
            total_bytes += chunk.len() as u64;

            // Capture for the UI up to MAX_BODY_SIZE — bytes flow to the
            // client regardless of whether we buffer them locally.
            if meta.capture && collected.len() < MAX_BODY_SIZE {
                let remaining = MAX_BODY_SIZE - collected.len();
                if chunk.len() <= remaining {
                    collected.extend_from_slice(chunk);
                } else {
                    collected.extend_from_slice(&chunk[..remaining]);
                    truncated = true;
                }
            } else if meta.capture {
                truncated = true;
            }

            // Reserve flow-control capacity then send. `reserve_capacity` is
            // advisory — we don't need to await it, but it helps the peer
            // send WINDOW_UPDATEs proactively.
            send_stream.reserve_capacity(chunk.len());
            send_stream
                .send_data(chunk.clone(), false)
                .map_err(|e| PostGateError::Proxy(format!("h2 send_data error: {}", e)))?;
        }
    }

    // Signal end of stream with an empty trailing DATA frame.
    send_stream
        .send_data(Bytes::new(), true)
        .map_err(|e| PostGateError::Proxy(format!("h2 end-of-stream error: {}", e)))?;

    let duration = meta.start_time.elapsed().as_millis() as u64;

    // Honor `disable://capture`: suppress storage, persistence, and emit while
    // still letting the body flow through to the client.
    if !meta.capture {
        return Ok(());
    }

    let collected = collected.freeze();
    // Store body in the in-memory DashMap (fire & forget).
    let stored = CapturedBody {
        data: collected.clone(),
        size: collected.len(),
        truncated,
    };
    {
        let body_storage = body_storage.clone();
        let request_id = meta.request_id.clone();
        tokio::spawn(async move {
            body_storage.store_response_body(&request_id, stored).await;
        });
    }
    if meta.persistence_enabled {
        app_state.persist_body(meta.request_id.clone(), collected.clone(), false);
    }

    app_state.emit_request_event(&CapturedRequestEvent {
        id: meta.request_id.clone(),
        event_type: RequestEventType::Completed,
        data: CapturedRequestData {
            id: meta.request_id,
            timestamp: meta.timestamp,
            method: meta.method,
            url: meta.url,
            host: meta.host,
            path: meta.path,
            request_headers: Some(meta.request_headers),
            response_status: Some(meta.response_status),
            response_headers: Some(meta.response_headers),
            duration_ms: Some(duration),
            matched_rules: meta.matched_rules,
            protocol: meta.protocol,
            content_type: meta.content_type,
            request_size: meta.request_size,
            response_size: Some(total_bytes),
            tls_version: meta.tls_version,
            ..Default::default()
        },
    });

    Ok(())
}
