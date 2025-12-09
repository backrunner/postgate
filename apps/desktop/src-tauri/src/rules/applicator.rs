//! Rule applicator - applies matched rules to requests and responses
//!
//! This module handles the actual application of rule actions to HTTP
//! requests and responses, supporting the full whistle-compatible action set.

use super::types::{
    BodyContent, CookieOptions, HeaderModifications, Rule, RuleAction, UrlParamModifications,
};
use bytes::Bytes;
use std::collections::HashMap;
use url::Url;

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
    /// Target host override
    pub target_host: Option<String>,
    /// Target proxy (for proxy chaining)
    pub upstream_proxy: Option<UpstreamProxy>,
    /// Whether to ignore/skip this request
    pub ignore: bool,
    /// Debug name for logging
    pub debug_name: Option<String>,
}

/// Upstream proxy configuration
#[derive(Debug, Clone)]
pub struct UpstreamProxy {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub auth: Option<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum ProxyType {
    Http,
    Https,
    Socks4,
    Socks5,
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
}

/// Apply rules to a request and return modifications
pub fn apply_request_rules(
    rules: &[Rule],
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&Bytes>,
) -> RequestModification {
    let mut modification = RequestModification {
        headers: headers.clone(),
        body: body.cloned(),
        ..Default::default()
    };

    // Parse URL for query parameter modifications
    let parsed_url = Url::parse(url).ok();

    for rule in rules {
        if !rule.enabled {
            continue;
        }

        // Check filters if present
        if let Some(filters) = &rule.filters {
            let protocol = parsed_url
                .as_ref()
                .map(|u| u.scheme())
                .unwrap_or("http");
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
                }

                RuleAction::RequestHeaders { modifications } => {
                    apply_header_modifications(&mut modification.headers, modifications);
                    modification.headers_to_remove.extend(modifications.remove.clone());
                }

                RuleAction::RequestBody { content } => {
                    modification.body = Some(resolve_body_content(content));
                }

                RuleAction::UrlParams { modifications } => {
                    if let Some(ref url) = parsed_url {
                        modification.query_params = Some(apply_url_param_modifications(url, modifications));
                    }
                }

                RuleAction::PathReplace { pattern, replacement } => {
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
                    modification.headers.insert(":method".to_string(), new_method.clone());
                }

                RuleAction::UserAgent { value } => {
                    modification.headers.insert("user-agent".to_string(), value.clone());
                }

                RuleAction::Referer { value } => {
                    modification.headers.insert("referer".to_string(), value.clone());
                }

                RuleAction::Auth { username, password } => {
                    let credentials = format!("{}:{}", username, password);
                    let encoded = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        credentials.as_bytes(),
                    );
                    modification.headers.insert("authorization".to_string(), format!("Basic {}", encoded));
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
                        modification.headers.insert("cookie".to_string(), cookie_str);
                    }
                }

                RuleAction::ForwardedFor { value } => {
                    modification.headers.insert("x-forwarded-for".to_string(), value.clone());
                }

                RuleAction::RequestReplace { pattern, replacement, regex } => {
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

                RuleAction::RequestCors { origin, credentials } => {
                    if let Some(o) = origin {
                        modification.headers.insert("origin".to_string(), o.clone());
                    }
                    if *credentials {
                        modification.headers.insert("access-control-request-credentials".to_string(), "true".to_string());
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
                    if let Ok(content) = std::fs::read(path) {
                        let content_type = mime_guess::from_path(path)
                            .first_or_octet_stream()
                            .to_string();
                        let mut headers = HashMap::new();
                        headers.insert("content-type".to_string(), content_type);
                        modification.short_circuit = Some(ShortCircuitResponse {
                            status: 200,
                            headers,
                            body: Bytes::from(content),
                        });
                    }
                }

                RuleAction::HtmlBody { content } => {
                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), "text/html; charset=utf-8".to_string());
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body: Bytes::from(content.clone()),
                    });
                }

                RuleAction::JsBody { content } => {
                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), "application/javascript; charset=utf-8".to_string());
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body: Bytes::from(content.clone()),
                    });
                }

                RuleAction::CssBody { content } => {
                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), "text/css; charset=utf-8".to_string());
                    modification.short_circuit = Some(ShortCircuitResponse {
                        status: 200,
                        headers,
                        body: Bytes::from(content.clone()),
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

                RuleAction::HttpProxy { host, port, auth } => {
                    modification.upstream_proxy = Some(UpstreamProxy {
                        proxy_type: ProxyType::Http,
                        host: host.clone(),
                        port: *port,
                        auth: auth.as_ref().map(|a| (a.username.clone(), a.password.clone())),
                    });
                }

                RuleAction::HttpsProxy { host, port, auth } => {
                    modification.upstream_proxy = Some(UpstreamProxy {
                        proxy_type: ProxyType::Https,
                        host: host.clone(),
                        port: *port,
                        auth: auth.as_ref().map(|a| (a.username.clone(), a.password.clone())),
                    });
                }

                RuleAction::SocksProxy { host, port, version, auth } => {
                    modification.upstream_proxy = Some(UpstreamProxy {
                        proxy_type: if *version == 4 { ProxyType::Socks4 } else { ProxyType::Socks5 },
                        host: host.clone(),
                        port: *port,
                        auth: auth.as_ref().map(|a| (a.username.clone(), a.password.clone())),
                    });
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
    rules: &[Rule],
    url: &str,
    method: &str,
    request_headers: &HashMap<String, String>,
    response_headers: &HashMap<String, String>,
    body: Option<&Bytes>,
    content_type: Option<&str>,
) -> ResponseModification {
    let mut modification = ResponseModification {
        headers: response_headers.clone(),
        body: body.cloned(),
        ..Default::default()
    };

    let parsed_url = Url::parse(url).ok();

    for rule in rules {
        if !rule.enabled {
            continue;
        }

        // Check filters if present
        if let Some(filters) = &rule.filters {
            let protocol = parsed_url
                .as_ref()
                .map(|u| u.scheme())
                .unwrap_or("http");
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
                    apply_header_modifications(&mut modification.headers, modifications);
                    modification.headers_to_remove.extend(modifications.remove.clone());
                }

                RuleAction::ResponseBody { content } => {
                    modification.body = Some(resolve_body_content(content));
                }

                RuleAction::ResponseReplace { pattern, replacement, regex } => {
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
                    modification.headers.insert("content-type".to_string(), content_type.clone());
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
                    modification.headers.insert("content-disposition".to_string(), value);
                }

                RuleAction::ResponseCors { origin, methods, headers, credentials, max_age } => {
                    let origin_value = origin.clone().unwrap_or_else(|| "*".to_string());
                    modification.headers.insert("access-control-allow-origin".to_string(), origin_value);
                    
                    if let Some(m) = methods {
                        modification.headers.insert("access-control-allow-methods".to_string(), m.clone());
                    }
                    
                    if let Some(h) = headers {
                        modification.headers.insert("access-control-allow-headers".to_string(), h.clone());
                    }
                    
                    if *credentials {
                        modification.headers.insert("access-control-allow-credentials".to_string(), "true".to_string());
                    }
                    
                    if let Some(age) = max_age {
                        modification.headers.insert("access-control-max-age".to_string(), age.to_string());
                    }
                }

                RuleAction::StatusCode { code } => {
                    modification.status_code = Some(*code);
                }

                RuleAction::HtmlAppend { content } => {
                    if is_html(content_type) {
                        modification.body = Some(append_to_html(
                            modification.body.as_ref(),
                            content,
                            false,
                        ));
                    }
                }

                RuleAction::HtmlPrepend { content } => {
                    if is_html(content_type) {
                        modification.body = Some(append_to_html(
                            modification.body.as_ref(),
                            content,
                            true,
                        ));
                    }
                }

                RuleAction::JsAppend { content } => {
                    if is_html(content_type) {
                        let script = format!("<script>{}</script>", content);
                        modification.body = Some(append_to_html(
                            modification.body.as_ref(),
                            &script,
                            false,
                        ));
                    } else if is_js(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = body.to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(content.as_bytes());
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::JsPrepend { content } => {
                    if is_html(content_type) {
                        let script = format!("<script>{}</script>", content);
                        modification.body = Some(append_to_html(
                            modification.body.as_ref(),
                            &script,
                            true,
                        ));
                    } else if is_js(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = content.as_bytes().to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(body);
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::CssAppend { content } => {
                    if is_html(content_type) {
                        let style = format!("<style>{}</style>", content);
                        modification.body = Some(append_to_html(
                            modification.body.as_ref(),
                            &style,
                            false,
                        ));
                    } else if is_css(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = body.to_vec();
                            new_body.extend_from_slice(b"\n");
                            new_body.extend_from_slice(content.as_bytes());
                            modification.body = Some(Bytes::from(new_body));
                        }
                    }
                }

                RuleAction::CssPrepend { content } => {
                    if is_html(content_type) {
                        let style = format!("<style>{}</style>", content);
                        modification.body = Some(append_to_html(
                            modification.body.as_ref(),
                            &style,
                            true,
                        ));
                    } else if is_css(content_type) {
                        if let Some(ref body) = modification.body {
                            let mut new_body = content.as_bytes().to_vec();
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

/// Resolve body content to bytes
fn resolve_body_content(content: &BodyContent) -> Bytes {
    match content {
        BodyContent::Text { content, .. } => Bytes::from(content.clone()),
        BodyContent::Json { value } => {
            Bytes::from(serde_json::to_string(value).unwrap_or_default())
        }
        BodyContent::File { path } => {
            std::fs::read(path).map(Bytes::from).unwrap_or_default()
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
}
