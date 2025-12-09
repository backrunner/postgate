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
    /// Exclude patterns (for excludeFilter)
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Include only patterns (for includeFilter)
    #[serde(default)]
    pub include: Vec<String>,
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
        if !self.content_types.is_empty() {
            if let Some(ct) = headers.get("content-type") {
                if !self.content_types.iter().any(|t| ct.contains(t)) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check exclude patterns
        for pattern in &self.exclude {
            if url.contains(pattern) {
                return false;
            }
        }

        // Check include patterns
        if !self.include.is_empty() {
            if !self.include.iter().any(|p| url.contains(p)) {
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

impl Pattern {
    /// Check if this pattern matches a URL
    pub fn matches(&self, url: &str) -> bool {
        match self {
            Pattern::Exact(s) => url == s,
            Pattern::Wildcard(pattern) => wildcard_match(pattern, url),
            Pattern::Regex(re) => re.is_match(url),
            Pattern::PathPrefix(prefix) => url.contains(prefix) || url.starts_with(prefix),
            Pattern::All => true,
            Pattern::Domain(domain) => {
                url.contains(domain) || url.contains(&format!(".{}", domain))
            }
            Pattern::Url { protocol, host, path } => {
                let mut matches = url.contains(host);
                if let Some(proto) = protocol {
                    matches = matches && url.starts_with(&format!("{}://", proto));
                }
                if let Some(p) = path {
                    matches = matches && url.contains(p);
                }
                matches
            }
        }
    }

    /// Check if pattern matches host
    pub fn matches_host(&self, host: &str) -> bool {
        match self {
            Pattern::Exact(s) => host == s || s.contains(host),
            Pattern::Wildcard(pattern) => wildcard_match(pattern, host),
            Pattern::Regex(re) => re.is_match(host),
            Pattern::PathPrefix(prefix) => host.starts_with(prefix),
            Pattern::All => true,
            Pattern::Domain(domain) => {
                host == domain || host.ends_with(&format!(".{}", domain))
            }
            Pattern::Url { host: pattern_host, .. } => {
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
