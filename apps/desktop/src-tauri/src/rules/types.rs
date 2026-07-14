//! Rule types for whistle-compatible proxy rules
//!
//! This module defines the core types for rule matching and actions,
//! with full compatibility for whistle rule syntax.

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
    /// Inline `{name}` value definitions declared inside this group's raw
    /// content via fenced ```` ``` name\n…\n``` ```` blocks.
    /// Whistle-compatible: inline definitions take precedence over the
    /// global Values store during resolution.
    #[serde(default)]
    pub inline_values: HashMap<String, String>,
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
    /// If true, the match result is inverted (whistle `!` prefix)
    #[serde(default)]
    pub negated: bool,
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

/// Runtime context used to evaluate whistle filters. Some filters are only
/// known after the upstream response arrives (for example status code and
/// response headers), so request and response rule application use different
/// subsets of this context.
pub struct FilterContext<'a> {
    pub method: &'a str,
    pub protocol: &'a str,
    pub port: u16,
    pub request_headers: &'a HashMap<String, String>,
    pub response_headers: Option<&'a HashMap<String, String>>,
    pub url: &'a str,
    pub client_ip: Option<&'a str>,
    pub status_code: Option<u16>,
    pub body: Option<&'a str>,
    pub content_type: Option<&'a str>,
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
        self.matches_request(method, protocol, port, headers, url, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn matches_request(
        &self,
        method: &str,
        protocol: &str,
        port: u16,
        headers: &HashMap<String, String>,
        url: &str,
        client_ip: Option<&str>,
        body: Option<&str>,
    ) -> bool {
        self.matches_context(FilterContext {
            method,
            protocol,
            port,
            request_headers: headers,
            response_headers: None,
            url,
            client_ip,
            status_code: None,
            body,
            content_type: headers.get("content-type").map(String::as_str),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn matches_response(
        &self,
        method: &str,
        protocol: &str,
        port: u16,
        request_headers: &HashMap<String, String>,
        response_headers: &HashMap<String, String>,
        url: &str,
        status_code: u16,
        content_type: Option<&str>,
    ) -> bool {
        self.matches_context(FilterContext {
            method,
            protocol,
            port,
            request_headers,
            response_headers: Some(response_headers),
            url,
            client_ip: None,
            status_code: Some(status_code),
            body: None,
            content_type: content_type
                .or_else(|| response_headers.get("content-type").map(String::as_str)),
        })
    }

    pub fn matches_context(&self, ctx: FilterContext<'_>) -> bool {
        // Check method filter
        if !self.methods.is_empty() {
            let method_upper = ctx.method.to_uppercase();
            if !self
                .methods
                .iter()
                .any(|m| m.to_uppercase() == method_upper)
            {
                return false;
            }
        }

        // Check protocol filter
        if !self.protocols.is_empty() {
            let proto_lower = ctx.protocol.to_lowercase();
            if !self
                .protocols
                .iter()
                .any(|p| p.to_lowercase() == proto_lower)
            {
                return false;
            }
        }

        // Check port filter
        if !self.ports.is_empty() && !self.ports.contains(&ctx.port) {
            return false;
        }

        // Check header filters
        for (key, expected_value) in &self.headers {
            let key_lower = key.to_lowercase();
            let value = ctx.request_headers.get(&key_lower).or_else(|| {
                ctx.response_headers
                    .and_then(|headers| headers.get(&key_lower))
            });
            match value {
                Some(value) if header_value_matches(value, expected_value) => {}
                _ => return false,
            }
        }

        // Check content type filter. Once a content-type filter is present, a
        // missing Content-Type should not match; otherwise `ct:json` becomes a
        // broad rule for body-less GETs and unrelated responses.
        if !self.content_types.is_empty() {
            let Some(ct) = ctx.content_type else {
                return false;
            };
            if !self
                .content_types
                .iter()
                .any(|t| ct.to_lowercase().contains(&t.to_lowercase()))
            {
                return false;
            }
        }

        // Check client IP filter when the caller has IP context. Some internal
        // unit paths and response-only checks don't, and request matching has
        // already applied the IP filter before response rules run.
        if !self.client_ips.is_empty() {
            if let Some(client_ip) = ctx.client_ip {
                if !self.client_ips.iter().any(|pattern| {
                    client_ip == pattern
                        || wildcard_match(pattern, client_ip)
                        || pattern_matches(pattern, client_ip)
                }) {
                    return false;
                }
            }
        }

        // Status is only known during response rule application. During request
        // matching we deliberately defer it so `s:404 resBody://...` can still
        // reach the response phase and decide there.
        if !self.status_codes.is_empty() {
            if let Some(status_code) = ctx.status_code {
                if !self.status_codes.contains(&status_code) {
                    return false;
                }
            }
        }

        if let Some(body) = ctx.body {
            // Body filters are not yet parsed into their own field, but this
            // hook keeps context evaluation extensible without another call-site
            // rewrite.
            let _ = body;
        }

        // Check host filter
        if !self.hosts.is_empty() {
            let host = url::Url::parse(ctx.url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()));
            if let Some(h) = host {
                if !self.hosts.iter().any(|pattern| {
                    h == *pattern
                        || h.ends_with(&format!(".{}", pattern))
                        || wildcard_match(pattern, &h)
                }) {
                    return false;
                }
            }
        }

        // Check exclude patterns (supports regex)
        // Whistle: if ANY exclude matches → reject
        for pattern in &self.exclude {
            if pattern_matches(pattern, ctx.url) {
                return false;
            }
        }

        // Check include patterns (supports regex)
        // Whistle: if include is specified, at least ONE must match
        if !self.include.is_empty() && !self.include.iter().any(|p| pattern_matches(p, ctx.url)) {
            return false;
        }

        true
    }
}

fn header_value_matches(value: &str, expected: &str) -> bool {
    if expected.starts_with('/') || expected.starts_with('^') || expected.contains(".*") {
        pattern_matches(expected, value)
    } else {
        value.to_lowercase().contains(&expected.to_lowercase())
    }
}

/// Pattern for matching requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Pattern {
    /// Exact match (whistle `$pattern` prefix)
    Exact(String),
    /// Wildcard pattern (e.g., *.example.com)
    Wildcard(String),
    /// Regular expression
    #[serde(skip)]
    Regex(Regex),
    /// Path prefix match (starts with /)
    PathPrefix(String),
    /// Match all
    All,
    /// Domain match (matches host and all subdomains)
    Domain(String),
    /// URL pattern with protocol (e.g., https://example.com/path)
    Url {
        protocol: Option<String>,
        host: String,
        path: Option<String>,
    },
    /// Port pattern (whistle `:port` syntax, matches any URL with that port)
    Port(u16),
    /// No-schema pattern (whistle `//host/path`, matches any protocol)
    NoSchema { host: String, path: Option<String> },
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
    pub fn matches(&self, url: &str) -> bool {
        self.match_with_remainder(url, 0).matched
    }

    /// Match pattern and return remaining path for whistle-compatible path forwarding
    ///
    /// `port` is needed for Pattern::Port matching
    pub fn match_with_remainder(&self, url: &str, port: u16) -> PatternMatchResult {
        let no_match = PatternMatchResult {
            matched: false,
            remaining_path: String::new(),
        };

        match self {
            Pattern::Exact(s) => {
                // Whistle exact match: also match without query string
                if let Ok(parsed) = url::Url::parse(url) {
                    let url_no_query = format!(
                        "{}://{}{}",
                        parsed.scheme(),
                        &parsed[url::Position::BeforeHost..url::Position::AfterPort],
                        parsed.path()
                    );
                    if url == s || url_no_query == *s {
                        PatternMatchResult {
                            matched: true,
                            remaining_path: String::new(),
                        }
                    } else {
                        no_match
                    }
                } else if url == s {
                    PatternMatchResult {
                        matched: true,
                        remaining_path: String::new(),
                    }
                } else {
                    no_match
                }
            }
            Pattern::Wildcard(pattern) => PatternMatchResult {
                matched: whistle_wildcard_match(pattern, url),
                remaining_path: String::new(),
            },
            Pattern::Regex(re) => PatternMatchResult {
                matched: re.is_match(url),
                remaining_path: String::new(),
            },
            Pattern::PathPrefix(prefix) => {
                // Whistle: path prefix uses starts_with, not contains
                let path = if let Ok(parsed) = url::Url::parse(url) {
                    parsed.path().to_string()
                } else {
                    url.to_string()
                };

                // starts_with + boundary check
                if path.starts_with(prefix.as_str()) {
                    let rest = &path[prefix.len()..];
                    // Boundary check: next char must be separator or end of string
                    if rest.is_empty()
                        || is_path_separator(rest.as_bytes()[0])
                        || prefix.ends_with('/')
                    {
                        let query = if let Ok(parsed) = url::Url::parse(url) {
                            parsed
                                .query()
                                .map(|q| format!("?{}", q))
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        PatternMatchResult {
                            matched: true,
                            remaining_path: format!("{}{}", rest, query),
                        }
                    } else {
                        no_match
                    }
                } else {
                    no_match
                }
            }
            Pattern::All => {
                if let Ok(parsed) = url::Url::parse(url) {
                    let path = parsed.path();
                    let query = parsed
                        .query()
                        .map(|q| format!("?{}", q))
                        .unwrap_or_default();
                    PatternMatchResult {
                        matched: true,
                        remaining_path: format!("{}{}", path, query),
                    }
                } else {
                    PatternMatchResult {
                        matched: true,
                        remaining_path: String::new(),
                    }
                }
            }
            Pattern::Domain(domain) => {
                // Domain match: host must match (exact or subdomain), keep full path
                // Whistle: also tries matching with port stripped (isDomain)
                if let Ok(parsed) = url::Url::parse(url) {
                    let host = parsed.host_str().unwrap_or("");
                    if host == domain || host.ends_with(&format!(".{}", domain)) {
                        let path = parsed.path();
                        let query = parsed
                            .query()
                            .map(|q| format!("?{}", q))
                            .unwrap_or_default();
                        PatternMatchResult {
                            matched: true,
                            remaining_path: format!("{}{}", path, query),
                        }
                    } else {
                        no_match
                    }
                } else {
                    PatternMatchResult {
                        matched: url.contains(domain),
                        remaining_path: String::new(),
                    }
                }
            }
            Pattern::Url {
                protocol,
                host,
                path: pattern_path,
            } => self.match_url_pattern(url, protocol.as_deref(), host, pattern_path.as_deref()),
            Pattern::Port(expected_port) => {
                // Match any URL with the given port
                if port == *expected_port {
                    if let Ok(parsed) = url::Url::parse(url) {
                        let path = parsed.path();
                        let query = parsed
                            .query()
                            .map(|q| format!("?{}", q))
                            .unwrap_or_default();
                        PatternMatchResult {
                            matched: true,
                            remaining_path: format!("{}{}", path, query),
                        }
                    } else {
                        PatternMatchResult {
                            matched: true,
                            remaining_path: String::new(),
                        }
                    }
                } else {
                    no_match
                }
            }
            Pattern::NoSchema {
                host,
                path: pattern_path,
            } => {
                // Match any protocol — delegate to URL pattern matching with no protocol
                self.match_url_pattern(url, None, host, pattern_path.as_deref())
            }
        }
    }

    /// Internal: match a URL-style pattern (used by both Pattern::Url and Pattern::NoSchema)
    fn match_url_pattern(
        &self,
        url: &str,
        expected_protocol: Option<&str>,
        expected_host: &str,
        expected_path: Option<&str>,
    ) -> PatternMatchResult {
        let no_match = PatternMatchResult {
            matched: false,
            remaining_path: String::new(),
        };

        let parsed = match url::Url::parse(url) {
            Ok(p) => p,
            Err(_) => return no_match,
        };

        // Check protocol
        if let Some(proto) = expected_protocol {
            if parsed.scheme() != proto {
                return no_match;
            }
        }

        // Check host (exact or wildcard)
        let url_host = parsed.host_str().unwrap_or("");
        let url_authority = parsed[url::Position::BeforeHost..url::Position::AfterPort].to_string();
        if url_host != expected_host
            && url_authority != expected_host
            && !wildcard_match(expected_host, url_host)
            && !wildcard_match(expected_host, &url_authority)
        {
            return no_match;
        }

        let url_path = parsed.path();
        let query = parsed
            .query()
            .map(|q| format!("?{}", q))
            .unwrap_or_default();

        if let Some(p) = expected_path {
            // Pattern has a path — do prefix matching with boundary check (whistle compatible)
            let p_normalized = if p.ends_with('/') {
                p.to_string()
            } else {
                format!("{}/", p)
            };
            let url_path_normalized = if url_path.ends_with('/') || url_path == p {
                url_path.to_string()
            } else {
                format!("{}/", url_path)
            };

            if url_path.starts_with(p)
                || url_path_normalized.starts_with(&p_normalized)
                || url_path == p
            {
                // Whistle boundary check: the char after the match must be a separator
                let after_match_pos = p.len();
                if after_match_pos < url_path.len() {
                    let next_byte = url_path.as_bytes()[after_match_pos];
                    if !is_path_separator(next_byte) && !p.ends_with('/') {
                        return no_match;
                    }
                }

                let remaining = if url_path.len() > p.len() {
                    &url_path[p.len()..]
                } else {
                    ""
                };
                let remaining = remaining.trim_start_matches('/');
                let remaining = if remaining.is_empty() && query.is_empty() {
                    String::new()
                } else if remaining.is_empty() {
                    query
                } else {
                    format!("/{}{}", remaining, query)
                };
                PatternMatchResult {
                    matched: true,
                    remaining_path: remaining,
                }
            } else {
                no_match
            }
        } else {
            // No path in pattern — match host only, keep full path
            PatternMatchResult {
                matched: true,
                remaining_path: format!("{}{}", url_path, query),
            }
        }
    }

    /// Check if pattern matches host (fallback for host-only matching)
    pub fn matches_host(&self, host: &str) -> bool {
        match self {
            Pattern::Exact(s) => host == s || s.contains(host),
            Pattern::Wildcard(pattern) => wildcard_match(pattern, host),
            Pattern::Regex(re) => re.is_match(host),
            Pattern::PathPrefix(_) => false,
            Pattern::All => true,
            Pattern::Domain(domain) => host == domain || host.ends_with(&format!(".{}", domain)),
            Pattern::Url {
                host: pattern_host,
                path,
                ..
            } => {
                if path.is_some() {
                    return false;
                }
                host == pattern_host || wildcard_match(pattern_host, host)
            }
            Pattern::Port(_) => false,
            Pattern::NoSchema {
                host: pattern_host,
                path,
                ..
            } => {
                if path.is_some() {
                    return false;
                }
                host == pattern_host || wildcard_match(pattern_host, host)
            }
        }
    }
}

/// Check if a byte is a path separator (whistle: `/`, `?`, `\`)
#[inline]
fn is_path_separator(b: u8) -> bool {
    b == b'/' || b == b'?' || b == b'\\'
}

/// Simple wildcard matching (basic `*` and `?`)
/// Used for host matching and simple cases
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    fn match_helper(pattern: &[char], text: &[char]) -> bool {
        match (pattern.first(), text.first()) {
            (None, None) => true,
            (Some('*'), _) => {
                match_helper(&pattern[1..], text)
                    || (!text.is_empty() && match_helper(pattern, &text[1..]))
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

/// Whistle-compatible wildcard matching for URL patterns.
/// Compiles a whistle wildcard URL pattern to regex and matches.
///
/// Whistle wildcard semantics:
/// - Domain: `*` = single segment `[^/?.]*`, `**` = any non-/? chars `[^/?]*`, `***` with `.` = optional subdomain
/// - Path: `*` = single segment `[^?/]*`, `**` = any path `[^?]*`, `***` = anything `.*`
/// - Query: `*` = single param `[^&]*`, `**` = rest `.*`
fn whistle_wildcard_match(pattern: &str, text: &str) -> bool {
    // If pattern has no multi-star sequences, use simple wildcard match
    if !pattern.contains("**") {
        return wildcard_match(pattern, text);
    }

    // Compile to regex
    if let Some(re) = compile_whistle_wildcard(pattern) {
        re.is_match(text)
    } else {
        wildcard_match(pattern, text)
    }
}

/// Compile a whistle wildcard pattern to a Regex
fn compile_whistle_wildcard(pattern: &str) -> Option<Regex> {
    // Split into protocol, domain, path, query parts
    let (proto_part, rest) = if let Some(idx) = pattern.find("://") {
        (&pattern[..idx + 3], &pattern[idx + 3..])
    } else if let Some(stripped) = pattern.strip_prefix("//") {
        ("//", stripped)
    } else {
        ("", pattern)
    };

    // Split rest into domain+path and query
    let (domain_path, query) = if let Some(idx) = rest.find('?') {
        (&rest[..idx], Some(&rest[idx + 1..]))
    } else {
        (rest, None)
    };

    // Split domain and path
    let (domain, path) = if let Some(idx) = domain_path.find('/') {
        (&domain_path[..idx], Some(&domain_path[idx..]))
    } else {
        (domain_path, None)
    };

    let mut regex = String::from("^");

    // Protocol part
    if proto_part.is_empty() {
        // No protocol specified — this shouldn't typically happen for wildcard patterns
        // but handle gracefully
    } else if proto_part == "//" {
        regex.push_str("[a-z]+://");
    } else {
        regex.push_str(&regex::escape(proto_part));
    }

    // Domain part — whistle domain wildcard rules
    let domain_regex = domain_to_regexp(domain);
    regex.push_str(&domain_regex);

    // Path part
    if let Some(p) = path {
        let path_regex = path_to_regexp(p);
        regex.push_str(&path_regex);
    }

    // Query part
    if let Some(q) = query {
        regex.push('?');
        let query_regex = query_to_regexp(q);
        regex.push_str(&query_regex);
    }

    Regex::new(&regex).ok()
}

/// Convert whistle domain wildcards to regex
/// `*` → `[^/?.]*` (single segment)
/// `**` → `[^/?]*` (any non-slash chars)
/// `***.` → `(?:[^/?]*\\.)?` (optional subdomain)
fn domain_to_regexp(domain: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = domain.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '*' {
            let star_count = chars[i..].iter().take_while(|&&c| c == '*').count();
            i += star_count;
            if star_count >= 3 && i < len && chars[i] == '.' {
                // ***. → optional subdomain
                result.push_str("(?:[^/?]*\\.)?");
                i += 1; // skip the dot
            } else if star_count >= 2 {
                // ** → any non-slash chars
                result.push_str("[^/?]*");
            } else {
                // * → single segment (no dots)
                result.push_str("[^/?.]*");
            }
        } else {
            result.push_str(&regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }

    result
}

/// Convert whistle path wildcards to regex
/// `*` → `[^?/]*` (single segment)
/// `**` → `[^?]*` (any path)
/// `***` → `.*` (anything including query)
fn path_to_regexp(path: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = path.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '*' {
            let star_count = chars[i..].iter().take_while(|&&c| c == '*').count();
            i += star_count;
            if star_count >= 3 {
                result.push_str(".*");
            } else if star_count >= 2 {
                result.push_str("[^?]*");
            } else {
                result.push_str("[^?/]*");
            }
        } else {
            result.push_str(&regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }

    result
}

/// Convert whistle query wildcards to regex
/// `*` → `[^&]*` (single param)
/// `**` → `.*` (rest of query)
fn query_to_regexp(query: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = query.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '*' {
            let star_count = chars[i..].iter().take_while(|&&c| c == '*').count();
            i += star_count;
            if star_count >= 2 {
                result.push_str(".*");
            } else {
                result.push_str("[^&]*");
            }
        } else {
            result.push_str(&regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }

    result
}

/// Parse a whistle-style regex pattern like `/regex/flags` and return the Rust regex string.
/// Whistle supports flags: i (case-insensitive), u (unicode, default in Rust).
/// We also support m (multiline) and s (dotall) for extended compat.
pub fn parse_regex_with_flags(pattern: &str) -> Option<String> {
    if !pattern.starts_with('/') {
        return None;
    }

    // Find the last '/' that closes the regex
    let body_start = 1;
    if let Some(last_slash) = pattern[body_start..].rfind('/') {
        let last_slash = last_slash + body_start;
        if last_slash > 0 {
            let regex_body = &pattern[body_start..last_slash];
            let flags_str = &pattern[last_slash + 1..];

            if regex_body.is_empty() {
                return None;
            }

            // Convert flags to Rust inline flags
            let mut inline_flags = String::new();
            for ch in flags_str.chars() {
                match ch {
                    'i' => inline_flags.push('i'),
                    'm' => inline_flags.push('m'),
                    's' => inline_flags.push('s'),
                    'u' | 'g' => {} // u is default in Rust, g not meaningful
                    _ => {}
                }
            }

            if inline_flags.is_empty() {
                return Some(regex_body.to_string());
            } else {
                return Some(format!("(?{}){}", inline_flags, regex_body));
            }
        }
    }

    None
}

/// Match pattern against text, supporting both substring and regex.
/// Used by includeFilter/excludeFilter and filter:// patterns.
pub fn pattern_matches(pattern: &str, text: &str) -> bool {
    // Try whistle-style /regex/flags format first
    if pattern.starts_with('/') {
        if let Some(regex_str) = parse_regex_with_flags(pattern) {
            if let Ok(re) = Regex::new(&regex_str) {
                return re.is_match(text);
            }
        }
        // Fallback: try treating content (minus outer slashes) as regex
        let trimmed = pattern.trim_matches('/');
        if !trimmed.is_empty() {
            if let Ok(re) = Regex::new(trimmed) {
                return re.is_match(text);
            }
        }
    }

    // Try as regex if it looks like a regex pattern
    if pattern.starts_with('^') || pattern.contains(".*") || pattern.ends_with('$') {
        if let Ok(re) = Regex::new(pattern) {
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
    UrlParams {
        modifications: UrlParamModifications,
    },

    /// Merge fields into a JSON/form request body (params:// / reqMerge://)
    RequestMerge { content: BodyContent },

    /// Replace URL path (pathReplace:// / urlReplace://)
    PathReplace {
        pattern: String,
        replacement: String,
    },

    /// Change HTTP method (method://)
    Method { method: String },

    /// Set request content type (reqType://)
    RequestType { content_type: String },

    /// Set request charset (reqCharset://)
    RequestCharset { charset: String },

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

    /// Merge fields into a JSON/JSONP/form response body (resMerge://)
    ResponseMerge { content: BodyContent },

    /// Replace with HTML body (htmlBody://)
    HtmlBody { content: String },

    /// Replace with CSS body (cssBody://)
    CssBody { content: String },

    /// Replace with JS body (jsBody://)
    JsBody { content: String },

    /// String replacement in response (resReplace://)
    ResponseReplace {
        pattern: String,
        replacement: String,
        regex: bool,
    },

    /// String replacement in request (reqReplace://)
    RequestReplace {
        pattern: String,
        replacement: String,
        regex: bool,
    },

    /// Set response cookies (resCookies://)
    ResponseCookies {
        cookies: HashMap<String, CookieOptions>,
    },

    /// Set response content type (resType://)
    ResponseType { content_type: String },

    /// Set response charset (resCharset://)
    ResponseCharset { charset: String },

    /// Set attachment header (attachment:// / download://)
    Attachment { filename: Option<String> },

    /// Configure response caching (cache://)
    Cache { policy: String },

    // === INJECTION ACTIONS ===
    /// Append to HTML (htmlAppend:// / html://)
    HtmlAppend { content: String },

    /// Prepend to HTML (htmlPrepend://)
    HtmlPrepend { content: String },

    /// Append JavaScript (jsAppend:// / js://)
    JsAppend { content: String },

    /// Prepend JavaScript (jsPrepend://)
    JsPrepend { content: String },

    /// Append CSS (cssAppend:// / css://)
    CssAppend { content: String },

    /// Prepend CSS (cssPrepend://)
    CssPrepend { content: String },

    // === CORS ACTIONS ===
    /// Set request CORS headers (reqCors://)
    RequestCors {
        origin: Option<String>,
        credentials: bool,
    },

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
    Plugin {
        name: String,
        config: serde_json::Value,
    },

    /// Log request (log://)
    Log { message: Option<String> },

    /// Ignore/skip rule processing (ignore:// / skip:// / filter://)
    Ignore,

    /// Enable specific features (enable://)
    Enable { features: Vec<String> },

    /// Disable specific features (disable://)
    Disable { features: Vec<String> },

    /// Known whistle protocol that PostGate parses but cannot faithfully apply.
    /// Keeping this as an action lets the UI warn instead of silently dropping it.
    Unsupported { protocol: String, value: String },

    // === PROXY CONFIGURATION ===
    /// Use HTTP proxy (proxy://, http-proxy://)
    HttpProxy {
        host: String,
        port: u16,
        auth: Option<ProxyAuth>,
    },

    /// Use HTTPS proxy (https-proxy://)
    HttpsProxy {
        host: String,
        port: u16,
        auth: Option<ProxyAuth>,
    },

    /// Use SOCKS proxy (socks://)
    SocksProxy {
        host: String,
        port: u16,
        version: u8,
        auth: Option<ProxyAuth>,
    },

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
    HtmlReplace {
        pattern: String,
        replacement: String,
        regex: bool,
    },

    /// Replace content in JavaScript (jsReplace://)
    JsReplace {
        pattern: String,
        replacement: String,
        regex: bool,
    },

    /// Replace content in CSS (cssReplace://)
    CssReplace {
        pattern: String,
        replacement: String,
        regex: bool,
    },

    /// Prepend to request body (reqPrepend://)
    RequestPrepend { content: String },

    /// Append to request body (reqAppend://)
    RequestAppend { content: String },

    /// Prepend to response body (resPrepend://)
    ResponsePrepend { content: String },

    /// Append to response body (resAppend://)
    ResponseAppend { content: String },

    /// Write the request body to a local file (reqWrite:// / reqWriteRaw://)
    RequestWrite { path: String, raw: bool },

    /// Write the response body to a local file (resWrite:// / resWriteRaw://)
    ResponseWrite { path: String, raw: bool },

    /// Set x-whistle-response-for (responseFor://)
    ResponseFor { value: String },
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
    pub set: HashMap<String, String>,
    pub remove: Vec<String>,
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
    Text {
        content: String,
        content_type: String,
    },
    Json {
        value: serde_json::Value,
    },
    File {
        path: PathBuf,
    },
    Base64 {
        data: String,
    },
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
    fn test_pattern_matches_fn() {
        // Test regex with flags
        assert!(pattern_matches("/api/i", "https://example.com/API/users"));
        assert!(pattern_matches("/api/i", "https://example.com/api/users"));

        // Test plain regex
        assert!(pattern_matches(
            "^https://.*\\.example\\.com",
            "https://api.example.com/test"
        ));

        // Test substring
        assert!(pattern_matches("/api", "https://example.com/api/users"));
        assert!(!pattern_matches("/api", "https://example.com/other"));
    }

    #[test]
    fn test_path_prefix_starts_with() {
        let prefix = Pattern::PathPrefix("/api".to_string());
        // Should match: starts with /api + separator boundary
        assert!(prefix.matches("https://example.com/api/users"));
        assert!(prefix.matches("https://example.com/api?q=1"));
        assert!(prefix.matches("https://example.com/api"));
        // Should NOT match: /api-v2 (no separator boundary)
        assert!(!prefix.matches("https://example.com/api-v2"));
        // Should NOT match: /v1/api (doesn't start with /api)
        assert!(!prefix.matches("https://example.com/v1/api"));
    }

    #[test]
    fn test_exact_pattern() {
        let exact = Pattern::Exact("https://example.com/api".to_string());
        assert!(exact.matches("https://example.com/api"));
        // Whistle: exact also matches with query stripped
        assert!(exact.matches("https://example.com/api?q=1"));
        assert!(!exact.matches("https://example.com/api/v2"));

        let exact_port = Pattern::Exact("https://example.com:8443/api".to_string());
        assert!(exact_port.matches("https://example.com:8443/api?q=1"));
        assert!(!exact_port.matches("https://example.com/api?q=1"));
    }

    #[test]
    fn test_domain_pattern() {
        let domain = Pattern::Domain("example.com".to_string());
        assert!(domain.matches_host("example.com"));
        assert!(domain.matches_host("api.example.com"));
        assert!(!domain.matches_host("example.org"));
    }

    #[test]
    fn test_port_pattern() {
        let port = Pattern::Port(8080);
        assert!(
            port.match_with_remainder("https://example.com/path", 8080)
                .matched
        );
        assert!(
            !port
                .match_with_remainder("https://example.com/path", 443)
                .matched
        );
    }

    #[test]
    fn test_url_pattern_boundary_check() {
        let pattern = Pattern::Url {
            protocol: Some("https".to_string()),
            host: "example.com".to_string(),
            path: Some("/api".to_string()),
        };
        // /api/users — boundary char is /
        assert!(
            pattern
                .match_with_remainder("https://example.com/api/users", 443)
                .matched
        );
        // /api?q=1 — boundary char is ?
        assert!(
            pattern
                .match_with_remainder("https://example.com/api?q=1", 443)
                .matched
        );
        // /api-v2 — boundary char is - (not a separator)
        assert!(
            !pattern
                .match_with_remainder("https://example.com/api-v2", 443)
                .matched
        );
    }

    #[test]
    fn test_no_schema_pattern() {
        let ns = Pattern::NoSchema {
            host: "example.com".to_string(),
            path: Some("/api".to_string()),
        };
        // Should match any protocol
        assert!(
            ns.match_with_remainder("https://example.com/api/users", 443)
                .matched
        );
        assert!(
            ns.match_with_remainder("http://example.com/api/users", 80)
                .matched
        );
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
    fn test_content_type_filter_requires_header_match() {
        let filters = RuleFilters {
            content_types: vec!["json".to_string()],
            ..Default::default()
        };

        let mut headers = HashMap::new();
        assert!(!filters.matches("GET", "https", 443, &headers, "https://example.com"));

        headers.insert("content-type".to_string(), "application/json".to_string());
        assert!(filters.matches("GET", "https", 443, &headers, "https://example.com"));

        headers.insert("content-type".to_string(), "text/html".to_string());
        assert!(!filters.matches("GET", "https", 443, &headers, "https://example.com"));
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

    #[test]
    fn test_whistle_wildcard_multi_star() {
        // ** in domain = any chars (no slash)
        assert!(whistle_wildcard_match(
            "https://**.example.com/",
            "https://sub.example.com/"
        ));
        assert!(whistle_wildcard_match(
            "https://**.example.com/",
            "https://deep.sub.example.com/"
        ));

        // ** in path = any path
        assert!(whistle_wildcard_match(
            "https://example.com/**",
            "https://example.com/a/b/c"
        ));

        // *** in path = anything
        assert!(whistle_wildcard_match(
            "https://example.com/api/***",
            "https://example.com/api/a/b?q=1"
        ));
    }
}
