//! Rule types for whistle-compatible proxy rules
//!
//! This module defines the core types for rule matching and actions,
//! with compatibility for whistle rule syntax.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A rule group containing multiple rules
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleGroup {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub rules: Vec<Rule>,
    pub raw_content: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A single proxy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub pattern: Pattern,
    pub filters: Option<RuleFilters>,
    pub actions: Vec<RuleAction>,
    pub enabled: bool,
    pub priority: i32,
    pub raw_line: String,
}

/// Filters for conditional rule matching (whistle compatible)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFilters {
    /// Match specific HTTP methods (GET, POST, etc.)
    #[serde(default)]
    pub methods: Vec<String>,
    /// Match specific protocols (http, https, ws, wss)
    #[serde(default)]
    pub protocols: Vec<String>,
    /// Match specific ports
    #[serde(default)]
    pub ports: Vec<u16>,
    /// Match requests with specific headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Match requests with specific content types
    #[serde(default)]
    pub content_types: Vec<String>,
    /// Exclude patterns (for excludeFilter) - supports regex
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Include only patterns (for includeFilter) - supports regex
    #[serde(default)]
    pub include: Vec<String>,
    /// Match specific client IPs (i:, ip:, clientIp:)
    #[serde(default)]
    pub client_ips: Vec<String>,
    /// Match specific hosts (h:, host:, hostname:)
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Match specific status codes (s:, statusCode:) - for response matching
    #[serde(default)]
    pub status_codes: Vec<u16>,
}

impl RuleFilters {
    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
            && self.protocols.is_empty()
            && self.ports.is_empty()
            && self.headers.is_empty()
            && self.content_types.is_empty()
            && self.exclude.is_empty()
            && self.include.is_empty()
            && self.client_ips.is_empty()
            && self.hosts.is_empty()
            && self.status_codes.is_empty()
    }

    /// Check if filters match a request
    pub fn matches(
        &self,
        method: &str,
        protocol: &str,
        port: u16,
        headers: &HashMap<String, String>,
        url: &str,
    ) -> bool {
        // Check method filter
        if !self.methods.is_empty() {
            let method_upper = method.to_uppercase();
            if !self.methods.iter().any(|m| m.to_uppercase() == method_upper) {
                return false;
            }
        }

        // Check protocol filter
        if !self.protocols.is_empty() {
            let proto_lower = protocol.to_lowercase();
            if !self.protocols.iter().any(|p| p.to_lowercase() == proto_lower) {
                return false;
            }
        }

        // Check port filter
        if !self.ports.is_empty() && !self.ports.contains(&port) {
            return false;
        }

        // Check header filters
        for (key, expected_value) in &self.headers {
            let key_lower = key.to_lowercase();
            match headers.get(&key_lower) {
                Some(value) if value.contains(expected_value) => {}
                _ => return false,
            }
        }

        // Check content type filter
        // Note: Don't reject if Content-Type header is missing (many GET requests have no body)
        if !self.content_types.is_empty() {
            if let Some(ct) = headers.get("content-type") {
                if !self.content_types.iter().any(|t| ct.to_lowercase().contains(&t.to_lowercase())) {
                    return false;
                }
            }
            // If no content-type header, allow the request to pass through
        }

        // Check host filter
        if !self.hosts.is_empty() {
            // Extract host from URL
            let host = url::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()));
            if let Some(h) = host {
                if !self.hosts.iter().any(|pattern| {
                    h == *pattern || h.ends_with(&format!(".{}", pattern)) || wildcard_match(pattern, &h)
                }) {
                    return false;
                }
            }
        }

        // Check exclude patterns (supports regex)
        for pattern in &self.exclude {
            if pattern_matches(pattern, url) {
                return false;
            }
        }

        // Check include patterns (supports regex)
        if !self.include.is_empty() {
            if !self.include.iter().any(|p| pattern_matches(p, url)) {
                return false;
            }
        }

        true
    }
}

/// Pattern for matching requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Pattern {
    /// Exact match
    Exact(String),
    /// Wildcard pattern (e.g., *.example.com)
    Wildcard(String),
    /// Regular expression
    #[serde(skip)]
    Regex(Regex),
    /// Path prefix match
    PathPrefix(String),
    /// Match all
    All,
    /// Domain match (matches host and all subdomains)
    Domain(String),
    /// URL pattern with protocol (e.g., https://example.com)
    Url { protocol: Option<String>, host: String, path: Option<String> },
}

/// Result of pattern matching, containing match info and remaining path
#[derive(Debug, Clone)]
pub struct PatternMatchResult {
    /// Whether the pattern matched
    pub matched: bool,
    /// The remaining path after the matched prefix (for path appending)
    pub remaining_path: String,
}

impl Pattern {
    /// Check if this pattern matches a URL
    #[allow(dead_code)]
    pub fn matches(&self, url: &str) -> bool {
        self.match_with_remainder(url).matched
    }

    /// Match pattern and return remaining path for whistle-compatible path forwarding
    /// 
    /// For example:
    /// - Pattern: `https://v.qq.com/biu/u/history/`
    /// - URL: `https://v.qq.com/biu/u/history/page/1?id=123`
    /// - Result: matched=true, remaining_path="/page/1?id=123"
    pub fn match_with_remainder(&self, url: &str) -> PatternMatchResult {
        match self {
            Pattern::Exact(s) => {
                if url == s {
                    PatternMatchResult { matched: true, remaining_path: String::new() }
                } else {
                    PatternMatchResult { matched: false, remaining_path: String::new() }
                }
            }
            Pattern::Wildcard(pattern) => {
                PatternMatchResult {
                    matched: wildcard_match(pattern, url),
                    remaining_path: String::new(),
                }
            }
            Pattern::Regex(re) => {
                PatternMatchResult {
                    matched: re.is_match(url),
                    remaining_path: String::new(),
                }
            }
            Pattern::PathPrefix(prefix) => {
                // PathPrefix uses contains-style matching (as per original implementation)
                // It works on both full URLs and bare paths
                let path = if let Ok(parsed) = url::Url::parse(url) {
                    parsed.path().to_string()
                } else {
                    // For bare paths, use directly
                    url.to_string()
                };
                
                // Check if path contains the prefix
                if path.contains(prefix) {
                    // Calculate remaining path after the prefix match
                    if let Some(idx) = path.find(prefix) {
                        let after_prefix = &path[idx + prefix.len()..];
                        let query = if let Ok(parsed) = url::Url::parse(url) {
                            parsed.query().map(|q| format!("?{}", q)).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        PatternMatchResult {
                            matched: true,
                            remaining_path: format!("{}{}", after_prefix, query),
                        }
                    } else {
                        PatternMatchResult { matched: true, remaining_path: String::new() }
                    }
                } else {
                    PatternMatchResult { matched: false, remaining_path: String::new() }
                }
            }
            Pattern::All => {
                // For "All" pattern, keep the full path
                if let Ok(parsed) = url::Url::parse(url) {
                    let path = parsed.path();
                    let query = parsed.query().map(|q| format!("?{}", q)).unwrap_or_default();
                    PatternMatchResult {
                        matched: true,
                        remaining_path: format!("{}{}", path, query),
                    }
                } else {
                    PatternMatchResult { matched: true, remaining_path: String::new() }
                }
            }
            Pattern::Domain(domain) => {
                // Domain match: host must match, keep full path
                if let Ok(parsed) = url::Url::parse(url) {
                    let host = parsed.host_str().unwrap_or("");
                    if host == domain || host.ends_with(&format!(".{}", domain)) {
                        let path = parsed.path();
                        let query = parsed.query().map(|q| format!("?{}", q)).unwrap_or_default();
                        PatternMatchResult {
                            matched: true,
                            remaining_path: format!("{}{}", path, query),
                        }
                    } else {
                        PatternMatchResult { matched: false, remaining_path: String::new() }
                    }
                } else {
                    PatternMatchResult {
                        matched: url.contains(domain),
                        remaining_path: String::new(),
                    }
                }
            }
            Pattern::Url { protocol, host, path: pattern_path } => {
                // URL pattern match with prefix behavior (whistle compatible)
                if let Ok(parsed) = url::Url::parse(url) {
                    // Check protocol
                    if let Some(proto) = protocol {
                        if parsed.scheme() != proto {
                            return PatternMatchResult { matched: false, remaining_path: String::new() };
                        }
                    }
                    
                    // Check host
                    let url_host = parsed.host_str().unwrap_or("");
                    if url_host != host && !wildcard_match(host, url_host) {
                        return PatternMatchResult { matched: false, remaining_path: String::new() };
                    }
                    
                    // Check path prefix and calculate remaining
                    let url_path = parsed.path();
                    let query = parsed.query().map(|q| format!("?{}", q)).unwrap_or_default();
                    
                    if let Some(p) = pattern_path {
                        // Pattern has a path - do prefix matching
                        let p_normalized = if p.ends_with('/') { p.clone() } else { format!("{}/", p) };
                        let url_path_normalized = if url_path.ends_with('/') || url_path == p { 
                            url_path.to_string() 
                        } else { 
                            format!("{}/", url_path) 
                        };
                        
                        // Check if URL path starts with pattern path
                        if url_path.starts_with(p) || url_path_normalized.starts_with(&p_normalized) || url_path == p {
                            let remaining = if url_path.len() > p.len() {
                                &url_path[p.len()..]
                            } else {
                                ""
                            };
                            // Clean up leading slash if present (will be added during join)
                            let remaining = remaining.trim_start_matches('/');
                            let remaining = if remaining.is_empty() && query.is_empty() {
                                String::new()
                            } else if remaining.is_empty() {
                                query
                            } else {
                                format!("/{}{}", remaining, query)
                            };
                            PatternMatchResult { matched: true, remaining_path: remaining }
                        } else {
                            PatternMatchResult { matched: false, remaining_path: String::new() }
                        }
                    } else {
                        // No path in pattern - match host only, keep full path
                        PatternMatchResult {
                            matched: true,
                            remaining_path: format!("{}{}", url_path, query),
                        }
                    }
                } else {
                    PatternMatchResult { matched: false, remaining_path: String::new() }
                }
            }
        }
    }

    /// Check if pattern matches host
    pub fn matches_host(&self, host: &str) -> bool {
        match self {
            Pattern::Exact(s) => host == s || s.contains(host),
            Pattern::Wildcard(pattern) => wildcard_match(pattern, host),
            Pattern::Regex(re) => re.is_match(host),
            Pattern::PathPrefix(_) => false, // PathPrefix doesn't match hosts
            Pattern::All => true,
            Pattern::Domain(domain) => {
                host == domain || host.ends_with(&format!(".{}", domain))
            }
            Pattern::Url { host: pattern_host, path, .. } => {
                // Only do host-only match if pattern has no path specified
                // If pattern has a path, we should not fallback to host-only matching
                if path.is_some() {
                    return false;
                }
                host == pattern_host || wildcard_match(pattern_host, host)
            }
        }
    }
}

/// Simple wildcard matching
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    fn match_helper(pattern: &[char], text: &[char]) -> bool {
        match (pattern.first(), text.first()) {
            (None, None) => true,
            (Some('*'), _) => {
                match_helper(&pattern[1..], text) || 
                (!text.is_empty() && match_helper(pattern, &text[1..]))
            }
            (Some('?'), Some(_)) => match_helper(&pattern[1..], &text[1..]),
            (Some(p), Some(t)) if *p == *t => match_helper(&pattern[1..], &text[1..]),
            (None, Some(_)) => false,
            (Some(_), None) => pattern.iter().all(|&c| c == '*'),
            _ => false,
        }
    }

    match_helper(&pattern_chars, &text_chars)
}

/// Match pattern against text, supporting both substring and regex
fn pattern_matches(pattern: &str, text: &str) -> bool {
    // Try as regex first if it looks like a regex pattern
    if pattern.starts_with('^') || pattern.starts_with('/') || pattern.contains(".*") || pattern.ends_with('$') {
        // Try to compile as regex
        if let Ok(re) = Regex::new(pattern.trim_matches('/')) {
            return re.is_match(text);
        }
    }
    // Fallback to substring match
    text.contains(pattern)
}

/// Action to perform when a rule matches (whistle compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RuleAction {
    // === HOST/REDIRECT ACTIONS ===
    
    /// Redirect to a different host (host://)
    Host { target: String },

    /// Serve a local file (file://)
    File { path: PathBuf },

    /// HTTP redirect (redirect://, 301://, 302://)
    Redirect { url: String, status: u16 },

    /// Return a specific status code (statusCode://)
    StatusCode { code: u16 },

    // === REQUEST MODIFICATIONS ===

    /// Modify request headers (reqHeaders://)
    RequestHeaders { modifications: HeaderModifications },

    /// Replace request body (reqBody://)
    RequestBody { content: BodyContent },

    /// Modify URL query parameters (urlParams://)
    UrlParams { modifications: UrlParamModifications },

    /// Replace URL path (pathReplace://)
    PathReplace { pattern: String, replacement: String },

    /// Change HTTP method (method://)
    Method { method: String },

    /// Set User-Agent header (ua://)
    UserAgent { value: String },

    /// Set Referer header (referer://)
    Referer { value: String },

    /// Set authentication (auth://)
    Auth { username: String, password: String },

    /// Set request cookies (reqCookies://)
    RequestCookies { cookies: HashMap<String, String> },

    /// Set X-Forwarded-For (forwardedFor://)
    ForwardedFor { value: String },

    // === RESPONSE MODIFICATIONS ===

    /// Modify response headers (resHeaders://)
    ResponseHeaders { modifications: HeaderModifications },

    /// Replace response body (resBody://)
    ResponseBody { content: BodyContent },

    /// Replace with HTML body (htmlBody://)
    HtmlBody { content: String },

    /// Replace with CSS body (cssBody://)
    CssBody { content: String },

    /// Replace with JS body (jsBody://)
    JsBody { content: String },

    /// String replacement in response (resReplace://)
    ResponseReplace { pattern: String, replacement: String, regex: bool },

    /// String replacement in request (reqReplace://)
    RequestReplace { pattern: String, replacement: String, regex: bool },

    /// Set response cookies (resCookies://)
    ResponseCookies { cookies: HashMap<String, CookieOptions> },

    /// Set response content type (resType://)
    ResponseType { content_type: String },

    /// Set response charset (resCharset://)
    ResponseCharset { charset: String },

    /// Set attachment header (attachment://)
    Attachment { filename: Option<String> },

    // === INJECTION ACTIONS ===

    /// Append to HTML (htmlAppend://)
    HtmlAppend { content: String },

    /// Prepend to HTML (htmlPrepend://)
    HtmlPrepend { content: String },

    /// Append JavaScript (jsAppend://)
    JsAppend { content: String },

    /// Prepend JavaScript (jsPrepend://)
    JsPrepend { content: String },

    /// Append CSS (cssAppend://)
    CssAppend { content: String },

    /// Prepend CSS (cssPrepend://)
    CssPrepend { content: String },

    // === CORS ACTIONS ===

    /// Set request CORS headers (reqCors://)
    RequestCors { origin: Option<String>, credentials: bool },

    /// Set response CORS headers (resCors://)
    ResponseCors { 
        origin: Option<String>, 
        methods: Option<String>,
        headers: Option<String>,
        credentials: bool,
        max_age: Option<u64>,
    },

    // === PERFORMANCE ACTIONS ===

    /// Add delay to request/response (reqDelay://, resDelay://)
    Delay {
        request_ms: Option<u64>,
        response_ms: Option<u64>,
    },

    /// Throttle speed (reqSpeed://, resSpeed://, speed://)
    Speed { 
        request_kbps: Option<u64>,
        response_kbps: Option<u64>,
    },

    // === DEBUGGING/CONTROL ===

    /// Enable debugging for this request (debug://)
    Debug { name: String },

    /// Forward to plugin (plugin://)
    Plugin { name: String, config: serde_json::Value },

    /// Log request (log://)
    Log { message: Option<String> },

    /// Ignore/skip rule processing (ignore://)
    Ignore,

    /// Enable specific features (enable://)
    Enable { features: Vec<String> },

    /// Disable specific features (disable://)
    Disable { features: Vec<String> },

    // === PROXY CONFIGURATION ===

    /// Use HTTP proxy (proxy://, http-proxy://)
    HttpProxy { host: String, port: u16, auth: Option<ProxyAuth> },

    /// Use HTTPS proxy (https-proxy://)
    HttpsProxy { host: String, port: u16, auth: Option<ProxyAuth> },

    /// Use SOCKS proxy (socks://)
    SocksProxy { host: String, port: u16, version: u8, auth: Option<ProxyAuth> },

    // === ADDITIONAL WHISTLE ACTIONS ===

    /// JSON body response (jsonBody://)
    JsonBody { value: serde_json::Value },

    /// Request timeout in milliseconds (timeout://)
    Timeout { ms: u64 },

    /// Delete specific headers (delete://)
    DeleteHeaders { headers: Vec<String> },

    /// Echo request back as response (echo://)
    Echo,

    /// Mock response from file (mock://)
    Mock { path: PathBuf },

    /// Replace content in HTML (htmlReplace://)
    HtmlReplace { pattern: String, replacement: String, regex: bool },

    /// Replace content in JavaScript (jsReplace://)
    JsReplace { pattern: String, replacement: String, regex: bool },

    /// Replace content in CSS (cssReplace://)
    CssReplace { pattern: String, replacement: String, regex: bool },

    /// Prepend to request body (reqPrepend://)
    RequestPrepend { content: String },

    /// Append to request body (reqAppend://)
    RequestAppend { content: String },

    /// Prepend to response body (resPrepend://)
    ResponsePrepend { content: String },

    /// Append to response body (resAppend://)
    ResponseAppend { content: String },
}

/// Proxy authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

/// URL parameter modifications
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UrlParamModifications {
    /// Set parameters (overwrite if exists)
    pub set: HashMap<String, String>,
    /// Remove parameters
    pub remove: Vec<String>,
    /// Append parameters (allow duplicates)
    pub append: HashMap<String, String>,
}

/// Header modifications
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeaderModifications {
    pub set: HashMap<String, String>,
    pub remove: Vec<String>,
    pub append: HashMap<String, String>,
}

/// Cookie options for response cookies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieOptions {
    pub value: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub max_age: Option<u64>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub same_site: Option<String>,
}

/// Body content for replacement
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BodyContent {
    /// Raw text/string content
    Text { content: String, content_type: String },

    /// JSON content
    Json { value: serde_json::Value },

    /// Local file content
    File { path: PathBuf },

    /// Base64 encoded binary
    Base64 { data: String },

    /// Empty body
    Empty,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_match() {
        assert!(wildcard_match("*.example.com", "api.example.com"));
        assert!(wildcard_match("*.example.com", "www.example.com"));
        assert!(!wildcard_match("*.example.com", "example.com"));
        assert!(wildcard_match("example.*", "example.com"));
        assert!(wildcard_match("example.*", "example.org"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("test?", "test1"));
        assert!(!wildcard_match("test?", "test12"));
    }

    #[test]
    fn test_pattern_matches() {
        let exact = Pattern::Exact("https://example.com/api".to_string());
        assert!(exact.matches("https://example.com/api"));
        assert!(!exact.matches("https://example.com/api/v2"));

        let prefix = Pattern::PathPrefix("/api".to_string());
        assert!(prefix.matches("/api/users"));
        assert!(prefix.matches("/api"));
        // PathPrefix uses contains-match, so /v1/api also matches
        assert!(prefix.matches("/v1/api"));

        let domain = Pattern::Domain("example.com".to_string());
        assert!(domain.matches_host("example.com"));
        assert!(domain.matches_host("api.example.com"));
        assert!(!domain.matches_host("example.org"));
    }

    #[test]
    fn test_rule_filters() {
        let filters = RuleFilters {
            methods: vec!["GET".to_string(), "POST".to_string()],
            protocols: vec!["https".to_string()],
            ..Default::default()
        };

        let headers = HashMap::new();
        assert!(filters.matches("GET", "https", 443, &headers, "https://example.com"));
        assert!(filters.matches("POST", "https", 443, &headers, "https://example.com"));
        assert!(!filters.matches("DELETE", "https", 443, &headers, "https://example.com"));
        assert!(!filters.matches("GET", "http", 80, &headers, "http://example.com"));
    }

    #[test]
    fn test_exclude_filter() {
        let filters = RuleFilters {
            exclude: vec!["/health".to_string(), "/metrics".to_string()],
            ..Default::default()
        };

        let headers = HashMap::new();
        assert!(filters.matches("GET", "https", 443, &headers, "https://example.com/api"));
        assert!(!filters.matches("GET", "https", 443, &headers, "https://example.com/health"));
        assert!(!filters.matches("GET", "https", 443, &headers, "https://example.com/metrics"));
    }
}
