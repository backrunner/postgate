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
use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use url::Url;

const DEFAULT_MERGE_BODY_LIMIT: usize = 2 * 1024 * 1024;
const BIG_MERGE_BODY_LIMIT: usize = 16 * 1024 * 1024;

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
    pub remote_resources: Option<&'a ResolvedResources>,
}

impl<'a> ResolveCtx<'a> {
    pub fn disabled() -> Self {
        Self {
            store: None,
            ctx: None,
            remote_resources: None,
        }
    }
}

/// Body bytes and metadata fetched from a remote `http(s)` rule resource.
#[derive(Debug, Clone)]
pub struct ResolvedResource {
    pub body: Bytes,
    pub content_type: Option<String>,
}

pub type ResolvedResources = HashMap<String, ResolvedResource>;

/// Remote `http(s)` resource URLs referenced by request-stage rules.
pub fn remote_resource_urls_for_request(matched_rules: &[MatchedRule]) -> Vec<String> {
    let mut urls = Vec::new();
    for matched in matched_rules {
        if !matched.rule.enabled {
            continue;
        }
        for action in &matched.rule.actions {
            match action {
                RuleAction::File { path } => {
                    collect_remote_resource_url_from_path(path, &matched.remaining_path, &mut urls);
                }
                RuleAction::RequestBody { content } => {
                    collect_remote_resource_url_from_body_content(
                        content,
                        &matched.remaining_path,
                        &mut urls,
                    );
                }
                RuleAction::RequestMerge { content } => {
                    collect_remote_resource_url_from_body_content(
                        content,
                        &matched.remaining_path,
                        &mut urls,
                    );
                }
                _ => {}
            }
        }
    }
    urls.sort();
    urls.dedup();
    urls
}

/// Remote `http(s)` resource URLs referenced by response-stage rules.
pub fn remote_resource_urls_for_response(matched_rules: &[MatchedRule]) -> Vec<String> {
    remote_resource_urls_for_response_with_context(matched_rules, None)
}

/// Remote `http(s)` resource URLs referenced by response-stage rules whose
/// response filters match the supplied response context.
#[allow(clippy::too_many_arguments)]
pub fn remote_resource_urls_for_response_context(
    matched_rules: &[MatchedRule],
    url: &str,
    method: &str,
    request_headers: &HashMap<String, String>,
    response_headers: &HashMap<String, String>,
    status_code: u16,
    content_type: Option<&str>,
) -> Vec<String> {
    remote_resource_urls_for_response_with_context(
        matched_rules,
        Some(ResponseResourceFilterCtx {
            url,
            method,
            request_headers,
            response_headers,
            status_code,
            content_type,
        }),
    )
}

#[derive(Clone, Copy)]
struct ResponseResourceFilterCtx<'a> {
    url: &'a str,
    method: &'a str,
    request_headers: &'a HashMap<String, String>,
    response_headers: &'a HashMap<String, String>,
    status_code: u16,
    content_type: Option<&'a str>,
}

fn remote_resource_urls_for_response_with_context(
    matched_rules: &[MatchedRule],
    filter_ctx: Option<ResponseResourceFilterCtx<'_>>,
) -> Vec<String> {
    let mut urls = Vec::new();
    let parsed_url = filter_ctx.and_then(|ctx| Url::parse(ctx.url).ok());
    for matched in matched_rules {
        if !matched.rule.enabled {
            continue;
        }
        if let (Some(filters), Some(ctx)) = (&matched.rule.filters, filter_ctx) {
            let protocol = parsed_url.as_ref().map(|u| u.scheme()).unwrap_or("http");
            let port = parsed_url
                .as_ref()
                .and_then(|u| u.port())
                .unwrap_or(if protocol == "https" { 443 } else { 80 });

            if !filters.matches_response(
                ctx.method,
                protocol,
                port,
                ctx.request_headers,
                ctx.response_headers,
                ctx.url,
                ctx.status_code,
                ctx.content_type,
            ) {
                continue;
            }
        }
        for action in &matched.rule.actions {
            match action {
                RuleAction::ResponseBody { content } => {
                    collect_remote_resource_url_from_body_content(
                        content,
                        &matched.remaining_path,
                        &mut urls,
                    );
                }
                RuleAction::ResponseMerge { content } => {
                    collect_remote_resource_url_from_body_content(
                        content,
                        &matched.remaining_path,
                        &mut urls,
                    );
                }
                RuleAction::Mock { path } => {
                    collect_remote_resource_url_from_path(path, &matched.remaining_path, &mut urls);
                }
                _ => {}
            }
        }
    }
    urls.sort();
    urls.dedup();
    urls
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

fn normalize_operation_value(arg: &str) -> &str {
    let trimmed = arg.trim();
    trimmed
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(arg)
}

fn effective_content_type<'a>(
    headers: &'a HashMap<String, String>,
    fallback: Option<&'a str>,
) -> Option<&'a str> {
    headers.get("content-type").map(String::as_str).or(fallback)
}

const DEBUG_BLOCKING_RESPONSE_HEADERS: [&str; 4] = [
    "content-security-policy",
    "content-security-policy-report-only",
    "x-content-security-policy",
    "x-webkit-csp",
];

fn remove_debug_blocking_headers(modification: &mut ResponseModification) {
    for header in DEBUG_BLOCKING_RESPONSE_HEADERS {
        modification.headers.remove(header);
        if !modification
            .headers_to_remove
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(header))
        {
            modification.headers_to_remove.push(header.to_string());
        }
    }
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
    /// Modified request method
    pub method: Option<String>,
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
    /// Files that should receive the final request body.
    pub write_files: Vec<BodyWriteTarget>,
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
    /// Files that should receive the request/response body.
    pub write_files: Vec<BodyWriteTarget>,
}

#[derive(Debug, Clone)]
pub struct BodyWriteTarget {
    pub path: String,
    pub raw: bool,
    pub remaining_path: String,
}

/// Data needed to serialize a request for whistle `reqWriteRaw://`.
pub struct RequestWriteContext<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a HashMap<String, String>,
    pub body: &'a Bytes,
    pub force: bool,
}

/// Data needed to serialize a response for whistle `resWriteRaw://`.
pub struct ResponseWriteContext<'a> {
    pub method: &'a str,
    pub status: u16,
    pub headers: &'a HashMap<String, String>,
    pub body: &'a Bytes,
    pub force: bool,
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
                | RuleAction::ResponseMerge { .. }
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
                RuleAction::ResponseWrite { .. } => return true,
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
                | RuleAction::RequestMerge { .. }
                | RuleAction::RequestReplace { .. }
                | RuleAction::RequestPrepend { .. }
                | RuleAction::RequestAppend { .. }
                | RuleAction::RequestWrite { .. }
                | RuleAction::HttpProxy { .. }
                | RuleAction::HttpsProxy { .. }
                | RuleAction::SocksProxy { .. } => return true,
                RuleAction::Speed { request_kbps, .. } if request_kbps.is_some() => return true,
                RuleAction::Plugin { .. } => return true,
                _ => {}
            }
        }
    }
    false
}

/// Direct responses consume the incoming body before returning so HTTP/1
/// keep-alive remains reusable and request capture is complete.
pub fn rules_may_short_circuit_request(matched_rules: &[MatchedRule]) -> bool {
    matched_rules.iter().any(|matched| {
        matched.rule.enabled
            && matched.rule.actions.iter().any(|action| {
                matches!(
                    action,
                    RuleAction::File { .. }
                        | RuleAction::Redirect { .. }
                        | RuleAction::StatusCode { .. }
                )
            })
    })
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
    /// Allow reqWrite:// / reqWriteRaw:// to overwrite existing files.
    pub const FORCE_REQ_WRITE: &str = "forcereqwrite";
    /// Allow resWrite:// / resWriteRaw:// to overwrite existing files.
    pub const FORCE_RES_WRITE: &str = "forcereswrite";
    /// Allow reqMerge:// to process request bodies up to Whistle's 16 MiB
    /// expanded limit instead of the default 2 MiB cap.
    pub const REQ_MERGE_BIG_DATA: &str = "reqmergebigdata";
    /// Allow resMerge:// to process response bodies up to Whistle's 16 MiB
    /// expanded limit instead of the default 2 MiB cap.
    pub const RES_MERGE_BIG_DATA: &str = "resmergebigdata";
}

/// Returns true if `feature` is not in the modification's `disabled_features`.
/// Used at capture / emit sites to honor `disable://capture`.
pub fn capture_enabled(modification: &RequestModification) -> bool {
    !modification.ignore
        && !modification.disabled_features.iter().any(|f| {
            f.eq_ignore_ascii_case(feature::CAPTURE) || f.eq_ignore_ascii_case(feature::HIDE)
        })
}

/// Returns true if `enable://abort` is set, i.e. the proxy should terminate
/// this request without responding.
pub fn should_abort(modification: &RequestModification) -> bool {
    is_enabled(modification, feature::ABORT)
}

/// Returns true when a whistle feature flag is enabled for this request.
pub fn is_enabled(modification: &RequestModification, name: &str) -> bool {
    modification
        .enabled_features
        .iter()
        .any(|f| f.eq_ignore_ascii_case(name))
        && !modification
            .disabled_features
            .iter()
            .any(|f| f.eq_ignore_ascii_case(name))
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
                    modification.body = Some(resolve_body_content(
                        content,
                        &matched.remaining_path,
                        inline,
                        res,
                    ));
                }

                RuleAction::UrlParams { modifications } => {
                    if let Some(ref url) = parsed_url {
                        modification.query_params =
                            Some(apply_url_param_modifications(url, modifications));
                    }
                }

                RuleAction::RequestMerge { content } => {
                    let limit = merge_body_limit(matched_rules, feature::REQ_MERGE_BIG_DATA);
                    let current = modification.body.as_ref().cloned().unwrap_or_default();
                    if current.len() <= limit {
                        let merge =
                            resolve_body_content(content, &matched.remaining_path, inline, res);
                        let content_type =
                            modification.headers.get("content-type").map(String::as_str);
                        if let Some(merged) = merge_structured_body(&current, &merge, content_type)
                        {
                            modification.body = Some(merged);
                        }
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
                    modification.method = Some(new_method.clone());
                }

                RuleAction::RequestType { content_type } => {
                    modification
                        .headers
                        .insert("content-type".to_string(), content_type.clone());
                }

                RuleAction::RequestCharset { charset } => {
                    let value = modification
                        .headers
                        .entry("content-type".to_string())
                        .or_insert_with(|| "text/plain".to_string());
                    if !value.to_lowercase().contains("charset=") {
                        value.push_str(&format!("; charset={}", charset));
                    }
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
                    // The action argument may be a `{name}` value, inline
                    // `(content)`, a file, or a directory that appends the
                    // matched path remainder.
                    let (body, content_type) =
                        resolve_file_content(path, &matched.remaining_path, inline, res);

                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), content_type);
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body,
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

                RuleAction::RequestWrite { path, raw } => {
                    modification.write_files.push(BodyWriteTarget {
                        path: path.clone(),
                        raw: *raw,
                        remaining_path: matched.remaining_path.clone(),
                    });
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

                RuleAction::Unsupported { protocol, value } => {
                    tracing::warn!(
                        "Whistle protocol {}://{} is parsed but unsupported in PostGate",
                        protocol,
                        value
                    );
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
#[allow(clippy::too_many_arguments)]
pub fn apply_response_rules(
    matched_rules: &[MatchedRule],
    url: &str,
    method: &str,
    request_headers: &HashMap<String, String>,
    response_headers: &HashMap<String, String>,
    status_code: u16,
    body: Option<&Bytes>,
    content_type: Option<&str>,
) -> ResponseModification {
    apply_response_rules_with_values(
        matched_rules,
        url,
        method,
        request_headers,
        response_headers,
        status_code,
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
    status_code: u16,
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
    let can_modify_body = has_response_body(method, status_code);

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

            if !filters.matches_response(
                method,
                protocol,
                port,
                request_headers,
                response_headers,
                url,
                status_code,
                content_type,
            ) {
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

                RuleAction::ResponseBody { content } if can_modify_body => {
                    modification.body = Some(resolve_body_content(
                        content,
                        &matched.remaining_path,
                        inline,
                        res,
                    ));
                }

                RuleAction::ResponseMerge { content } if can_modify_body => {
                    let current = modification.body.as_ref().cloned().unwrap_or_default();
                    let limit = merge_body_limit(matched_rules, feature::RES_MERGE_BIG_DATA);
                    if current.len() <= limit {
                        let merge =
                            resolve_body_content(content, &matched.remaining_path, inline, res);
                        let effective = effective_content_type(&modification.headers, content_type);
                        if let Some(merged) = merge_structured_body(&current, &merge, effective) {
                            modification.body = Some(merged);
                        }
                    }
                }

                RuleAction::ResponseReplace {
                    pattern,
                    replacement,
                    regex,
                } if can_modify_body => {
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

                RuleAction::Cache { policy } => {
                    let policy = resolve_ref(policy, inline, res);
                    apply_cache_policy(
                        &mut modification.headers,
                        &mut modification.headers_to_remove,
                        &policy,
                    );
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

                RuleAction::HtmlBody { content } if can_modify_body => {
                    if is_html(effective_content_type(&modification.headers, content_type)) {
                        modification
                            .headers
                            .insert("content-type".to_string(), content_type_for_html());
                        modification.body = Some(resolve_bytes(content, inline, res));
                    }
                }

                RuleAction::JsBody { content } if can_modify_body => {
                    if is_html(effective_content_type(&modification.headers, content_type)) {
                        let resolved = resolve_ref(content, inline, res);
                        modification
                            .headers
                            .insert("content-type".to_string(), content_type_for_html());
                        modification.body =
                            Some(Bytes::from(format!("<script>{}</script>", resolved)));
                    } else if is_js(effective_content_type(&modification.headers, content_type)) {
                        modification
                            .headers
                            .insert("content-type".to_string(), content_type_for_js());
                        modification.body = Some(resolve_bytes(content, inline, res));
                    }
                }

                RuleAction::CssBody { content } if can_modify_body => {
                    if is_html(effective_content_type(&modification.headers, content_type)) {
                        let resolved = resolve_ref(content, inline, res);
                        modification
                            .headers
                            .insert("content-type".to_string(), content_type_for_html());
                        modification.body =
                            Some(Bytes::from(format!("<style>{}</style>", resolved)));
                    } else if is_css(effective_content_type(&modification.headers, content_type)) {
                        modification
                            .headers
                            .insert("content-type".to_string(), content_type_for_css());
                        modification.body = Some(resolve_bytes(content, inline, res));
                    }
                }

                RuleAction::HtmlAppend { content } if can_modify_body => {
                    if is_html(effective_content_type(&modification.headers, content_type)) {
                        let resolved = resolve_ref(content, inline, res);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &resolved, false));
                    }
                }

                RuleAction::HtmlPrepend { content } if can_modify_body => {
                    if is_html(effective_content_type(&modification.headers, content_type)) {
                        let resolved = resolve_ref(content, inline, res);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &resolved, true));
                    }
                }

                RuleAction::JsAppend { content } if can_modify_body => {
                    let resolved = resolve_ref(content, inline, res);
                    let effective_ct = effective_content_type(&modification.headers, content_type);
                    if is_html(effective_ct) {
                        let script = format!("<script>{}</script>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &script, false));
                    } else if is_js(effective_ct) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = body.to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(resolved.as_bytes());
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::JsPrepend { content } if can_modify_body => {
                    let resolved = resolve_ref(content, inline, res);
                    let effective_ct = effective_content_type(&modification.headers, content_type);
                    if is_html(effective_ct) {
                        let script = format!("<script>{}</script>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &script, true));
                    } else if is_js(effective_ct) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = resolved.as_bytes().to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(body);
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::CssAppend { content } if can_modify_body => {
                    let resolved = resolve_ref(content, inline, res);
                    let effective_ct = effective_content_type(&modification.headers, content_type);
                    if is_html(effective_ct) {
                        let style = format!("<style>{}</style>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &style, false));
                    } else if is_css(effective_ct) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = body.to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(resolved.as_bytes());
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::CssPrepend { content } if can_modify_body => {
                    let resolved = resolve_ref(content, inline, res);
                    let effective_ct = effective_content_type(&modification.headers, content_type);
                    if is_html(effective_ct) {
                        let style = format!("<style>{}</style>", resolved);
                        modification.body =
                            Some(append_to_html(modification.body.as_ref(), &style, true));
                    } else if is_css(effective_ct) {
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

                RuleAction::Debug { .. } if can_modify_body => {
                    // Enable debug script injection for HTML responses
                    if is_html(effective_content_type(&modification.headers, content_type)) {
                        modification.inject_debug = true;
                        // The injected bridge needs inline execution, a
                        // localhost WebSocket, and the bundled Chobitsu asset.
                        // A response CSP would otherwise make debug:// appear
                        // active while silently preventing the CDP connection.
                        remove_debug_blocking_headers(&mut modification);
                    }
                }

                // `jsonBody://{value}` — short-circuit with a JSON response.
                RuleAction::JsonBody { value } if can_modify_body => {
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
                RuleAction::Echo if can_modify_body => {
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
                RuleAction::Mock { path } if can_modify_body => {
                    let (body, guessed_ct) =
                        resolve_file_content(path, &matched.remaining_path, inline, res);
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
                } if can_modify_body => {
                    if is_html(effective_content_type(&modification.headers, content_type)) {
                        apply_body_replace(&mut modification.body, pattern, replacement, *regex);
                    }
                }
                RuleAction::JsReplace {
                    pattern,
                    replacement,
                    regex,
                } if can_modify_body => {
                    if is_js(effective_content_type(&modification.headers, content_type)) {
                        apply_body_replace(&mut modification.body, pattern, replacement, *regex);
                    }
                }
                RuleAction::CssReplace {
                    pattern,
                    replacement,
                    regex,
                } if can_modify_body => {
                    if is_css(effective_content_type(&modification.headers, content_type)) {
                        apply_body_replace(&mut modification.body, pattern, replacement, *regex);
                    }
                }

                // `resPrepend://` / `resAppend://` — plain text prepend/append
                // to the response body. Unlike the `html*` / `js*` / `css*`
                // variants these are content-type agnostic.
                RuleAction::ResponsePrepend { content } if can_modify_body => {
                    let extra = resolve_bytes(content, inline, res);
                    let current = modification.body.clone().unwrap_or_default();
                    let mut combined = Vec::with_capacity(extra.len() + current.len());
                    combined.extend_from_slice(&extra);
                    combined.extend_from_slice(&current);
                    modification.body = Some(Bytes::from(combined));
                }
                RuleAction::ResponseAppend { content } if can_modify_body => {
                    let extra = resolve_bytes(content, inline, res);
                    let current = modification.body.clone().unwrap_or_default();
                    let mut combined = Vec::with_capacity(extra.len() + current.len());
                    combined.extend_from_slice(&current);
                    combined.extend_from_slice(&extra);
                    modification.body = Some(Bytes::from(combined));
                }
                RuleAction::ResponseWrite { path, raw } => {
                    modification.write_files.push(BodyWriteTarget {
                        path: path.clone(),
                        raw: *raw,
                        remaining_path: matched.remaining_path.clone(),
                    });
                }
                RuleAction::ResponseFor { value } => {
                    modification
                        .headers
                        .insert("x-whistle-response-for".to_string(), value.clone());
                }
                RuleAction::Unsupported { protocol, value } => {
                    tracing::warn!(
                        "Whistle protocol {}://{} is parsed but unsupported in PostGate",
                        protocol,
                        value
                    );
                }

                _ => {
                    // Request-only actions are skipped here
                }
            }
        }
    }

    modification
}

pub fn persist_request_writes(targets: &[BodyWriteTarget], ctx: RequestWriteContext<'_>) {
    for target in targets {
        if !target.raw && !has_request_body(ctx.method) {
            continue;
        }
        let data = if target.raw {
            serialize_raw_request(ctx.method, ctx.url, ctx.headers, ctx.body)
        } else {
            ctx.body.clone()
        };
        if let Err(e) =
            write_body_file(&target.path, &target.remaining_path, &data, ctx.force, None)
        {
            tracing::warn!(
                "{} request write failed for {}: {}",
                if target.raw { "raw" } else { "body" },
                target.path,
                e
            );
        }
    }
}

pub fn persist_response_writes(targets: &[BodyWriteTarget], ctx: ResponseWriteContext<'_>) {
    for target in targets {
        if !target.raw && !has_response_body(ctx.method, ctx.status) {
            continue;
        }
        let data = if target.raw {
            serialize_raw_response(ctx.status, ctx.headers, ctx.body)
        } else {
            ctx.body.clone()
        };
        if let Err(e) = write_body_file(
            &target.path,
            &target.remaining_path,
            &data,
            ctx.force,
            Some(ctx.status),
        ) {
            tracing::warn!(
                "{} response write failed for {}: {}",
                if target.raw { "raw" } else { "body" },
                target.path,
                e
            );
        }
    }
}

fn write_body_file(
    path: &str,
    remaining_path: &str,
    body: &Bytes,
    force: bool,
    response_status: Option<u16>,
) -> std::io::Result<()> {
    let Some(path) = normalize_write_path(path, remaining_path, response_status) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    if !force && path.try_exists()? {
        return Ok(());
    }
    std::fs::write(path, body)
}

fn normalize_write_path(
    raw_path: &str,
    remaining_path: &str,
    response_status: Option<u16>,
) -> Option<PathBuf> {
    let raw_path = raw_path.trim();
    if raw_path.is_empty() {
        return None;
    }

    let mut path = join_write_path(raw_path, remaining_path);
    if let Some(status) = response_status {
        if status != 200 {
            path = PathBuf::from(format!("{}.{}", path.display(), status));
        }
    }

    if path_ends_with_separator(&path.to_string_lossy()) {
        path.push("index.html");
    }

    Some(path)
}

fn join_write_path(raw_path: &str, remaining_path: &str) -> PathBuf {
    let mut path = PathBuf::from(raw_path);
    let mut rest = remaining_path;
    if rest.is_empty() {
        return path;
    }

    if path_ends_with_separator(raw_path) {
        let rest_ends_with_sep = path_ends_with_separator(rest);
        rest = rest.trim_start_matches(['/', '\\']);
        for part in rest.split(['/', '\\']).filter(|part| !part.is_empty()) {
            if part == "." || part == ".." {
                continue;
            }
            path.push(part);
        }
        if rest_ends_with_sep && !path_ends_with_separator(&path.to_string_lossy()) {
            path.push("");
        }
    }

    path
}

fn path_ends_with_separator(raw: &str) -> bool {
    raw.ends_with('/') || raw.ends_with('\\')
}

fn has_request_body(method: &str) -> bool {
    !matches!(
        method.to_ascii_uppercase().as_str(),
        "GET" | "HEAD" | "OPTIONS" | "CONNECT"
    )
}

fn has_response_body(method: &str, status: u16) -> bool {
    !method.eq_ignore_ascii_case("HEAD")
        && status != 204
        && !(300..400).contains(&status)
        && !(100..=199).contains(&status)
}

fn serialize_raw_request(
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    body: &Bytes,
) -> Bytes {
    let mut raw = format!("{} {} HTTP/1.1", method, request_target_for_raw(url));
    append_headers(&mut raw, headers, true);
    raw.push_str("\r\n\r\n");
    let mut bytes = raw.into_bytes();
    bytes.extend_from_slice(body);
    Bytes::from(bytes)
}

fn serialize_raw_response(status: u16, headers: &HashMap<String, String>, body: &Bytes) -> Bytes {
    let mut raw = format!(
        "HTTP/1.1 {} {}",
        status,
        http_status_reason(status).unwrap_or_default()
    );
    append_headers(&mut raw, headers, false);
    raw.push_str("\r\n\r\n");
    let mut bytes = raw.into_bytes();
    bytes.extend_from_slice(body);
    Bytes::from(bytes)
}

fn request_target_for_raw(url: &str) -> String {
    Url::parse(url)
        .ok()
        .map(|parsed| {
            let mut target = parsed.path().to_string();
            if target.is_empty() {
                target.push('/');
            }
            if let Some(query) = parsed.query() {
                target.push('?');
                target.push_str(query);
            }
            target
        })
        .unwrap_or_else(|| url.to_string())
}

fn append_headers(out: &mut String, headers: &HashMap<String, String>, is_request: bool) {
    let mut keys: Vec<&String> = headers.keys().collect();
    keys.sort_unstable();
    for key in keys {
        let lower = key.to_ascii_lowercase();
        if is_request && lower == "content-encoding" {
            continue;
        }
        if lower.starts_with(':') {
            continue;
        }
        if let Some(value) = headers.get(key) {
            out.push_str("\r\n");
            out.push_str(key);
            out.push_str(": ");
            out.push_str(value);
        }
    }
}

fn http_status_reason(status: u16) -> Option<&'static str> {
    match status {
        100 => Some("Continue"),
        101 => Some("Switching Protocols"),
        102 => Some("Processing"),
        103 => Some("Early Hints"),
        200 => Some("OK"),
        201 => Some("Created"),
        202 => Some("Accepted"),
        203 => Some("Non-Authoritative Information"),
        204 => Some("No Content"),
        205 => Some("Reset Content"),
        206 => Some("Partial Content"),
        207 => Some("Multi-Status"),
        208 => Some("Already Reported"),
        226 => Some("IM Used"),
        300 => Some("Multiple Choices"),
        301 => Some("Moved Permanently"),
        302 => Some("Found"),
        303 => Some("See Other"),
        304 => Some("Not Modified"),
        305 => Some("Use Proxy"),
        307 => Some("Temporary Redirect"),
        308 => Some("Permanent Redirect"),
        400 => Some("Bad Request"),
        401 => Some("Unauthorized"),
        402 => Some("Payment Required"),
        403 => Some("Forbidden"),
        404 => Some("Not Found"),
        405 => Some("Method Not Allowed"),
        406 => Some("Not Acceptable"),
        407 => Some("Proxy Authentication Required"),
        408 => Some("Request Timeout"),
        409 => Some("Conflict"),
        410 => Some("Gone"),
        411 => Some("Length Required"),
        412 => Some("Precondition Failed"),
        413 => Some("Payload Too Large"),
        414 => Some("URI Too Long"),
        415 => Some("Unsupported Media Type"),
        416 => Some("Range Not Satisfiable"),
        417 => Some("Expectation Failed"),
        418 => Some("I'm a teapot"),
        421 => Some("Misdirected Request"),
        422 => Some("Unprocessable Entity"),
        423 => Some("Locked"),
        424 => Some("Failed Dependency"),
        425 => Some("Too Early"),
        426 => Some("Upgrade Required"),
        428 => Some("Precondition Required"),
        429 => Some("Too Many Requests"),
        431 => Some("Request Header Fields Too Large"),
        451 => Some("Unavailable For Legal Reasons"),
        500 => Some("Internal Server Error"),
        501 => Some("Not Implemented"),
        502 => Some("Bad Gateway"),
        503 => Some("Service Unavailable"),
        504 => Some("Gateway Timeout"),
        505 => Some("HTTP Version Not Supported"),
        506 => Some("Variant Also Negotiates"),
        507 => Some("Insufficient Storage"),
        508 => Some("Loop Detected"),
        510 => Some("Not Extended"),
        511 => Some("Network Authentication Required"),
        _ => None,
    }
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
    remaining_path: &str,
    inline: &HashMap<String, String>,
    res: &ResolveCtx<'_>,
) -> Bytes {
    match content {
        BodyContent::Text { content, .. } => {
            resolve_bytes(normalize_operation_value(content), inline, res)
        }
        BodyContent::Json { value } => {
            let raw = serde_json::to_string(value).unwrap_or_default();
            resolve_bytes(&raw, inline, res)
        }
        BodyContent::File { path } => {
            let (body, _) = resolve_file_content(path, remaining_path, inline, res);
            body
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
    let Some(ct) = content_type else {
        return false;
    };
    let ct = ct.to_ascii_lowercase();
    ct.contains("text/html") || ct.contains("application/xhtml")
}

/// Check if content type is JavaScript
fn is_js(content_type: Option<&str>) -> bool {
    let Some(ct) = content_type else {
        return false;
    };
    let ct = ct.to_ascii_lowercase();
    ct.contains("javascript")
        || ct.contains("ecmascript")
        || ct.contains("text/js")
        || ct.contains("application/x-javascript")
}

/// Check if content type is CSS
fn is_css(content_type: Option<&str>) -> bool {
    let Some(ct) = content_type else {
        return false;
    };
    ct.to_ascii_lowercase().contains("text/css")
}

fn content_type_for_html() -> String {
    "text/html; charset=utf-8".to_string()
}

fn content_type_for_js() -> String {
    "application/javascript; charset=utf-8".to_string()
}

fn content_type_for_css() -> String {
    "text/css; charset=utf-8".to_string()
}

fn content_type_for_text() -> String {
    "text/plain; charset=utf-8".to_string()
}

fn collect_remote_resource_url_from_body_content(
    content: &BodyContent,
    remaining_path: &str,
    urls: &mut Vec<String>,
) {
    if let BodyContent::File { path } = content {
        collect_remote_resource_url_from_path(path, remaining_path, urls);
    }
}

fn collect_remote_resource_url_from_path(
    path: &std::path::Path,
    remaining_path: &str,
    urls: &mut Vec<String>,
) {
    let path_str = path.to_string_lossy();
    if let Some(url) = remote_resource_url(path_str.trim(), remaining_path) {
        urls.push(url);
    }
}

fn merge_body_limit(matched_rules: &[MatchedRule], feature_name: &str) -> usize {
    let mut enabled = false;
    for matched in matched_rules {
        for action in &matched.rule.actions {
            match action {
                RuleAction::Enable { features }
                    if features.iter().any(|feature| feature == feature_name) =>
                {
                    enabled = true;
                }
                RuleAction::Disable { features }
                    if features.iter().any(|feature| feature == feature_name) =>
                {
                    enabled = false;
                }
                _ => {}
            }
        }
    }
    if enabled {
        BIG_MERGE_BODY_LIMIT
    } else {
        DEFAULT_MERGE_BODY_LIMIT
    }
}

fn merge_structured_body(
    current: &Bytes,
    merge: &Bytes,
    content_type: Option<&str>,
) -> Option<Bytes> {
    let merge_value = parse_merge_value(merge)?;
    let content_type = content_type.unwrap_or_default().to_ascii_lowercase();

    if content_type.contains("application/x-www-form-urlencoded") {
        return merge_form_body(current, merge_value);
    }

    if !(content_type.is_empty()
        || content_type.contains("json")
        || content_type.contains("javascript")
        || content_type.contains("html"))
    {
        return None;
    }

    merge_json_or_jsonp(current, merge_value)
}

fn parse_merge_value(raw: &Bytes) -> Option<serde_json::Value> {
    let text = String::from_utf8_lossy(raw);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        if value.is_object() {
            return Some(value);
        }
    }

    let mut object = serde_json::Map::new();
    if text.contains('=') {
        for (key, value) in url::form_urlencoded::parse(text.as_bytes()) {
            insert_dotted_value(&mut object, &key, parse_scalar_value(&value));
        }
    } else {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            insert_dotted_value(&mut object, key.trim(), parse_scalar_value(value.trim()));
        }
    }

    (!object.is_empty()).then_some(serde_json::Value::Object(object))
}

fn parse_scalar_value(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()))
}

fn split_dotted_key(key: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut escaped = false;
    for ch in key.chars() {
        if escaped {
            part.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '.' {
            parts.push(std::mem::take(&mut part));
        } else {
            part.push(ch);
        }
    }
    if escaped {
        part.push('\\');
    }
    parts.push(part);
    parts.retain(|part| !part.is_empty());
    parts
}

fn insert_dotted_value(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    let parts = split_dotted_key(key);
    insert_value_path(object, &parts, value);
}

fn insert_value_path(
    object: &mut serde_json::Map<String, serde_json::Value>,
    parts: &[String],
    value: serde_json::Value,
) {
    let Some((head, tail)) = parts.split_first() else {
        return;
    };
    if tail.is_empty() {
        object.insert(head.clone(), value);
        return;
    }

    let entry = object
        .entry(head.clone())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(child) = entry.as_object_mut() {
        insert_value_path(child, tail, value);
    }
}

fn deep_merge_json(target: &mut serde_json::Value, patch: serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(&key) {
                    Some(existing) => deep_merge_json(existing, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, patch) => *target = patch,
    }
}

fn merge_json_or_jsonp(current: &Bytes, patch: serde_json::Value) -> Option<Bytes> {
    let text = String::from_utf8_lossy(current);
    if text.trim().is_empty() {
        return serde_json::to_vec(&patch).ok().map(Bytes::from);
    }

    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) {
        deep_merge_json(&mut value, patch);
        return serde_json::to_vec(&value).ok().map(Bytes::from);
    }

    let (start, end) = jsonp_payload_range(&text)?;
    let mut value = serde_json::from_str::<serde_json::Value>(&text[start..end]).ok()?;
    deep_merge_json(&mut value, patch);
    let replacement = serde_json::to_string(&value).ok()?;
    let mut output = String::with_capacity(text.len() + replacement.len());
    output.push_str(&text[..start]);
    output.push_str(&replacement);
    output.push_str(&text[end..]);
    Some(Bytes::from(output))
}

fn jsonp_payload_range(text: &str) -> Option<(usize, usize)> {
    let start = text.find(['{', '['])?;
    let open = text.as_bytes()[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, byte) in text.as_bytes()[start..].iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        if byte == b'"' {
            in_string = true;
        } else if byte == open {
            depth += 1;
        } else if byte == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some((start, start + offset + 1));
            }
        }
    }
    None
}

fn merge_form_body(current: &Bytes, patch: serde_json::Value) -> Option<Bytes> {
    let mut fields: HashMap<String, String> = url::form_urlencoded::parse(current)
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let patch = patch.as_object()?;
    for (key, value) in patch {
        let value = match value {
            serde_json::Value::String(value) => value.clone(),
            other => other.to_string(),
        };
        fields.insert(key.clone(), value);
    }

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    let mut fields: Vec<_> = fields.into_iter().collect();
    fields.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    serializer.extend_pairs(fields);
    Some(Bytes::from(serializer.finish()))
}

fn apply_cache_policy(
    headers: &mut HashMap<String, String>,
    headers_to_remove: &mut Vec<String>,
    policy: &str,
) {
    let policy = policy.trim().to_ascii_lowercase();
    if matches!(policy.as_str(), "keep" | "reserve") {
        return;
    }

    let max_age = policy.parse::<i64>().ok();
    let no_cache = matches!(policy.as_str(), "no" | "no-cache" | "no-store")
        || max_age.is_some_and(|age| age < 0);
    if max_age.is_none() && !no_cache {
        return;
    }

    if no_cache {
        let value = if policy == "no-store" {
            "no-store"
        } else {
            "no-cache"
        };
        headers.insert("cache-control".to_string(), value.to_string());
        headers.insert("pragma".to_string(), "no-cache".to_string());
        let expires = SystemTime::now()
            .checked_sub(Duration::from_secs(60_000))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        headers.insert("expires".to_string(), httpdate::fmt_http_date(expires));
        return;
    }

    let seconds = max_age.unwrap_or_default() as u64;
    headers.insert("cache-control".to_string(), format!("max-age={seconds}"));
    let expires = SystemTime::now()
        .checked_add(Duration::from_secs(seconds))
        .unwrap_or(SystemTime::now());
    headers.insert("expires".to_string(), httpdate::fmt_http_date(expires));
    headers.remove("pragma");
    if !headers_to_remove.iter().any(|header| header == "pragma") {
        headers_to_remove.push("pragma".to_string());
    }
}

fn resolve_file_content(
    path: &std::path::Path,
    remaining_path: &str,
    inline: &HashMap<String, String>,
    res: &ResolveCtx<'_>,
) -> (Bytes, String) {
    let path_str = path.to_string_lossy();
    let trimmed = path_str.trim();
    let inline_value = normalize_operation_value(trimmed);
    if inline_value != trimmed {
        return (
            resolve_bytes(inline_value, inline, res),
            content_type_for_text(),
        );
    }

    if let Some(url) = remote_resource_url(trimmed, remaining_path) {
        if let Some(resource) = remote_resource(&url, res) {
            return (
                resource.body.clone(),
                resource
                    .content_type
                    .clone()
                    .unwrap_or_else(|| content_type_for_resource_url(&url)),
            );
        }
        return (Bytes::new(), content_type_for_resource_url(&url));
    }

    let is_ref = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with("`{") && trimmed.ends_with("}`"));
    if is_ref {
        let body = resolve_bytes(trimmed, inline, res);
        let name_for_mime = trimmed
            .trim_matches('`')
            .trim_start_matches('{')
            .trim_end_matches('}');
        let ct = mime_guess::from_path(name_for_mime)
            .first_or_octet_stream()
            .to_string();
        return (body, ct);
    }

    let resolved_path = resolve_local_file_path(path, remaining_path);
    let body = std::fs::read(&resolved_path)
        .map(Bytes::from)
        .unwrap_or_default();
    let ct = mime_guess::from_path(&resolved_path)
        .first_or_octet_stream()
        .to_string();
    (body, ct)
}

fn remote_resource(url: &str, res: &ResolveCtx<'_>) -> Option<ResolvedResource> {
    res.remote_resources
        .and_then(|resources| resources.get(url))
        .cloned()
}

fn remote_resource_url(raw: &str, remaining_path: &str) -> Option<String> {
    let trimmed = raw.trim();
    if normalize_operation_value(trimmed) != trimmed {
        return None;
    }

    let mut url = Url::parse(trimmed).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);

    if should_join_remaining_path(trimmed, &url) {
        let rest = remaining_path
            .split(['?', '#'])
            .next()
            .unwrap_or(remaining_path)
            .trim_start_matches(['/', '\\']);
        if !rest.is_empty() {
            url = url.join(rest).ok()?;
        }
    }

    Some(url.to_string())
}

fn should_join_remaining_path(raw: &str, url: &Url) -> bool {
    raw.ends_with('/') || url.path().ends_with('/')
}

fn content_type_for_resource_url(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|url| {
            mime_guess::from_path(url.path())
                .first()
                .map(|mime| mime.to_string())
        })
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

fn resolve_local_file_path(path: &std::path::Path, remaining_path: &str) -> PathBuf {
    let base = expand_home(path);
    if path_ends_with_separator(&base.to_string_lossy()) || base.is_dir() {
        return join_file_path(&base, remaining_path);
    }
    base
}

fn expand_home(path: &std::path::Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" || raw.starts_with("~/") {
        if let Some(home) = env::var_os("HOME") {
            let suffix = raw.strip_prefix("~/").unwrap_or("");
            return PathBuf::from(home).join(suffix);
        }
    }
    path.to_path_buf()
}

fn join_file_path(base: &std::path::Path, remaining_path: &str) -> PathBuf {
    let rest = remaining_path
        .split(['?', '#'])
        .next()
        .unwrap_or(remaining_path)
        .trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        base.to_path_buf()
    } else {
        base.join(rest)
    }
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
    use crate::rules::engine::MatchedRule;
    use crate::rules::types::{Pattern, Rule, RuleFilters};
    use std::fs;
    use std::sync::Arc;

    fn matched_rule(action: RuleAction) -> MatchedRule {
        matched_rule_with_filters(action, None)
    }

    fn matched_rule_with_filters(action: RuleAction, filters: Option<RuleFilters>) -> MatchedRule {
        matched_rule_with_actions_and_filters(vec![action], filters)
    }

    fn matched_rule_with_actions(actions: Vec<RuleAction>) -> MatchedRule {
        matched_rule_with_actions_and_filters(actions, None)
    }

    fn matched_rule_with_actions_and_filters(
        actions: Vec<RuleAction>,
        filters: Option<RuleFilters>,
    ) -> MatchedRule {
        matched_rule_with_actions_filters_and_remainder(actions, filters, "")
    }

    fn matched_rule_with_actions_filters_and_remainder(
        actions: Vec<RuleAction>,
        filters: Option<RuleFilters>,
        remaining_path: impl Into<String>,
    ) -> MatchedRule {
        MatchedRule {
            rule: Arc::new(Rule {
                id: "t".into(),
                pattern: Pattern::Domain("example.com".into()),
                filters,
                actions,
                enabled: true,
                priority: 0,
                raw_line: String::new(),
                negated: false,
            }),
            remaining_path: remaining_path.into(),
            inline_values: Arc::new(HashMap::new()),
        }
    }

    fn body_write_target(path: impl Into<String>, raw: bool) -> BodyWriteTarget {
        BodyWriteTarget {
            path: path.into(),
            raw,
            remaining_path: String::new(),
        }
    }

    fn body_write_target_with_remainder(
        path: impl Into<String>,
        raw: bool,
        remaining_path: impl Into<String>,
    ) -> BodyWriteTarget {
        BodyWriteTarget {
            path: path.into(),
            raw,
            remaining_path: remaining_path.into(),
        }
    }

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
    fn test_request_merge_updates_json_and_form_bodies() {
        let json_rules = vec![matched_rule(RuleAction::RequestMerge {
            content: BodyContent::Text {
                content: "user.name=postgate&enabled=true".into(),
                content_type: "text/plain".into(),
            },
        })];
        let json_headers =
            HashMap::from([("content-type".to_string(), "application/json".to_string())]);
        let json_body = Bytes::from_static(br#"{"user":{"id":7},"keep":true}"#);
        let json = apply_request_rules(
            &json_rules,
            "https://example.com/api",
            "POST",
            &json_headers,
            Some(&json_body),
        );
        let value: serde_json::Value = serde_json::from_slice(json.body.as_ref().unwrap()).unwrap();
        assert_eq!(value["user"]["id"], 7);
        assert_eq!(value["user"]["name"], "postgate");
        assert_eq!(value["enabled"], true);
        assert_eq!(value["keep"], true);

        let form_headers = HashMap::from([(
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )]);
        let form = apply_request_rules(
            &json_rules,
            "https://example.com/api",
            "POST",
            &form_headers,
            Some(&Bytes::from_static(b"keep=1&enabled=false")),
        );
        let fields: HashMap<_, _> = url::form_urlencoded::parse(form.body.as_ref().unwrap())
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        assert_eq!(fields.get("keep").map(String::as_str), Some("1"));
        assert_eq!(fields.get("enabled").map(String::as_str), Some("true"));
    }

    #[test]
    fn test_response_merge_preserves_jsonp_wrapper_and_cache_policy() {
        let rules = vec![
            matched_rule(RuleAction::ResponseMerge {
                content: BodyContent::Text {
                    content: r#"nested.value=2&added=true"#.into(),
                    content_type: "text/plain".into(),
                },
            }),
            matched_rule(RuleAction::Cache {
                policy: "60".into(),
            }),
        ];
        let response = apply_response_rules(
            &rules,
            "https://example.com/api",
            "GET",
            &HashMap::new(),
            &HashMap::from([("pragma".to_string(), "no-cache".to_string())]),
            200,
            Some(&Bytes::from_static(b"callback({\"nested\":{\"keep\":1}});")),
            Some("application/javascript"),
        );
        assert_eq!(
            response.headers.get("cache-control").map(String::as_str),
            Some("max-age=60")
        );
        assert!(response.headers_to_remove.contains(&"pragma".to_string()));
        let body = String::from_utf8(response.body.unwrap().to_vec()).unwrap();
        assert!(body.starts_with("callback("));
        assert!(body.ends_with(");"));
        assert!(body.contains("\"keep\":1"));
        assert!(body.contains("\"value\":2"));
        assert!(body.contains("\"added\":true"));
    }

    #[test]
    fn test_apply_request_rules_sets_method_url_and_body_write() {
        let rules = vec![
            matched_rule(RuleAction::Method {
                method: "POST".into(),
            }),
            matched_rule(RuleAction::RequestType {
                content_type: "application/json".into(),
            }),
            matched_rule(RuleAction::RequestCharset {
                charset: "utf-8".into(),
            }),
            matched_rule(RuleAction::PathReplace {
                pattern: "/v1".into(),
                replacement: "/v2".into(),
            }),
            matched_rule(RuleAction::UrlParams {
                modifications: UrlParamModifications {
                    set: [("debug".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                    remove: vec!["old".to_string()],
                    append: HashMap::new(),
                },
            }),
            matched_rule(RuleAction::RequestWrite {
                path: "/tmp/postgate-req.bin".into(),
                raw: true,
            }),
        ];

        let modification = apply_request_rules(
            &rules,
            "https://example.com/v1/users?old=1",
            "GET",
            &HashMap::new(),
            Some(&Bytes::from_static(b"body")),
        );

        assert_eq!(modification.method.as_deref(), Some("POST"));
        assert_eq!(
            modification.headers.get("content-type").map(String::as_str),
            Some("application/json; charset=utf-8")
        );
        assert_eq!(modification.path.as_deref(), Some("/v2/users"));
        assert_eq!(modification.query_params.as_deref(), Some("debug=true"));
        assert_eq!(modification.write_files.len(), 1);
        assert!(modification.write_files[0].raw);
    }

    #[test]
    fn test_file_rule_maps_directory_with_remaining_path() {
        let dir = tempfile::tempdir().unwrap();
        let asset_dir = dir.path().join("assets");
        fs::create_dir_all(asset_dir.join("js")).unwrap();
        fs::write(asset_dir.join("js/app.js"), "console.log('dir');").unwrap();

        let rule = matched_rule_with_actions_filters_and_remainder(
            vec![RuleAction::File { path: asset_dir }],
            None,
            "/js/app.js?cache=1",
        );

        let modification = apply_request_rules(
            &[rule],
            "https://example.com/static/js/app.js?cache=1",
            "GET",
            &HashMap::new(),
            None,
        );

        let response = modification.short_circuit.unwrap();
        assert_eq!(response.body, Bytes::from_static(b"console.log('dir');"));
        assert_eq!(
            response.headers.get("content-type").map(String::as_str),
            Some("text/javascript")
        );
    }

    #[test]
    fn test_remote_resource_url_collection_joins_directory_remainders() {
        let request_rules = vec![matched_rule_with_actions_filters_and_remainder(
            vec![
                RuleAction::File {
                    path: "https://assets.example/static/".into(),
                },
                RuleAction::RequestBody {
                    content: BodyContent::File {
                        path: "https://api.example/payload.json".into(),
                    },
                },
            ],
            None,
            "/js/app.js?cache=1",
        )];

        assert_eq!(
            remote_resource_urls_for_request(&request_rules),
            vec![
                "https://api.example/payload.json".to_string(),
                "https://assets.example/static/js/app.js".to_string(),
            ]
        );

        let response_rules = vec![matched_rule_with_actions_filters_and_remainder(
            vec![
                RuleAction::ResponseBody {
                    content: BodyContent::File {
                        path: "https://assets.example/body/".into(),
                    },
                },
                RuleAction::Mock {
                    path: "https://api.example/mock.json".into(),
                },
            ],
            None,
            "/users?id=1",
        )];

        assert_eq!(
            remote_resource_urls_for_response(&response_rules),
            vec![
                "https://api.example/mock.json".to_string(),
                "https://assets.example/body/users".to_string(),
            ]
        );
    }

    #[test]
    fn test_response_remote_resource_collection_honors_response_filters() {
        let rules = vec![matched_rule_with_filters(
            RuleAction::ResponseBody {
                content: BodyContent::File {
                    path: "https://assets.example/not-found.json".into(),
                },
            },
            Some(RuleFilters {
                status_codes: vec![404],
                content_types: vec!["json".into()],
                ..Default::default()
            }),
        )];
        let request_headers = HashMap::new();
        let mut response_headers = HashMap::new();
        response_headers.insert("content-type".to_string(), "application/json".to_string());

        assert_eq!(
            remote_resource_urls_for_response_context(
                &rules,
                "https://example.com/api",
                "GET",
                &request_headers,
                &response_headers,
                200,
                Some("application/json"),
            ),
            Vec::<String>::new()
        );

        assert_eq!(
            remote_resource_urls_for_response_context(
                &rules,
                "https://example.com/api",
                "GET",
                &request_headers,
                &response_headers,
                404,
                Some("application/json"),
            ),
            vec!["https://assets.example/not-found.json".to_string()]
        );
    }

    #[test]
    fn test_request_file_rule_uses_prefetched_remote_resource() {
        let mut remote_resources = ResolvedResources::new();
        remote_resources.insert(
            "https://assets.example/static/app.js".to_string(),
            ResolvedResource {
                body: Bytes::from_static(b"console.log('remote');"),
                content_type: Some("application/javascript".to_string()),
            },
        );
        let resolve_ctx = ResolveCtx {
            store: None,
            ctx: None,
            remote_resources: Some(&remote_resources),
        };
        let rules = vec![matched_rule(RuleAction::File {
            path: "https://assets.example/static/app.js".into(),
        })];

        let modification = apply_request_rules_with_values(
            &rules,
            "https://example.com/app.js",
            "GET",
            &HashMap::new(),
            None,
            &resolve_ctx,
        );

        let response = modification.short_circuit.unwrap();
        assert_eq!(response.body, Bytes::from_static(b"console.log('remote');"));
        assert_eq!(
            response.headers.get("content-type").map(String::as_str),
            Some("application/javascript")
        );
    }

    #[test]
    fn test_html_js_css_body_are_response_only() {
        let request_headers = HashMap::new();

        for action in [
            RuleAction::HtmlBody {
                content: "<h1>ok</h1>".into(),
            },
            RuleAction::JsBody {
                content: "console.log('ok')".into(),
            },
            RuleAction::CssBody {
                content: "body{color:red}".into(),
            },
        ] {
            let modification = apply_request_rules(
                &[matched_rule(action.clone())],
                "https://example.com/app",
                "GET",
                &request_headers,
                None,
            );

            assert!(
                modification.short_circuit.is_none(),
                "{:?} must not short-circuit during request-stage rules",
                action
            );
            assert_eq!(modification.body, None);
        }
    }

    #[test]
    fn test_persist_request_writes_raw_message_and_body_only_policy() {
        let dir = tempfile::tempdir().unwrap();
        let raw_path = dir.path().join("req.raw");
        let body_path = dir.path().join("req.body");
        let get_body_path = dir.path().join("get.body");
        let mut headers = HashMap::new();
        headers.insert("host".to_string(), "example.com".to_string());
        headers.insert("content-encoding".to_string(), "gzip".to_string());

        persist_request_writes(
            &[
                body_write_target(body_path.to_string_lossy(), false),
                body_write_target(raw_path.to_string_lossy(), true),
            ],
            RequestWriteContext {
                method: "POST",
                url: "https://example.com/api/users?debug=1",
                headers: &headers,
                body: &Bytes::from_static(b"payload"),
                force: false,
            },
        );

        assert_eq!(fs::read(&body_path).unwrap(), b"payload");
        let raw = String::from_utf8(fs::read(&raw_path).unwrap()).unwrap();
        assert!(raw.starts_with("POST /api/users?debug=1 HTTP/1.1\r\n"));
        assert!(raw.contains("\r\nhost: example.com\r\n"));
        assert!(!raw.contains("content-encoding: gzip"));
        assert!(raw.ends_with("\r\n\r\npayload"));

        persist_request_writes(
            &[body_write_target(get_body_path.to_string_lossy(), false)],
            RequestWriteContext {
                method: "GET",
                url: "https://example.com/api",
                headers: &headers,
                body: &Bytes::from_static(b"ignored"),
                force: true,
            },
        );
        assert!(!get_body_path.exists());
    }

    #[test]
    fn test_persist_response_writes_raw_message_status_suffix_and_head_policy() {
        let dir = tempfile::tempdir().unwrap();
        let body_path = dir.path().join("res.body");
        let raw_path = dir.path().join("res.raw");
        let not_found = dir.path().join("missing.html.404");
        let head_body_path = dir.path().join("head.body");
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/plain".to_string());

        persist_response_writes(
            &[
                body_write_target(body_path.to_string_lossy(), false),
                body_write_target(raw_path.to_string_lossy(), true),
            ],
            ResponseWriteContext {
                method: "GET",
                status: 200,
                headers: &headers,
                body: &Bytes::from_static(b"ok"),
                force: false,
            },
        );
        assert_eq!(fs::read(&body_path).unwrap(), b"ok");
        let raw = String::from_utf8(fs::read(&raw_path).unwrap()).unwrap();
        assert!(raw.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(raw.contains("\r\ncontent-type: text/plain\r\n"));
        assert!(raw.ends_with("\r\n\r\nok"));

        persist_response_writes(
            &[body_write_target(
                not_found.with_extension("").to_string_lossy(),
                false,
            )],
            ResponseWriteContext {
                method: "GET",
                status: 404,
                headers: &headers,
                body: &Bytes::from_static(b"missing"),
                force: true,
            },
        );
        assert_eq!(fs::read(&not_found).unwrap(), b"missing");

        persist_response_writes(
            &[body_write_target(head_body_path.to_string_lossy(), false)],
            ResponseWriteContext {
                method: "HEAD",
                status: 200,
                headers: &headers,
                body: &Bytes::from_static(b"ignored"),
                force: true,
            },
        );
        assert!(!head_body_path.exists());
    }

    #[test]
    fn test_persist_writes_directory_index_remainder_and_force_policy() {
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("out");
        let out_dir_slash = format!("{}/", out_dir.display());
        let existing = out_dir.join("asset.js");

        persist_request_writes(
            &[body_write_target_with_remainder(
                &out_dir_slash,
                false,
                "/asset.js?cache=1",
            )],
            RequestWriteContext {
                method: "POST",
                url: "https://example.com/asset.js?cache=1",
                headers: &HashMap::new(),
                body: &Bytes::from_static(b"first"),
                force: false,
            },
        );
        assert_eq!(
            fs::read(out_dir.join("asset.js?cache=1")).unwrap(),
            b"first"
        );

        fs::create_dir_all(&out_dir).unwrap();
        fs::write(&existing, b"old").unwrap();
        persist_request_writes(
            &[body_write_target_with_remainder(
                &out_dir_slash,
                false,
                "/asset.js",
            )],
            RequestWriteContext {
                method: "POST",
                url: "https://example.com/asset.js",
                headers: &HashMap::new(),
                body: &Bytes::from_static(b"new"),
                force: false,
            },
        );
        assert_eq!(fs::read(&existing).unwrap(), b"old");

        persist_request_writes(
            &[body_write_target_with_remainder(
                &out_dir_slash,
                false,
                "/asset.js",
            )],
            RequestWriteContext {
                method: "POST",
                url: "https://example.com/asset.js",
                headers: &HashMap::new(),
                body: &Bytes::from_static(b"new"),
                force: true,
            },
        );
        assert_eq!(fs::read(&existing).unwrap(), b"new");

        persist_request_writes(
            &[body_write_target_with_remainder(&out_dir_slash, false, "/")],
            RequestWriteContext {
                method: "POST",
                url: "https://example.com/",
                headers: &HashMap::new(),
                body: &Bytes::from_static(b"index"),
                force: true,
            },
        );
        assert_eq!(fs::read(out_dir.join("index.html")).unwrap(), b"index");

        persist_response_writes(
            &[body_write_target_with_remainder(&out_dir_slash, false, "/")],
            ResponseWriteContext {
                method: "GET",
                status: 404,
                headers: &HashMap::new(),
                body: &Bytes::from_static(b"not found"),
                force: true,
            },
        );
        assert_eq!(fs::read(out_dir.join(".404")).unwrap(), b"not found");
    }

    #[test]
    fn test_apply_response_rules_honors_status_filter_and_write_actions() {
        let rules = vec![matched_rule_with_filters(
            RuleAction::ResponseBody {
                content: BodyContent::Text {
                    content: "not found".into(),
                    content_type: "text/plain".into(),
                },
            },
            Some(RuleFilters {
                status_codes: vec![404],
                ..Default::default()
            }),
        )];

        let request_headers = HashMap::new();
        let response_headers = HashMap::new();
        let body = Bytes::from_static(b"ok");

        let no_match = apply_response_rules(
            &rules,
            "https://example.com/api",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/plain"),
        );
        assert_eq!(no_match.body, Some(body.clone()));

        let matched = apply_response_rules(
            &rules,
            "https://example.com/api",
            "GET",
            &request_headers,
            &response_headers,
            404,
            Some(&body),
            Some("text/plain"),
        );
        assert_eq!(matched.body, Some(Bytes::from_static(b"not found")));

        let write_rules = vec![
            matched_rule(RuleAction::ResponseWrite {
                path: "/tmp/postgate-res.bin".into(),
                raw: false,
            }),
            matched_rule(RuleAction::ResponseFor {
                value: "client".into(),
            }),
        ];
        let modification = apply_response_rules(
            &write_rules,
            "https://example.com/api",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/plain"),
        );
        assert_eq!(modification.write_files.len(), 1);
        assert_eq!(
            modification
                .headers
                .get("x-whistle-response-for")
                .map(String::as_str),
            Some("client")
        );
    }

    #[test]
    fn test_response_body_and_mock_use_prefetched_remote_resources() {
        let mut remote_resources = ResolvedResources::new();
        remote_resources.insert(
            "https://assets.example/body.json".to_string(),
            ResolvedResource {
                body: Bytes::from_static(br#"{"ok":true}"#),
                content_type: Some("application/json".to_string()),
            },
        );
        remote_resources.insert(
            "https://assets.example/mock/users".to_string(),
            ResolvedResource {
                body: Bytes::from_static(b"mocked users"),
                content_type: Some("text/plain".to_string()),
            },
        );
        let resolve_ctx = ResolveCtx {
            store: None,
            ctx: None,
            remote_resources: Some(&remote_resources),
        };
        let request_headers = HashMap::new();
        let response_headers = HashMap::new();

        let res_body = apply_response_rules_with_values(
            &[matched_rule(RuleAction::ResponseBody {
                content: BodyContent::File {
                    path: "https://assets.example/body.json".into(),
                },
            })],
            "https://example.com/api",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&Bytes::from_static(b"original")),
            Some("application/json"),
            &resolve_ctx,
        );
        assert_eq!(res_body.body, Some(Bytes::from_static(br#"{"ok":true}"#)));

        let mock = apply_response_rules_with_values(
            &[matched_rule_with_actions_filters_and_remainder(
                vec![RuleAction::Mock {
                    path: "https://assets.example/mock/".into(),
                }],
                None,
                "/users?id=1",
            )],
            "https://example.com/users?id=1",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&Bytes::from_static(b"original")),
            Some("text/plain"),
            &resolve_ctx,
        );
        assert_eq!(mock.status_code, Some(200));
        assert_eq!(mock.body, Some(Bytes::from_static(b"mocked users")));
        assert_eq!(
            mock.headers.get("content-type").map(String::as_str),
            Some("text/plain")
        );
    }

    #[test]
    fn test_apply_response_body_rules_follow_whistle_content_type_and_body_policy() {
        let request_headers = HashMap::new();
        let response_headers = HashMap::new();
        let body = Bytes::from_static(b"original");

        let html_rule = vec![matched_rule(RuleAction::HtmlBody {
            content: "<h1>changed</h1>".into(),
        })];
        let html = apply_response_rules(
            &html_rule,
            "https://example.com/page",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("TEXT/HTML; charset=gbk"),
        );
        assert_eq!(html.body, Some(Bytes::from_static(b"<h1>changed</h1>")));
        assert_eq!(
            html.headers.get("content-type").map(String::as_str),
            Some("text/html; charset=utf-8")
        );

        let json = apply_response_rules(
            &html_rule,
            "https://example.com/page",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("application/json"),
        );
        assert_eq!(json.body, Some(body.clone()));

        let no_body_status = apply_response_rules(
            &html_rule,
            "https://example.com/page",
            "GET",
            &request_headers,
            &response_headers,
            204,
            Some(&body),
            Some("text/html"),
        );
        assert_eq!(no_body_status.body, Some(body.clone()));

        let head = apply_response_rules(
            &html_rule,
            "https://example.com/page",
            "HEAD",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/html"),
        );
        assert_eq!(head.body, Some(body.clone()));

        let js_in_html = apply_response_rules(
            &[matched_rule(RuleAction::JsBody {
                content: "window.x=1;".into(),
            })],
            "https://example.com/page",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/html"),
        );
        assert_eq!(
            js_in_html.body,
            Some(Bytes::from_static(b"<script>window.x=1;</script>"))
        );
        assert_eq!(
            js_in_html.headers.get("content-type").map(String::as_str),
            Some("text/html; charset=utf-8")
        );

        let js_file = apply_response_rules(
            &[matched_rule(RuleAction::JsBody {
                content: "window.x=1;".into(),
            })],
            "https://example.com/app.js",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("application/ecmascript"),
        );
        assert_eq!(js_file.body, Some(Bytes::from_static(b"window.x=1;")));
        assert_eq!(
            js_file.headers.get("content-type").map(String::as_str),
            Some("application/javascript; charset=utf-8")
        );

        let css_in_html = apply_response_rules(
            &[matched_rule(RuleAction::CssBody {
                content: "body{color:red}".into(),
            })],
            "https://example.com/page",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/html"),
        );
        assert_eq!(
            css_in_html.body,
            Some(Bytes::from_static(b"<style>body{color:red}</style>"))
        );
        assert_eq!(
            css_in_html.headers.get("content-type").map(String::as_str),
            Some("text/html; charset=utf-8")
        );

        let css_file = apply_response_rules(
            &[matched_rule(RuleAction::CssBody {
                content: "body{color:red}".into(),
            })],
            "https://example.com/app.css",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("Text/CSS"),
        );
        assert_eq!(css_file.body, Some(Bytes::from_static(b"body{color:red}")));
        assert_eq!(
            css_file.headers.get("content-type").map(String::as_str),
            Some("text/css; charset=utf-8")
        );

        let js_after_res_type = apply_response_rules(
            &[matched_rule_with_actions(vec![
                RuleAction::ResponseType {
                    content_type: "application/javascript".into(),
                },
                RuleAction::JsBody {
                    content: "window.y=2;".into(),
                },
            ])],
            "https://example.com/app",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/html"),
        );
        assert_eq!(
            js_after_res_type.body,
            Some(Bytes::from_static(b"window.y=2;"))
        );
        assert_eq!(
            js_after_res_type
                .headers
                .get("content-type")
                .map(String::as_str),
            Some("application/javascript; charset=utf-8")
        );

        let css_after_res_type = apply_response_rules(
            &[matched_rule_with_actions(vec![
                RuleAction::ResponseType {
                    content_type: "text/css".into(),
                },
                RuleAction::CssBody {
                    content: "body{color:blue}".into(),
                },
            ])],
            "https://example.com/app",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/html"),
        );
        assert_eq!(
            css_after_res_type.body,
            Some(Bytes::from_static(b"body{color:blue}"))
        );
        assert_eq!(
            css_after_res_type
                .headers
                .get("content-type")
                .map(String::as_str),
            Some("text/css; charset=utf-8")
        );
    }

    #[test]
    fn test_response_body_modifiers_skip_statuses_without_body() {
        let request_headers = HashMap::new();
        let response_headers = HashMap::new();
        let body = Bytes::from_static(b"original");

        for action in [
            RuleAction::ResponseBody {
                content: BodyContent::Text {
                    content: "changed".into(),
                    content_type: "text/plain".into(),
                },
            },
            RuleAction::ResponseAppend {
                content: "changed".into(),
            },
            RuleAction::ResponseReplace {
                pattern: "original".into(),
                replacement: "changed".into(),
                regex: false,
            },
            RuleAction::JsonBody {
                value: serde_json::json!({"ok": true}),
            },
        ] {
            let modification = apply_response_rules(
                &[matched_rule(action.clone())],
                "https://example.com/api",
                "GET",
                &request_headers,
                &response_headers,
                304,
                Some(&body),
                Some("text/plain"),
            );

            assert_eq!(
                modification.body,
                Some(body.clone()),
                "{:?} must not mutate body for 304 responses",
                action
            );
        }
    }

    #[test]
    fn test_debug_rule_removes_csp_from_html_responses() {
        let request_headers = HashMap::new();
        let response_headers = HashMap::from([
            (
                "content-type".to_string(),
                "text/html; charset=utf-8".to_string(),
            ),
            (
                "content-security-policy".to_string(),
                "default-src 'self'".to_string(),
            ),
            (
                "content-security-policy-report-only".to_string(),
                "default-src 'none'".to_string(),
            ),
        ]);
        let body = Bytes::from_static(b"<html><head></head><body></body></html>");

        let modification = apply_response_rules(
            &[matched_rule(RuleAction::Debug {
                name: "page".to_string(),
            })],
            "https://example.com/",
            "GET",
            &request_headers,
            &response_headers,
            200,
            Some(&body),
            Some("text/html; charset=utf-8"),
        );

        assert!(modification.inject_debug);
        for header in DEBUG_BLOCKING_RESPONSE_HEADERS {
            assert!(!modification.headers.contains_key(header));
            assert!(modification
                .headers_to_remove
                .iter()
                .any(|removed| removed == header));
        }
    }

    #[test]
    fn test_rules_require_response_body_covers_all_body_actions() {
        // Guard against future regressions: if we add a body-modifying action
        // and forget to list it here, streaming-path users silently lose it.
        // This test exercises the predicate for a representative sample of
        // the newly-added actions.
        let mk_rule = |action: RuleAction| MatchedRule {
            rule: Arc::new(Rule {
                id: "t".into(),
                pattern: Pattern::Domain("example.com".into()),
                filters: None,
                actions: vec![action],
                enabled: true,
                priority: 0,
                raw_line: String::new(),
                negated: false,
            }),
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
