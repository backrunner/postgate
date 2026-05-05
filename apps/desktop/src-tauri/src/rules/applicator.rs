//! Rule applicator - applies matched rules to requests and responses
//!
//! This module handles the actual application of rule actions to HTTP
//! requests and responses, supporting the full whistle-compatible action set.

use super::engine::MatchedRule;
use super::types::{
    BodyContent, CookieOptions, HeaderModifications, RuleAction, UrlParamModifications,
};
use crate::values::{resolve_str, RequestCtx};
use bytes::Bytes;
use dashmap::DashMap;
use std::collections::HashMap;
use url::Url;

/// Handle to the shared values store + current request context, passed into
/// the applicator so `{name}` and `` `{name}` `` references can be resolved
/// against inline definitions and the global Values tab.
///
/// Supplying `None` (via [`ResolveCtx::disabled`]) turns reference resolution
/// into a no-op — action arguments are used verbatim. This keeps the unit
/// tests inside this module unchanged.
pub struct ResolveCtx<'a> {
    pub store: Option<&'a DashMap<String, String>>,
    pub ctx: Option<&'a RequestCtx<'a>>,
}

impl<'a> ResolveCtx<'a> {
    pub fn disabled() -> Self {
        Self {
            store: None,
            ctx: None,
        }
    }
}

/// Resolve a whistle-style reference using the supplied inline+global maps.
/// Returns the input unchanged when either the store or ctx is absent, or
/// when the input contains no references.
fn resolve_ref(arg: &str, inline: &HashMap<String, String>, res: &ResolveCtx<'_>) -> String {
    match (res.store, res.ctx) {
        (Some(store), Some(ctx)) => resolve_str(arg, inline, store, ctx, 0),
        // No store but we still honor inline definitions so inline values
        // work even on code paths that don't plumb the global store.
        (None, Some(ctx)) => {
            let empty = DashMap::new();
            resolve_str(arg, inline, &empty, ctx, 0)
        }
        (Some(store), None) => {
            let empty_ctx = RequestCtx::empty();
            resolve_str(arg, inline, store, &empty_ctx, 0)
        }
        (None, None) => arg.to_string(),
    }
}

fn resolve_bytes(arg: &str, inline: &HashMap<String, String>, res: &ResolveCtx<'_>) -> Bytes {
    Bytes::from(resolve_ref(arg, inline, res))
}

/// Result of applying rules to a request
#[derive(Debug, Default)]
pub struct RequestModification {
    /// Modified headers
    pub headers: HashMap<String, String>,
    /// Headers to remove
    pub headers_to_remove: Vec<String>,
    /// Modified body (if any)
    pub body: Option<Bytes>,
    /// Modified URL path
    pub path: Option<String>,
    /// Modified URL query parameters
    pub query_params: Option<String>,
    /// Should short-circuit with this response
    pub short_circuit: Option<ShortCircuitResponse>,
    /// Request delay in ms
    pub delay_ms: Option<u64>,
    /// Request speed limit in kbps
    pub speed_kbps: Option<u64>,
    /// Target host override (full URL like http://127.0.0.1:3000/browser)
    pub target_host: Option<String>,
    /// Remaining path to append to target (whistle compatible)
    pub remaining_path: Option<String>,
    /// Whether to ignore/skip this request
    pub ignore: bool,
    /// Debug name for logging
    pub debug_name: Option<String>,
    /// Plugin to handle request (name, config)
    pub plugin: Option<PluginHandlerInfo>,
    /// Upstream timeout in milliseconds (whistle `timeout://<ms>`). When set
    /// the forward call is wrapped in `tokio::time::timeout`; expiring
    /// returns 504 Gateway Timeout to the client.
    pub timeout_ms: Option<u64>,
    /// Feature flags opted into via `enable://`.
    pub enabled_features: Vec<String>,
    /// Feature flags opted out of via `disable://`.
    pub disabled_features: Vec<String>,
    /// Upstream proxy to chain this request through (whistle
    /// `proxy://`/`http-proxy://`/`https-proxy://`/`socks://`). When set the
    /// forward client is built against this upstream proxy instead of
    /// connecting directly.
    pub upstream_proxy: Option<UpstreamProxy>,
}

/// An upstream proxy configuration derived from `proxy://` / `socks://`
/// actions.
#[derive(Debug, Clone)]
pub struct UpstreamProxy {
    pub kind: UpstreamProxyKind,
    pub host: String,
    pub port: u16,
    pub auth: Option<ProxyCreds>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamProxyKind {
    Http,
    Https,
    Socks4,
    Socks5,
}

#[derive(Debug, Clone)]
pub struct ProxyCreds {
    pub username: String,
    pub password: String,
}

/// Plugin handler information
#[derive(Debug, Clone)]
pub struct PluginHandlerInfo {
    pub name: String,
    pub config: serde_json::Value,
}

/// Response to return instead of proxying
#[derive(Debug)]
pub struct ShortCircuitResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Bytes,
}

/// Result of applying rules to a response
#[derive(Debug, Default)]
pub struct ResponseModification {
    /// Modified headers
    pub headers: HashMap<String, String>,
    /// Headers to remove
    pub headers_to_remove: Vec<String>,
    /// Modified body (if any)
    pub body: Option<Bytes>,
    /// Response delay in ms
    pub delay_ms: Option<u64>,
    /// Response speed limit in kbps
    pub speed_kbps: Option<u64>,
    /// Modified status code
    pub status_code: Option<u16>,
    /// Cookies to set
    pub cookies: Vec<String>,
    /// Inject debug script into HTML responses
    pub inject_debug: bool,
}

/// Does the set of matched rules require buffering the upstream response body
/// before sending to the client?
///
/// Returns false only when the rules touch nothing beyond headers / status /
/// cookies — in that case we can stream the response straight through and
/// the client's TTFB tracks the upstream's, not `upstream_latency +
/// upstream_body_size / bandwidth`.
pub fn rules_require_response_body(matched_rules: &[MatchedRule]) -> bool {
    for matched in matched_rules {
        if !matched.rule.enabled {
            continue;
        }
        for action in &matched.rule.actions {
            match action {
                // Body replacement or injection.
                RuleAction::ResponseBody { .. }
                | RuleAction::HtmlBody { .. }
                | RuleAction::CssBody { .. }
                | RuleAction::JsBody { .. }
                | RuleAction::JsonBody { .. }
                | RuleAction::ResponseReplace { .. }
                | RuleAction::HtmlReplace { .. }
                | RuleAction::JsReplace { .. }
                | RuleAction::CssReplace { .. }
                | RuleAction::HtmlAppend { .. }
                | RuleAction::HtmlPrepend { .. }
                | RuleAction::JsAppend { .. }
                | RuleAction::JsPrepend { .. }
                | RuleAction::CssAppend { .. }
                | RuleAction::CssPrepend { .. }
                | RuleAction::ResponsePrepend { .. }
                | RuleAction::ResponseAppend { .. }
                | RuleAction::Echo
                | RuleAction::Mock { .. } => return true,
                // Response throttling requires owning the body chunks.
                RuleAction::Speed { response_kbps, .. } if response_kbps.is_some() => return true,
                // Debug injects a <script> into HTML responses.
                RuleAction::Debug { .. } => return true,
                // Plugins may rewrite the response body in handleResponse.
                RuleAction::Plugin { .. } => return true,
                _ => {}
            }
        }
    }
    false
}

/// Does the set of matched rules require buffering the client's request body
/// before sending upstream? Same rationale as `rules_require_response_body`.
pub fn rules_require_request_body(matched_rules: &[MatchedRule]) -> bool {
    for matched in matched_rules {
        if !matched.rule.enabled {
            continue;
        }
        for action in &matched.rule.actions {
            match action {
                RuleAction::RequestBody { .. }
                | RuleAction::RequestReplace { .. }
                | RuleAction::RequestPrepend { .. }
                | RuleAction::RequestAppend { .. } => return true,
                RuleAction::Speed { request_kbps, .. } if request_kbps.is_some() => return true,
                RuleAction::Plugin { .. } => return true,
                _ => {}
            }
        }
    }
    false
}

/// Whistle feature flags consumed by the proxy. Values match the whistle
/// `enable://` / `disable://` token names (lowercased).
pub mod feature {
    /// Don't record this request in the UI or persistent capture.
    pub const CAPTURE: &str = "capture";
    /// Legacy alias of `capture` — whistle users also use `hide`.
    pub const HIDE: &str = "hide";
    /// Close the connection without responding. Useful for simulating peer
    /// disconnects.
    pub const ABORT: &str = "abort";
}

/// Returns true if `feature` is not in the modification's `disabled_features`.
/// Used at capture / emit sites to honor `disable://capture`.
pub fn capture_enabled(modification: &RequestModification) -> bool {
    !modification
        .disabled_features
        .iter()
        .any(|f| f.eq_ignore_ascii_case(feature::CAPTURE) || f.eq_ignore_ascii_case(feature::HIDE))
}

/// Returns true if `enable://abort` is set, i.e. the proxy should terminate
/// this request without responding.
pub fn should_abort(modification: &RequestModification) -> bool {
    modification
        .enabled_features
        .iter()
        .any(|f| f.eq_ignore_ascii_case(feature::ABORT))
}

/// Apply rules to a request and return modifications
pub fn apply_request_rules(
    matched_rules: &[MatchedRule],
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&Bytes>,
) -> RequestModification {
    apply_request_rules_with_values(
        matched_rules,
        url,
        method,
        headers,
        body,
        &ResolveCtx::disabled(),
    )
}

/// Apply rules to a request with whistle-compatible `{name}` reference
/// resolution. Callers with access to the app state's values store should
/// prefer this entry point; the older [`apply_request_rules`] is a
/// convenience wrapper for tests and legacy callers.
#[allow(clippy::collapsible_match)]
pub fn apply_request_rules_with_values(
    matched_rules: &[MatchedRule],
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&Bytes>,
    res: &ResolveCtx<'_>,
) -> RequestModification {
    let mut modification = RequestModification {
        headers: headers.clone(),
        body: body.cloned(),
        ..Default::default()
    };

    // Parse URL for query parameter modifications
    let parsed_url = Url::parse(url).ok();

    for matched in matched_rules {
        let rule = &matched.rule;
        if !rule.enabled {
            continue;
        }
        let inline = matched.inline_values.as_ref();

        // Check filters if present
        if let Some(filters) = &rule.filters {
            let protocol = parsed_url.as_ref().map(|u| u.scheme()).unwrap_or("http");
            let port = parsed_url
                .as_ref()
                .and_then(|u| u.port())
                .unwrap_or(if protocol == "https" { 443 } else { 80 });

            if !filters.matches(method, protocol, port, headers, url) {
                continue;
            }
        }

        for action in &rule.actions {
            match action {
                RuleAction::Host { target } => {
                    modification.target_host = Some(target.clone());
                    // Store the remaining path for whistle-compatible forwarding
                    modification.remaining_path = Some(matched.remaining_path.clone());
                }

                RuleAction::RequestHeaders { modifications } => {
                    apply_header_modifications_with_values(
                        &mut modification.headers,
                        modifications,
                        inline,
                        res,
                    );
                    modification
                        .headers_to_remove
                        .extend(modifications.remove.clone());
                }

                RuleAction::RequestBody { content } => {
                    modification.body = Some(resolve_body_content(content, inline, res));
                }

                RuleAction::UrlParams { modifications } => {
                    if let Some(ref url) = parsed_url {
                        modification.query_params =
                            Some(apply_url_param_modifications(url, modifications));
                    }
                }

                RuleAction::PathReplace {
                    pattern,
                    replacement,
                } => {
                    if let Some(ref url) = parsed_url {
                        let path = url.path();
                        let new_path = if pattern.starts_with('/') || pattern.starts_with('^') {
                            // Regex replacement
                            if let Ok(re) = regex::Regex::new(pattern) {
                                re.replace_all(path, replacement.as_str()).to_string()
                            } else {
                                path.replace(pattern, replacement)
                            }
                        } else {
                            path.replace(pattern, replacement)
                        };
                        modification.path = Some(new_path);
                    }
                }

                RuleAction::Method { method: new_method } => {
                    // Method modification - stored in headers for now
                    modification
                        .headers
                        .insert(":method".to_string(), new_method.clone());
                }

                RuleAction::UserAgent { value } => {
                    modification
                        .headers
                        .insert("user-agent".to_string(), value.clone());
                }

                RuleAction::Referer { value } => {
                    modification
                        .headers
                        .insert("referer".to_string(), value.clone());
                }

                RuleAction::Auth { username, password } => {
                    let credentials = format!("{}:{}", username, password);
                    let encoded = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        credentials.as_bytes(),
                    );
                    modification
                        .headers
                        .insert("authorization".to_string(), format!("Basic {}", encoded));
                }

                RuleAction::RequestCookies { cookies } => {
                    let cookie_str: String = cookies
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>()
                        .join("; ");

                    if let Some(existing) = modification.headers.get_mut("cookie") {
                        existing.push_str("; ");
                        existing.push_str(&cookie_str);
                    } else {
                        modification
                            .headers
                            .insert("cookie".to_string(), cookie_str);
                    }
                }

                RuleAction::ForwardedFor { value } => {
                    modification
                        .headers
                        .insert("x-forwarded-for".to_string(), value.clone());
                }

                RuleAction::RequestReplace {
                    pattern,
                    replacement,
                    regex,
                } => {
                    if let Some(ref body) = modification.body {
                        let body_str = String::from_utf8_lossy(body);
                        let new_body = if *regex {
                            if let Ok(re) = regex::Regex::new(pattern) {
                                re.replace_all(&body_str, replacement.as_str()).to_string()
                            } else {
                                body_str.replace(pattern, replacement)
                            }
                        } else {
                            body_str.replace(pattern, replacement)
                        };
                        modification.body = Some(Bytes::from(new_body));
                    }
                }

                RuleAction::RequestCors {
                    origin,
                    credentials,
                } => {
                    if let Some(o) = origin {
                        modification.headers.insert("origin".to_string(), o.clone());
                    }
                    if *credentials {
                        modification.headers.insert(
                            "access-control-request-credentials".to_string(),
                            "true".to_string(),
                        );
                    }
                }

                RuleAction::StatusCode { code } => {
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: *code,
                        headers: HashMap::new(),
                        body: Bytes::new(),
                    });
                }

                RuleAction::Redirect { url, status } => {
                    let mut headers = HashMap::new();
                    headers.insert("location".to_string(), url.clone());
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: *status,
                        headers,
                        body: Bytes::new(),
                    });
                }

                RuleAction::File { path } => {
                    // The action argument may be a `{name}` reference into the
                    // values store (whistle `file://{name}`) OR a real disk
                    // path. Try the values store first.
                    let path_str = path.to_string_lossy().into_owned();
                    let trimmed = path_str.trim();
                    let is_ref = (trimmed.starts_with('{') && trimmed.ends_with('}'))
                        || (trimmed.starts_with("`{") && trimmed.ends_with("}`"));

                    let (body, content_type) = if is_ref {
                        let body = resolve_bytes(trimmed, inline, res);
                        // Guess MIME from the referenced value's name if we
                        // can (strip the braces and treat as a filename).
                        let name_for_mime = trimmed
                            .trim_matches('`')
                            .trim_start_matches('{')
                            .trim_end_matches('}');
                        let ct = mime_guess::from_path(name_for_mime)
                            .first_or_octet_stream()
                            .to_string();
                        (body, ct)
                    } else {
                        let body = std::fs::read(path).map(Bytes::from).unwrap_or_default();
                        let ct = mime_guess::from_path(path)
                            .first_or_octet_stream()
                            .to_string();
                        (body, ct)
                    };

                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), content_type);
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body,
                    });
                }

                RuleAction::HtmlBody { content } => {
                    let mut headers = HashMap::new();
                    headers.insert(
                        "content-type".to_string(),
                        "text/html; charset=utf-8".to_string(),
                    );
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body: resolve_bytes(content, inline, res),
                    });
                }

                RuleAction::JsBody { content } => {
                    let mut headers = HashMap::new();
                    headers.insert(
                        "content-type".to_string(),
                        "application/javascript; charset=utf-8".to_string(),
                    );
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body: resolve_bytes(content, inline, res),
                    });
                }

                RuleAction::CssBody { content } => {
                    let mut headers = HashMap::new();
                    headers.insert(
                        "content-type".to_string(),
                        "text/css; charset=utf-8".to_string(),
                    );
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body: resolve_bytes(content, inline, res),
                    });
                }

                RuleAction::Delay { request_ms, .. } => {
                    if let Some(ms) = request_ms {
                        modification.delay_ms = Some(*ms);
                    }
                }

                RuleAction::Speed { request_kbps, .. } => {
                    if let Some(kbps) = request_kbps {
                        modification.speed_kbps = Some(*kbps);
                    }
                }

                RuleAction::Debug { name } => {
                    modification.debug_name = Some(name.clone());
                }

                RuleAction::Ignore => {
                    modification.ignore = true;
                }

                RuleAction::Log { message } => {
                    if let Some(msg) = message {
                        tracing::info!(target: "rule_log", "{}", msg);
                    } else {
                        tracing::info!(target: "rule_log", "Request matched rule: {}", url);
                    }
                }

                RuleAction::Plugin { name, config } => {
                    modification.plugin = Some(PluginHandlerInfo {
                        name: name.clone(),
                        config: config.clone(),
                    });
                }

                // `reqPrepend://` — prepend raw text to the request body.
                RuleAction::RequestPrepend { content } => {
                    let extra = resolve_bytes(content, inline, res);
                    let current = modification.body.clone().unwrap_or_default();
                    let mut combined = Vec::with_capacity(extra.len() + current.len());
                    combined.extend_from_slice(&extra);
                    combined.extend_from_slice(&current);
                    modification.body = Some(Bytes::from(combined));
                }

                // `reqAppend://` — append raw text to the request body.
                RuleAction::RequestAppend { content } => {
                    let extra = resolve_bytes(content, inline, res);
                    let current = modification.body.clone().unwrap_or_default();
                    let mut combined = Vec::with_capacity(extra.len() + current.len());
                    combined.extend_from_slice(&current);
                    combined.extend_from_slice(&extra);
                    modification.body = Some(Bytes::from(combined));
                }

                // `timeout://<ms>` — abort the upstream call if it takes longer.
                // Stored on the RequestModification; consumed in the proxy
                // forward sites via tokio::time::timeout.
                RuleAction::Timeout { ms } => {
                    modification.timeout_ms = Some(*ms);
                }

                // `delete://name1,name2` — pure request-header removal.
                RuleAction::DeleteHeaders { headers } => {
                    for name in headers {
                        let lower = name.to_lowercase();
                        modification.headers.remove(&lower);
                        modification.headers_to_remove.push(lower);
                    }
                }

                // Upstream proxy chaining — captured on the modification so
                // the forward sites can route through a per-target pooled
                // client. The actual connection is still made lazily by
                // the proxy::upstream module.
                RuleAction::HttpProxy { host, port, auth } => {
                    modification.upstream_proxy = Some(UpstreamProxy {
                        kind: UpstreamProxyKind::Http,
                        host: host.clone(),
                        port: *port,
                        auth: auth.as_ref().map(|a| ProxyCreds {
                            username: a.username.clone(),
                            password: a.password.clone(),
                        }),
                    });
                }
                RuleAction::HttpsProxy { host, port, auth } => {
                    modification.upstream_proxy = Some(UpstreamProxy {
                        kind: UpstreamProxyKind::Https,
                        host: host.clone(),
                        port: *port,
                        auth: auth.as_ref().map(|a| ProxyCreds {
                            username: a.username.clone(),
                            password: a.password.clone(),
                        }),
                    });
                }
                RuleAction::SocksProxy {
                    host,
                    port,
                    version,
                    auth,
                } => {
                    let kind = if *version == 4 {
                        UpstreamProxyKind::Socks4
                    } else {
                        UpstreamProxyKind::Socks5
                    };
                    modification.upstream_proxy = Some(UpstreamProxy {
                        kind,
                        host: host.clone(),
                        port: *port,
                        auth: auth.as_ref().map(|a| ProxyCreds {
                            username: a.username.clone(),
                            password: a.password.clone(),
                        }),
                    });
                }

                // `enable://<features>` / `disable://<features>` — toggled
                // for the duration of this request. Currently only a few
                // flags are wired into the proxy; the rest are recorded so
                // plugins / reporting can see them.
                RuleAction::Enable { features } => {
                    modification
                        .enabled_features
                        .extend(features.iter().cloned());
                }
                RuleAction::Disable { features } => {
                    modification
                        .disabled_features
                        .extend(features.iter().cloned());
                }

                _ => {
                    // Response-only actions are skipped here
                }
            }
        }
    }

    modification
}

/// Apply rules to a response and return modifications
pub fn apply_response_rules(
    matched_rules: &[MatchedRule],
    url: &str,
    method: &str,
    request_headers: &HashMap<String, String>,
    response_headers: &HashMap<String, String>,
    body: Option<&Bytes>,
    content_type: Option<&str>,
) -> ResponseModification {
    apply_response_rules_with_values(
        matched_rules,
        url,
        method,
        request_headers,
        response_headers,
        body,
        content_type,
        &ResolveCtx::disabled(),
    )
}

/// Apply rules to a response with whistle-compatible `{name}` reference
/// resolution. See [`apply_request_rules_with_values`] for the rationale.
#[allow(clippy::collapsible_match, clippy::too_many_arguments)]
pub fn apply_response_rules_with_values(
    matched_rules: &[MatchedRule],
    url: &str,
    method: &str,
    request_headers: &HashMap<String, String>,
    response_headers: &HashMap<String, String>,
    body: Option<&Bytes>,
    content_type: Option<&str>,
    res: &ResolveCtx<'_>,
) -> ResponseModification {
    let mut modification = ResponseModification {
        headers: response_headers.clone(),
        body: body.cloned(),
        ..Default::default()
    };

    let parsed_url = Url::parse(url).ok();

    for matched in matched_rules {
        let rule = &matched.rule;
        if !rule.enabled {
            continue;
        }
        let inline = matched.inline_values.as_ref();

        // Check filters if present
        if let Some(filters) = &rule.filters {
            let protocol = parsed_url.as_ref().map(|u| u.scheme()).unwrap_or("http");
            let port = parsed_url
                .as_ref()
                .and_then(|u| u.port())
                .unwrap_or(if protocol == "https" { 443 } else { 80 });

            if !filters.matches(method, protocol, port, request_headers, url) {
                continue;
            }
        }

        for action in &rule.actions {
            match action {
                RuleAction::ResponseHeaders { modifications } => {
                    apply_header_modifications_with_values(
                        &mut modification.headers,
                        modifications,
                        inline,
                        res,
                    );
                    modification
                        .headers_to_remove
                        .extend(modifications.remove.clone());
                }

                RuleAction::ResponseBody { content } => {
                    modification.body = Some(resolve_body_content(content, inline, res));
                }

                RuleAction::ResponseReplace {
                    pattern,
                    replacement,
                    regex,
                } => {
                    if let Some(ref body) = modification.body {
                        let body_str = String::from_utf8_lossy(body);
                        let new_body = if *regex {
                            if let Ok(re) = regex::Regex::new(pattern) {
                                re.replace_all(&body_str, replacement.as_str()).to_string()
                            } else {
                                body_str.replace(pattern, replacement)
                            }
                        } else {
                            body_str.replace(pattern, replacement)
                        };
                        modification.body = Some(Bytes::from(new_body));
                    }
                }

                RuleAction::ResponseCookies { cookies } => {
                    for (name, opts) in cookies {
                        modification.cookies.push(format_set_cookie(name, opts));
                    }
                }

                RuleAction::ResponseType { content_type } => {
                    modification
                        .headers
                        .insert("content-type".to_string(), content_type.clone());
                }

                RuleAction::ResponseCharset { charset } => {
                    if let Some(ct) = modification.headers.get_mut("content-type") {
                        if !ct.contains("charset=") {
                            ct.push_str(&format!("; charset={}", charset));
                        }
                    }
                }

                RuleAction::Attachment { filename } => {
                    let value = match filename {
                        Some(name) => format!("attachment; filename=\"{}\"", name),
                        None => "attachment".to_string(),
                    };
                    modification
                        .headers
                        .insert("content-disposition".to_string(), value);
                }

                RuleAction::ResponseCors {
                    origin,
                    methods,
                    headers,
                    credentials,
                    max_age,
                } => {
                    let origin_value = origin.clone().unwrap_or_else(|| "*".to_string());
                    modification
                        .headers
                        .insert("access-control-allow-origin".to_string(), origin_value);

                    if let Some(m) = methods {
                        modification
                            .headers
                            .insert("access-control-allow-methods".to_string(), m.clone());
                    }

                    if let Some(h) = headers {
                        modification
                            .headers
                            .insert("access-control-allow-headers".to_string(), h.clone());
                    }

                    if *credentials {
                        modification.headers.insert(
                            "access-control-allow-credentials".to_string(),
                            "true".to_string(),
                        );
                    }

                    if let Some(age) = max_age {
                        modification
                            .headers
                            .insert("access-control-max-age".to_string(), age.to_string());
                    }
                }

                RuleAction::StatusCode { code } => {
                    modification.status_code = Some(*code);
                }

                RuleAction::HtmlAppend { content } => {
                    if is_html(content_type) {
                        let resolved = resolve_ref(content, inline, res);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &resolved, false));
                    }
                }

                RuleAction::HtmlPrepend { content } => {
                    if is_html(content_type) {
                        let resolved = resolve_ref(content, inline, res);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &resolved, true));
                    }
                }

                RuleAction::JsAppend { content } => {
                    let resolved = resolve_ref(content, inline, res);
                    if is_html(content_type) {
                        let script = format!("<script>{}</script>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &script, false));
                    } else if is_js(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = body.to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(resolved.as_bytes());
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::JsPrepend { content } => {
                    let resolved = resolve_ref(content, inline, res);
                    if is_html(content_type) {
                        let script = format!("<script>{}</script>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &script, true));
                    } else if is_js(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = resolved.as_bytes().to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(body);
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::CssAppend { content } => {
                    let resolved = resolve_ref(content, inline, res);
                    if is_html(content_type) {
                        let style = format!("<style>{}</style>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &style, false));
                    } else if is_css(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = body.to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(resolved.as_bytes());
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::CssPrepend { content } => {
                    let resolved = resolve_ref(content, inline, res);
                    if is_html(content_type) {
                        let style = format!("<style>{}</style>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &style, true));
                    } else if is_css(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = resolved.as_bytes().to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(body);
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::Delay { response_ms, .. } => {
                    if let Some(ms) = response_ms {
                        modification.delay_ms = Some(*ms);
                    }
                }

                RuleAction::Speed { response_kbps, .. } => {
                    if let Some(kbps) = response_kbps {
                        modification.speed_kbps = Some(*kbps);
                    }
                }

                RuleAction::Enable { features } => {
                    for feature in features {
                        tracing::debug!("Enabling feature: {}", feature);
                        // Feature flags would be handled by the proxy context
                    }
                }

                RuleAction::Disable { features } => {
                    for feature in features {
                        tracing::debug!("Disabling feature: {}", feature);
                        // Feature flags would be handled by the proxy context
                    }
                }

                RuleAction::Log { message } => {
                    if let Some(msg) = message {
                        tracing::info!(target: "rule_log", "{}", msg);
                    } else {
                        tracing::info!(target: "rule_log", "Response for: {}", url);
                    }
                }

                RuleAction::Debug { .. } => {
                    // Enable debug script injection for HTML responses
                    if is_html(content_type) {
                        modification.inject_debug = true;
                    }
                }

                // `jsonBody://{value}` — short-circuit with a JSON response.
                RuleAction::JsonBody { value } => {
                    let body_str = serde_json::to_string(value).unwrap_or_default();
                    let resolved = resolve_bytes(&body_str, inline, res);
                    modification.headers.insert(
                        "content-type".to_string(),
                        "application/json; charset=utf-8".to_string(),
                    );
                    modification.body = Some(resolved);
                }

                // `delete://name1,name2` — remove response headers by name.
                RuleAction::DeleteHeaders { headers } => {
                    for name in headers {
                        let lower = name.to_lowercase();
                        modification.headers.remove(&lower);
                        modification.headers_to_remove.push(lower);
                    }
                }

                // `echo://` — respond with an echo of the request. The
                // applicator can't see the request body here without a hook,
                // so we emit a textual summary using the URL/method/headers.
                // This matches whistle's default behaviour for HTML clients.
                RuleAction::Echo => {
                    let mut s = String::new();
                    s.push_str(method);
                    s.push(' ');
                    s.push_str(url);
                    s.push('\n');
                    for (k, v) in request_headers {
                        s.push_str(k);
                        s.push_str(": ");
                        s.push_str(v);
                        s.push('\n');
                    }
                    modification.headers.insert(
                        "content-type".to_string(),
                        "text/plain; charset=utf-8".to_string(),
                    );
                    modification.body = Some(Bytes::from(s));
                }

                // `mock://<path>` — replace response body with the referenced
                // file/value and pick a MIME from the name. Status is 200.
                RuleAction::Mock { path } => {
                    let path_str = path.to_string_lossy().into_owned();
                    let trimmed = path_str.trim();
                    let is_ref = (trimmed.starts_with('{') && trimmed.ends_with('}'))
                        || (trimmed.starts_with("`{") && trimmed.ends_with("}`"));
                    let (body, guessed_ct) = if is_ref {
                        let body = resolve_bytes(trimmed, inline, res);
                        let name_for_mime = trimmed
                            .trim_matches('`')
                            .trim_start_matches('{')
                            .trim_end_matches('}');
                        let ct = mime_guess::from_path(name_for_mime)
                            .first_or_octet_stream()
                            .to_string();
                        (body, ct)
                    } else {
                        let bytes = std::fs::read(path).map(Bytes::from).unwrap_or_default();
                        let ct = mime_guess::from_path(path)
                            .first_or_octet_stream()
                            .to_string();
                        (bytes, ct)
                    };
                    modification
                        .headers
                        .entry("content-type".to_string())
                        .or_insert(guessed_ct);
                    modification.body = Some(body);
                    modification.status_code = Some(200);
                }

                // `htmlReplace://` / `jsReplace://` / `cssReplace://`
                // — content-type-gated string/regex replacements. Essentially
                // `resReplace://` scoped to a specific mime family.
                RuleAction::HtmlReplace {
                    pattern,
                    replacement,
                    regex,
                } => {
                    if is_html(content_type) {
                        apply_body_replace(&mut modification.body, pattern, replacement, *regex);
                    }
                }
                RuleAction::JsReplace {
                    pattern,
                    replacement,
                    regex,
                } => {
                    if is_js(content_type) {
                        apply_body_replace(&mut modification.body, pattern, replacement, *regex);
                    }
                }
                RuleAction::CssReplace {
                    pattern,
                    replacement,
                    regex,
                } => {
                    if is_css(content_type) {
                        apply_body_replace(&mut modification.body, pattern, replacement, *regex);
                    }
                }

                // `resPrepend://` / `resAppend://` — plain text prepend/append
                // to the response body. Unlike the `html*` / `js*` / `css*`
                // variants these are content-type agnostic.
                RuleAction::ResponsePrepend { content } => {
                    let extra = resolve_bytes(content, inline, res);
                    let current = modification.body.clone().unwrap_or_default();
                    let mut combined = Vec::with_capacity(extra.len() + current.len());
                    combined.extend_from_slice(&extra);
                    combined.extend_from_slice(&current);
                    modification.body = Some(Bytes::from(combined));
                }
                RuleAction::ResponseAppend { content } => {
                    let extra = resolve_bytes(content, inline, res);
                    let current = modification.body.clone().unwrap_or_default();
                    let mut combined = Vec::with_capacity(extra.len() + current.len());
                    combined.extend_from_slice(&current);
                    combined.extend_from_slice(&extra);
                    modification.body = Some(Bytes::from(combined));
                }

                _ => {
                    // Request-only actions are skipped here
                }
            }
        }
    }

    modification
}

/// Apply header modifications
fn apply_header_modifications(
    headers: &mut HashMap<String, String>,
    modifications: &HeaderModifications,
) {
    // Remove headers
    for key in &modifications.remove {
        headers.remove(&key.to_lowercase());
    }

    // Set headers
    for (key, value) in &modifications.set {
        headers.insert(key.to_lowercase(), value.clone());
    }

    // Append headers
    for (key, value) in &modifications.append {
        let key_lower = key.to_lowercase();
        if let Some(existing) = headers.get_mut(&key_lower) {
            existing.push_str(", ");
            existing.push_str(value);
        } else {
            headers.insert(key_lower, value.clone());
        }
    }
}

/// Like [`apply_header_modifications`] but runs header values through the
/// values resolver so `{name}` references are expanded per whistle.
fn apply_header_modifications_with_values(
    headers: &mut HashMap<String, String>,
    modifications: &HeaderModifications,
    inline: &HashMap<String, String>,
    res: &ResolveCtx<'_>,
) {
    for key in &modifications.remove {
        headers.remove(&key.to_lowercase());
    }
    for (key, value) in &modifications.set {
        let resolved = resolve_ref(value, inline, res);
        headers.insert(key.to_lowercase(), resolved);
    }
    for (key, value) in &modifications.append {
        let resolved = resolve_ref(value, inline, res);
        let key_lower = key.to_lowercase();
        if let Some(existing) = headers.get_mut(&key_lower) {
            existing.push_str(", ");
            existing.push_str(&resolved);
        } else {
            headers.insert(key_lower, resolved);
        }
    }
}

/// Apply URL parameter modifications
fn apply_url_param_modifications(url: &Url, modifications: &UrlParamModifications) -> String {
    let mut params: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // Remove specified params
    params.retain(|(k, _)| !modifications.remove.contains(k));

    // Set/overwrite params
    for (key, value) in &modifications.set {
        if let Some(existing) = params.iter_mut().find(|(k, _)| k == key) {
            existing.1 = value.clone();
        } else {
            params.push((key.clone(), value.clone()));
        }
    }

    // Append params (allow duplicates)
    for (key, value) in &modifications.append {
        params.push((key.clone(), value.clone()));
    }

    // Build query string
    if params.is_empty() {
        String::new()
    } else {
        params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// Format Set-Cookie header value
fn format_set_cookie(name: &str, opts: &CookieOptions) -> String {
    let mut cookie = format!("{}={}", name, opts.value);

    if let Some(ref path) = opts.path {
        cookie.push_str(&format!("; Path={}", path));
    }

    if let Some(ref domain) = opts.domain {
        cookie.push_str(&format!("; Domain={}", domain));
    }

    if let Some(max_age) = opts.max_age {
        cookie.push_str(&format!("; Max-Age={}", max_age));
    }

    if opts.secure {
        cookie.push_str("; Secure");
    }

    if opts.http_only {
        cookie.push_str("; HttpOnly");
    }

    if let Some(ref same_site) = opts.same_site {
        cookie.push_str(&format!("; SameSite={}", same_site));
    }

    cookie
}

/// Resolve body content to bytes, expanding whistle-style `{name}` references
/// in text/JSON payloads and in file paths against the values store.
fn resolve_body_content(
    content: &BodyContent,
    inline: &HashMap<String, String>,
    res: &ResolveCtx<'_>,
) -> Bytes {
    match content {
        BodyContent::Text { content, .. } => resolve_bytes(content, inline, res),
        BodyContent::Json { value } => {
            let raw = serde_json::to_string(value).unwrap_or_default();
            resolve_bytes(&raw, inline, res)
        }
        BodyContent::File { path } => {
            // If the path is a `{name}` reference, resolve from the values
            // store; otherwise read from disk (whistle file:// semantics).
            let path_str = path.to_string_lossy();
            let trimmed = path_str.trim();
            let is_ref = (trimmed.starts_with('{') && trimmed.ends_with('}'))
                || (trimmed.starts_with("`{") && trimmed.ends_with("}`"));
            if is_ref {
                resolve_bytes(trimmed, inline, res)
            } else {
                std::fs::read(path).map(Bytes::from).unwrap_or_default()
            }
        }
        BodyContent::Base64 { data } => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map(Bytes::from)
                .unwrap_or_default()
        }
        BodyContent::Empty => Bytes::new(),
    }
}

/// Check if content type is HTML
fn is_html(content_type: Option<&str>) -> bool {
    content_type
        .map(|ct| ct.contains("text/html"))
        .unwrap_or(false)
}

/// Check if content type is JavaScript
fn is_js(content_type: Option<&str>) -> bool {
    content_type
        .map(|ct| ct.contains("javascript") || ct.contains("text/js"))
        .unwrap_or(false)
}

/// Check if content type is CSS
fn is_css(content_type: Option<&str>) -> bool {
    content_type
        .map(|ct| ct.contains("text/css"))
        .unwrap_or(false)
}

/// String- or regex-replace inside a response body. Used by resReplace,
/// htmlReplace, jsReplace, cssReplace. Falls back to literal `.replace`
/// when the supplied pattern doesn't compile as a regex.
fn apply_body_replace(body: &mut Option<Bytes>, pattern: &str, replacement: &str, regex: bool) {
    if let Some(current) = body.as_ref() {
        let body_str = String::from_utf8_lossy(current);
        let new_body = if regex {
            match regex::Regex::new(pattern) {
                Ok(re) => re.replace_all(&body_str, replacement).to_string(),
                Err(_) => body_str.replace(pattern, replacement),
            }
        } else {
            body_str.replace(pattern, replacement)
        };
        *body = Some(Bytes::from(new_body));
    }
}

/// Append or prepend content to HTML body
fn append_to_html(body: Option<&Bytes>, content: &str, prepend: bool) -> Bytes {
    let html = body
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();

    let new_html = if prepend {
        // Insert after <head> if exists, otherwise at beginning
        if let Some(pos) = html.to_lowercase().find("<head>") {
            let insert_pos = pos + 6;
            format!("{}{}{}", &html[..insert_pos], content, &html[insert_pos..])
        } else if let Some(pos) = html.to_lowercase().find("<html>") {
            let insert_pos = pos + 6;
            format!("{}{}{}", &html[..insert_pos], content, &html[insert_pos..])
        } else {
            format!("{}{}", content, html)
        }
    } else {
        // Insert before </body> if exists, otherwise at end
        if let Some(pos) = html.to_lowercase().rfind("</body>") {
            format!("{}{}{}", &html[..pos], content, &html[pos..])
        } else if let Some(pos) = html.to_lowercase().rfind("</html>") {
            format!("{}{}{}", &html[..pos], content, &html[pos..])
        } else {
            format!("{}{}", html, content)
        }
    };

    Bytes::from(new_html)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_header_modifications() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/html".to_string());
        headers.insert("x-custom".to_string(), "value1".to_string());

        let mods = HeaderModifications {
            set: [("x-new".to_string(), "new-value".to_string())]
                .into_iter()
                .collect(),
            remove: vec!["x-custom".to_string()],
            append: HashMap::new(),
        };

        apply_header_modifications(&mut headers, &mods);

        assert!(!headers.contains_key("x-custom"));
        assert_eq!(headers.get("x-new"), Some(&"new-value".to_string()));
    }

    #[test]
    fn test_html_append() {
        let html = Bytes::from("<html><head></head><body>Hello</body></html>");
        let result = append_to_html(Some(&html), "<script>test</script>", false);
        let result_str = String::from_utf8_lossy(&result);
        assert!(result_str.contains("<script>test</script></body>"));
    }

    #[test]
    fn test_html_prepend() {
        let html = Bytes::from("<html><head></head><body>Hello</body></html>");
        let result = append_to_html(Some(&html), "<meta charset='utf-8'>", true);
        let result_str = String::from_utf8_lossy(&result);
        assert!(result_str.contains("<head><meta charset='utf-8'>"));
    }

    #[test]
    fn test_url_param_modifications() {
        let url = Url::parse("https://example.com/api?foo=1&bar=2").unwrap();
        let mods = UrlParamModifications {
            set: [("baz".to_string(), "3".to_string())].into_iter().collect(),
            remove: vec!["foo".to_string()],
            append: HashMap::new(),
        };

        let result = apply_url_param_modifications(&url, &mods);
        assert!(result.contains("bar=2"));
        assert!(result.contains("baz=3"));
        assert!(!result.contains("foo=1"));
    }

    #[test]
    fn test_format_set_cookie() {
        let opts = CookieOptions {
            value: "test_value".to_string(),
            path: Some("/".to_string()),
            domain: Some("example.com".to_string()),
            max_age: Some(3600),
            secure: true,
            http_only: true,
            same_site: Some("Strict".to_string()),
        };

        let cookie = format_set_cookie("session", &opts);
        assert!(cookie.contains("session=test_value"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Domain=example.com"));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[test]
    fn test_apply_body_replace_literal() {
        let mut body = Some(Bytes::from("hello world, hello"));
        apply_body_replace(&mut body, "hello", "hi", false);
        assert_eq!(body.unwrap(), Bytes::from("hi world, hi"));
    }

    #[test]
    fn test_apply_body_replace_regex() {
        let mut body = Some(Bytes::from("user=123; user=456"));
        apply_body_replace(&mut body, r"user=\d+", "user=X", true);
        assert_eq!(body.unwrap(), Bytes::from("user=X; user=X"));
    }

    #[test]
    fn test_rules_require_response_body_covers_all_body_actions() {
        // Guard against future regressions: if we add a body-modifying action
        // and forget to list it here, streaming-path users silently lose it.
        // This test exercises the predicate for a representative sample of
        // the newly-added actions.
        use crate::rules::engine::MatchedRule;
        use crate::rules::types::{Pattern, Rule};

        let mk_rule = |action: RuleAction| MatchedRule {
            rule: Rule {
                id: "t".into(),
                pattern: Pattern::Domain("example.com".into()),
                filters: None,
                actions: vec![action],
                enabled: true,
                priority: 0,
                raw_line: String::new(),
                negated: false,
            },
            remaining_path: String::new(),
            inline_values: Arc::new(HashMap::new()),
        };

        // Should require buffering:
        for action in [
            RuleAction::JsonBody {
                value: serde_json::json!({}),
            },
            RuleAction::HtmlReplace {
                pattern: "a".into(),
                replacement: "b".into(),
                regex: false,
            },
            RuleAction::JsReplace {
                pattern: "a".into(),
                replacement: "b".into(),
                regex: false,
            },
            RuleAction::CssReplace {
                pattern: "a".into(),
                replacement: "b".into(),
                regex: false,
            },
            RuleAction::ResponsePrepend {
                content: "x".into(),
            },
            RuleAction::ResponseAppend {
                content: "x".into(),
            },
            RuleAction::Echo,
            RuleAction::Mock {
                path: "/tmp/x".into(),
            },
        ] {
            assert!(
                rules_require_response_body(&[mk_rule(action.clone())]),
                "response body should be required for {:?}",
                action
            );
        }

        // Should NOT require buffering:
        for action in [
            RuleAction::ResponseHeaders {
                modifications: HeaderModifications::default(),
            },
            RuleAction::DeleteHeaders {
                headers: vec!["X-Foo".into()],
            },
            RuleAction::ResponseType {
                content_type: "text/plain".into(),
            },
            RuleAction::Delay {
                request_ms: None,
                response_ms: Some(100),
            },
        ] {
            assert!(
                !rules_require_response_body(&[mk_rule(action.clone())]),
                "response body should NOT be required for {:?}",
                action
            );
        }
    }
}
