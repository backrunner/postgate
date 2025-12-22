//! Whistle-compatible rule parser
//!
//! Parses rules in whistle syntax format:
//! pattern operatorURI
//!
//! Examples:
//! - example.com host://127.0.0.1:8080
//! - /api/* resHeaders://{content-type: application/json}
//! - ^https://.*\.example\.com$ resCors://* reqDelay://1000

use super::types::{
    BodyContent, CookieOptions, HeaderModifications, Pattern, ProxyAuth, Rule, RuleAction,
    RuleFilters, UrlParamModifications,
};
use crate::error::{PostGateError, Result};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

/// Parse whistle-compatible rules from text
pub fn parse_rules(content: &str) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        match parse_rule_line(line) {
            Ok(rule) => rules.push(rule),
            Err(e) => {
                tracing::warn!(
                    "Failed to parse rule at line {}: {} - {}",
                    line_num + 1,
                    line,
                    e
                );
            }
        }
    }

    Ok(rules)
}

/// Parse a single rule line
fn parse_rule_line(line: &str) -> Result<Rule> {
    // Split into pattern and action parts
    let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();

    if parts.len() < 2 {
        return Err(PostGateError::RuleParse(format!(
            "Invalid rule format: {}",
            line
        )));
    }

    let pattern_str = parts[0].trim();
    let action_str = parts[1].trim();

    let pattern = parse_pattern(pattern_str)?;
    let (actions, filters) = parse_actions_and_filters(action_str)?;

    Ok(Rule {
        id: Uuid::new_v4().to_string(),
        pattern,
        filters: if filters.is_empty() {
            None
        } else {
            Some(filters)
        },
        actions,
        enabled: true,
        priority: 0,
        raw_line: line.to_string(),
    })
}

/// Parse a pattern string into a Pattern
fn parse_pattern(s: &str) -> Result<Pattern> {
    // Check for regex pattern (starts with ^)
    if s.starts_with('^') {
        let regex = Regex::new(s)
            .map_err(|e| PostGateError::RuleParse(format!("Invalid regex: {}", e)))?;
        return Ok(Pattern::Regex(regex));
    }

    // Check for regex pattern (enclosed in /)
    if s.starts_with('/') && s.len() > 1 {
        if let Some(end) = s[1..].rfind('/') {
            let regex_str = &s[1..=end];
            let regex = Regex::new(regex_str)
                .map_err(|e| PostGateError::RuleParse(format!("Invalid regex: {}", e)))?;
            return Ok(Pattern::Regex(regex));
        }
    }

    // Check for URL pattern with protocol
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ws://") || s.starts_with("wss://") {
        return parse_url_pattern(s);
    }

    // Check for wildcard pattern
    if s.contains('*') || s.contains('?') {
        // Check if it's a domain wildcard (e.g., *.example.com)
        if s.starts_with("*.") || s.starts_with("**") {
            return Ok(Pattern::Wildcard(s.to_string()));
        }
        return Ok(Pattern::Wildcard(s.to_string()));
    }

    // Check for path prefix (starts with /)
    if s.starts_with('/') {
        return Ok(Pattern::PathPrefix(s.to_string()));
    }

    // Check for domain pattern (contains dots but not a full URL)
    if s.contains('.') && !s.contains('/') {
        return Ok(Pattern::Domain(s.to_string()));
    }

    // Default to exact match
    Ok(Pattern::Exact(s.to_string()))
}

/// Parse a URL pattern into Pattern::Url
fn parse_url_pattern(s: &str) -> Result<Pattern> {
    let (protocol, rest) = if let Some(idx) = s.find("://") {
        (Some(s[..idx].to_string()), &s[idx + 3..])
    } else {
        (None, s)
    };

    let (host, path) = if let Some(idx) = rest.find('/') {
        (rest[..idx].to_string(), Some(rest[idx..].to_string()))
    } else {
        (rest.to_string(), None)
    };

    Ok(Pattern::Url {
        protocol,
        host,
        path,
    })
}

/// Parse action string into RuleActions and optional filters
fn parse_actions_and_filters(s: &str) -> Result<(Vec<RuleAction>, RuleFilters)> {
    let mut actions = Vec::new();
    let mut filters = RuleFilters::default();

    // Split by whitespace, but respect quoted strings and JSON objects
    let action_parts = split_action_string(s);

    for part in action_parts {
        // Check for filter operators
        if parse_filter(&part, &mut filters)? {
            continue;
        }

        if let Some(action) = parse_single_action(&part)? {
            actions.push(action);
        }
    }

    if actions.is_empty() {
        return Err(PostGateError::RuleParse("No valid actions found".into()));
    }

    Ok((actions, filters))
}

/// Parse filter operators, returns true if it was a filter
fn parse_filter(s: &str, filters: &mut RuleFilters) -> Result<bool> {
    // IMPORTANT: Skip if this looks like an action (protocol://value format)
    // This prevents "host://..." from being matched as "host:" filter
    if s.contains("://") {
        return Ok(false);
    }

    // Method filter: m:GET,POST or method:GET
    if let Some(value) = s.strip_prefix("m:").or_else(|| s.strip_prefix("method:")) {
        filters.methods = value.split(',').map(|s| s.trim().to_uppercase()).collect();
        return Ok(true);
    }

    // Protocol filter: p:https,wss or protocol:https
    if let Some(value) = s.strip_prefix("p:").or_else(|| s.strip_prefix("protocol:")) {
        filters.protocols = value.split(',').map(|s| s.trim().to_lowercase()).collect();
        return Ok(true);
    }

    // Port filter: port:443,8080
    if let Some(value) = s.strip_prefix("port:") {
        filters.ports = value
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        return Ok(true);
    }

    // Content type filter: ct:application/json or contentType:
    if let Some(value) = s
        .strip_prefix("ct:")
        .or_else(|| s.strip_prefix("contentType:"))
        .or_else(|| s.strip_prefix("reqContentType:"))
        .or_else(|| s.strip_prefix("resContentType:"))
    {
        filters.content_types = value.split(',').map(|s| s.trim().to_string()).collect();
        return Ok(true);
    }

    // IP filter: i:, ip:, clientIp:
    if let Some(value) = s
        .strip_prefix("i:")
        .or_else(|| s.strip_prefix("ip:"))
        .or_else(|| s.strip_prefix("clientIp:"))
    {
        filters.client_ips = value.split(',').map(|s| s.trim().to_string()).collect();
        return Ok(true);
    }

    // Host filter: h:, host:, hostname:
    if let Some(value) = s
        .strip_prefix("h:")
        .or_else(|| s.strip_prefix("host:"))
        .or_else(|| s.strip_prefix("hostname:"))
    {
        filters.hosts = value.split(',').map(|s| s.trim().to_string()).collect();
        return Ok(true);
    }

    // Status code filter: s:, statusCode:
    if let Some(value) = s
        .strip_prefix("s:")
        .or_else(|| s.strip_prefix("statusCode:"))
    {
        filters.status_codes = value
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        return Ok(true);
    }

    // Exclude filter: excludeFilter:pattern (supports regex)
    if let Some(value) = s
        .strip_prefix("excludeFilter://")
        .or_else(|| s.strip_prefix("excludeFilter:"))
    {
        filters.exclude.push(value.to_string());
        return Ok(true);
    }

    // Include filter: includeFilter:pattern (supports regex)
    if let Some(value) = s
        .strip_prefix("includeFilter://")
        .or_else(|| s.strip_prefix("includeFilter:"))
    {
        filters.include.push(value.to_string());
        return Ok(true);
    }

    Ok(false)
}

/// Split action string respecting quotes and JSON braces
fn split_action_string(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut brace_depth: u32 = 0;
    let mut bracket_depth: u32 = 0;

    for c in s.chars() {
        match c {
            '"' | '\'' if brace_depth == 0 && bracket_depth == 0 && !in_quotes => {
                in_quotes = true;
                quote_char = c;
                current.push(c);
            }
            c if c == quote_char && in_quotes => {
                in_quotes = false;
                current.push(c);
            }
            '{' if !in_quotes => {
                brace_depth += 1;
                current.push(c);
            }
            '}' if !in_quotes => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(c);
            }
            '[' if !in_quotes => {
                bracket_depth += 1;
                current.push(c);
            }
            ']' if !in_quotes => {
                bracket_depth = bracket_depth.saturating_sub(1);
                current.push(c);
            }
            ' ' | '\t' if !in_quotes && brace_depth == 0 && bracket_depth == 0 => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

/// Parse a single action
fn parse_single_action(s: &str) -> Result<Option<RuleAction>> {
    // IMPORTANT: Check for bare URLs FIRST before protocol://value parsing
    // This handles whistle syntax like: pattern http://target.com
    // where http://target.com should be treated as a host redirect, not as protocol "http"
    if s.starts_with("http://") || s.starts_with("https://") {
        return Ok(Some(RuleAction::Host {
            target: s.to_string(),
        }));
    }

    // Parse protocol://value format (for whistle actions like host://, file://, etc.)
    if let Some(idx) = s.find("://") {
        let protocol = &s[..idx].to_lowercase();
        let value = &s[idx + 3..];

        let action = match protocol.as_str() {
            // === HOST/REDIRECT ACTIONS ===
            "host" => RuleAction::Host {
                target: value.to_string(),
            },

            "file" => RuleAction::File { path: value.into() },

            "redirect" | "302" => RuleAction::Redirect {
                url: value.to_string(),
                status: 302,
            },

            "301" => RuleAction::Redirect {
                url: value.to_string(),
                status: 301,
            },

            "307" => RuleAction::Redirect {
                url: value.to_string(),
                status: 307,
            },

            "308" => RuleAction::Redirect {
                url: value.to_string(),
                status: 308,
            },

            "statuscode" | "status" | "replaceStatus" => {
                let code: u16 = value.parse().map_err(|_| {
                    PostGateError::RuleParse(format!("Invalid status code: {}", value))
                })?;
                RuleAction::StatusCode { code }
            }

            // === REQUEST MODIFICATIONS ===
            "reqheaders" | "reqheader" => {
                let modifications = parse_header_modifications(value)?;
                RuleAction::RequestHeaders { modifications }
            }

            "reqbody" => RuleAction::RequestBody {
                content: parse_body_content(value)?,
            },

            "urlparams" => {
                let modifications = parse_url_param_modifications(value)?;
                RuleAction::UrlParams { modifications }
            }

            "pathreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                RuleAction::PathReplace {
                    pattern,
                    replacement,
                }
            }

            "method" => RuleAction::Method {
                method: value.to_uppercase(),
            },

            "ua" | "useragent" => RuleAction::UserAgent {
                value: value.to_string(),
            },

            "referer" | "referrer" => RuleAction::Referer {
                value: value.to_string(),
            },

            "auth" => {
                let (username, password) = parse_auth(value)?;
                RuleAction::Auth { username, password }
            }

            "reqcookies" | "reqcookie" => {
                let cookies = parse_simple_cookies(value)?;
                RuleAction::RequestCookies { cookies }
            }

            "forwardedfor" | "xff" => RuleAction::ForwardedFor {
                value: value.to_string(),
            },

            "reqreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                let is_regex = pattern.starts_with('/') || pattern.starts_with('^');
                RuleAction::RequestReplace {
                    pattern,
                    replacement,
                    regex: is_regex,
                }
            }

            // === RESPONSE MODIFICATIONS ===
            "resheaders" | "resheader" => {
                let modifications = parse_header_modifications(value)?;
                RuleAction::ResponseHeaders { modifications }
            }

            "resbody" => RuleAction::ResponseBody {
                content: parse_body_content(value)?,
            },

            "htmlbody" => RuleAction::HtmlBody {
                content: value.to_string(),
            },

            "cssbody" => RuleAction::CssBody {
                content: value.to_string(),
            },

            "jsbody" => RuleAction::JsBody {
                content: value.to_string(),
            },

            "resreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                let is_regex = pattern.starts_with('/') || pattern.starts_with('^');
                RuleAction::ResponseReplace {
                    pattern,
                    replacement,
                    regex: is_regex,
                }
            }

            "rescookies" | "rescookie" => {
                let cookies = parse_response_cookies(value)?;
                RuleAction::ResponseCookies { cookies }
            }

            "restype" | "contenttype" => RuleAction::ResponseType {
                content_type: value.to_string(),
            },

            "rescharset" | "charset" => RuleAction::ResponseCharset {
                charset: value.to_string(),
            },

            "attachment" => RuleAction::Attachment {
                filename: if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                },
            },

            // === INJECTION ACTIONS ===
            "htmlappend" => RuleAction::HtmlAppend {
                content: value.to_string(),
            },

            "htmlprepend" => RuleAction::HtmlPrepend {
                content: value.to_string(),
            },

            "jsappend" | "js" => RuleAction::JsAppend {
                content: value.to_string(),
            },

            "jsprepend" => RuleAction::JsPrepend {
                content: value.to_string(),
            },

            "cssappend" | "css" => RuleAction::CssAppend {
                content: value.to_string(),
            },

            "cssprepend" => RuleAction::CssPrepend {
                content: value.to_string(),
            },

            // === CORS ACTIONS ===
            "reqcors" => {
                let (origin, credentials) = parse_cors_value(value);
                RuleAction::RequestCors {
                    origin,
                    credentials,
                }
            }

            "rescors" | "cors" => {
                let cors = parse_response_cors(value)?;
                cors
            }

            // === PERFORMANCE ACTIONS ===
            "reqdelay" | "delay" => {
                let ms: u64 = value.parse().map_err(|_| {
                    PostGateError::RuleParse(format!("Invalid delay: {}", value))
                })?;
                RuleAction::Delay {
                    request_ms: Some(ms),
                    response_ms: None,
                }
            }

            "resdelay" => {
                let ms: u64 = value.parse().map_err(|_| {
                    PostGateError::RuleParse(format!("Invalid delay: {}", value))
                })?;
                RuleAction::Delay {
                    request_ms: None,
                    response_ms: Some(ms),
                }
            }

            "reqspeed" => {
                let kbps: u64 = value.parse().map_err(|_| {
                    PostGateError::RuleParse(format!("Invalid speed: {}", value))
                })?;
                RuleAction::Speed {
                    request_kbps: Some(kbps),
                    response_kbps: None,
                }
            }

            "resspeed" => {
                let kbps: u64 = value.parse().map_err(|_| {
                    PostGateError::RuleParse(format!("Invalid speed: {}", value))
                })?;
                RuleAction::Speed {
                    request_kbps: None,
                    response_kbps: Some(kbps),
                }
            }

            "speed" => {
                let kbps: u64 = value.parse().map_err(|_| {
                    PostGateError::RuleParse(format!("Invalid speed: {}", value))
                })?;
                RuleAction::Speed {
                    request_kbps: Some(kbps),
                    response_kbps: Some(kbps),
                }
            }

            // === DEBUGGING/CONTROL ===
            "debug" | "weinre" => RuleAction::Debug {
                name: value.to_string(),
            },

            "plugin" => RuleAction::Plugin {
                name: value.to_string(),
                config: serde_json::Value::Null,
            },

            "log" => RuleAction::Log {
                message: if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                },
            },

            "ignore" | "filter" => RuleAction::Ignore,

            "enable" => RuleAction::Enable {
                features: value.split(',').map(|s| s.trim().to_string()).collect(),
            },

            "disable" => RuleAction::Disable {
                features: value.split(',').map(|s| s.trim().to_string()).collect(),
            },

            // === PROXY CONFIGURATION ===
            "proxy" | "http-proxy" | "httpproxy" => {
                let (host, port, auth) = parse_proxy_address(value)?;
                RuleAction::HttpProxy { host, port, auth }
            }

            "https-proxy" | "httpsproxy" => {
                let (host, port, auth) = parse_proxy_address(value)?;
                RuleAction::HttpsProxy { host, port, auth }
            }

            "socks" | "socks5" => {
                let (host, port, auth) = parse_proxy_address(value)?;
                RuleAction::SocksProxy {
                    host,
                    port,
                    version: 5,
                    auth,
                }
            }

            "socks4" => {
                let (host, port, auth) = parse_proxy_address(value)?;
                RuleAction::SocksProxy {
                    host,
                    port,
                    version: 4,
                    auth,
                }
            }

            // === ADDITIONAL WHISTLE ACTIONS ===
            "jsonbody" => {
                let json_value = serde_json::from_str(value)
                    .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
                RuleAction::JsonBody { value: json_value }
            }

            "timeout" => {
                let ms: u64 = value.parse().map_err(|_| {
                    PostGateError::RuleParse(format!("Invalid timeout: {}", value))
                })?;
                RuleAction::Timeout { ms }
            }

            "delete" => {
                let headers: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
                RuleAction::DeleteHeaders { headers }
            }

            "echo" => RuleAction::Echo,

            "mock" => RuleAction::Mock { path: value.into() },

            "htmlreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                let is_regex = pattern.starts_with('/') || pattern.starts_with('^');
                RuleAction::HtmlReplace { pattern, replacement, regex: is_regex }
            }

            "jsreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                let is_regex = pattern.starts_with('/') || pattern.starts_with('^');
                RuleAction::JsReplace { pattern, replacement, regex: is_regex }
            }

            "cssreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                let is_regex = pattern.starts_with('/') || pattern.starts_with('^');
                RuleAction::CssReplace { pattern, replacement, regex: is_regex }
            }

            "reqprepend" => RuleAction::RequestPrepend { content: value.to_string() },

            "reqappend" => RuleAction::RequestAppend { content: value.to_string() },

            "resprepend" => RuleAction::ResponsePrepend { content: value.to_string() },

            "resappend" => RuleAction::ResponseAppend { content: value.to_string() },

            _ => {
                tracing::warn!("Unknown action protocol: {}", protocol);
                return Ok(None);
            }
        };

        return Ok(Some(action));
    }

    // Handle IP:port as host redirect (e.g., 127.0.0.1:8080)
    if s.contains(':') && s.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
        return Ok(Some(RuleAction::Host {
            target: s.to_string(),
        }));
    }

    Ok(None)
}

/// Parse header modifications from JSON or key=value format
fn parse_header_modifications(s: &str) -> Result<HeaderModifications> {
    // Try JSON first
    if s.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<HashMap<String, serde_json::Value>>(s) {
            let mut modifications = HeaderModifications::default();
            for (key, value) in json {
                match value {
                    serde_json::Value::Null => {
                        modifications.remove.push(key);
                    }
                    serde_json::Value::String(v) => {
                        modifications.set.insert(key, v);
                    }
                    _ => {
                        modifications.set.insert(key, value.to_string());
                    }
                }
            }
            return Ok(modifications);
        }
    }

    // Parse key=value format or key:value format
    let mut modifications = HeaderModifications::default();

    for pair in s.split(',') {
        let pair = pair.trim();
        if pair.starts_with('-') {
            // Remove header
            modifications.remove.push(pair[1..].to_string());
        } else if pair.starts_with('+') {
            // Append header
            let rest = &pair[1..];
            if let Some(idx) = rest.find(['=', ':']) {
                let key = rest[..idx].trim().to_string();
                let value = rest[idx + 1..].trim().to_string();
                modifications.append.insert(key, value);
            }
        } else if let Some(idx) = pair.find(['=', ':']) {
            let key = pair[..idx].trim().to_string();
            let value = pair[idx + 1..].trim().to_string();
            modifications.set.insert(key, value);
        }
    }

    Ok(modifications)
}

/// Parse body content from string
fn parse_body_content(s: &str) -> Result<BodyContent> {
    // File reference
    if s.starts_with('/') || s.starts_with("./") || s.starts_with("~/") {
        return Ok(BodyContent::File { path: s.into() });
    }

    // JSON object/array
    if s.starts_with('{') || s.starts_with('[') {
        if let Ok(value) = serde_json::from_str(s) {
            return Ok(BodyContent::Json { value });
        }
    }

    // Base64 marker
    if s.starts_with("base64:") {
        return Ok(BodyContent::Base64 {
            data: s[7..].to_string(),
        });
    }

    // Empty marker
    if s.is_empty() || s == "empty" {
        return Ok(BodyContent::Empty);
    }

    // Default to text
    Ok(BodyContent::Text {
        content: s.to_string(),
        content_type: "text/plain".to_string(),
    })
}

/// Parse URL parameter modifications
fn parse_url_param_modifications(s: &str) -> Result<UrlParamModifications> {
    let mut modifications = UrlParamModifications::default();

    // Try JSON first
    if s.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<HashMap<String, serde_json::Value>>(s) {
            for (key, value) in json {
                match value {
                    serde_json::Value::Null => {
                        modifications.remove.push(key);
                    }
                    serde_json::Value::String(v) => {
                        modifications.set.insert(key, v);
                    }
                    _ => {
                        modifications.set.insert(key, value.to_string());
                    }
                }
            }
            return Ok(modifications);
        }
    }

    // Parse key=value format
    for pair in s.split('&') {
        let pair = pair.trim();
        if pair.starts_with('-') {
            modifications.remove.push(pair[1..].to_string());
        } else if pair.starts_with('+') {
            let rest = &pair[1..];
            if let Some(idx) = rest.find('=') {
                let key = rest[..idx].to_string();
                let value = rest[idx + 1..].to_string();
                modifications.append.insert(key, value);
            }
        } else if let Some(idx) = pair.find('=') {
            let key = pair[..idx].to_string();
            let value = pair[idx + 1..].to_string();
            modifications.set.insert(key, value);
        }
    }

    Ok(modifications)
}

/// Parse a replacement pair (pattern->replacement)
fn parse_replace_pair(s: &str) -> Result<(String, String)> {
    // Format: pattern->replacement or pattern|replacement
    let separator = if s.contains("->") {
        "->"
    } else if s.contains('|') {
        "|"
    } else {
        return Err(PostGateError::RuleParse(format!(
            "Invalid replace format, expected pattern->replacement: {}",
            s
        )));
    };

    let parts: Vec<&str> = s.splitn(2, separator).collect();
    if parts.len() != 2 {
        return Err(PostGateError::RuleParse(format!(
            "Invalid replace format: {}",
            s
        )));
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse auth string (username:password)
fn parse_auth(s: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(PostGateError::RuleParse(format!(
            "Invalid auth format, expected username:password: {}",
            s
        )));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse simple cookies (key=value format)
fn parse_simple_cookies(s: &str) -> Result<HashMap<String, String>> {
    let mut cookies = HashMap::new();

    // Try JSON first
    if s.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<HashMap<String, String>>(s) {
            return Ok(json);
        }
    }

    // Parse key=value; format
    for pair in s.split(';') {
        let pair = pair.trim();
        if let Some(idx) = pair.find('=') {
            let key = pair[..idx].trim().to_string();
            let value = pair[idx + 1..].trim().to_string();
            cookies.insert(key, value);
        }
    }

    Ok(cookies)
}

/// Parse response cookies with options
fn parse_response_cookies(s: &str) -> Result<HashMap<String, CookieOptions>> {
    let mut cookies = HashMap::new();

    // Try JSON first for full options
    if s.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<HashMap<String, CookieOptions>>(s) {
            return Ok(json);
        }
        // Try simple JSON
        if let Ok(json) = serde_json::from_str::<HashMap<String, String>>(s) {
            for (key, value) in json {
                cookies.insert(
                    key,
                    CookieOptions {
                        value,
                        path: None,
                        domain: None,
                        max_age: None,
                        secure: false,
                        http_only: false,
                        same_site: None,
                    },
                );
            }
            return Ok(cookies);
        }
    }

    // Parse simple format
    for pair in s.split(';') {
        let pair = pair.trim();
        if let Some(idx) = pair.find('=') {
            let key = pair[..idx].trim().to_string();
            let value = pair[idx + 1..].trim().to_string();
            cookies.insert(
                key,
                CookieOptions {
                    value,
                    path: None,
                    domain: None,
                    max_age: None,
                    secure: false,
                    http_only: false,
                    same_site: None,
                },
            );
        }
    }

    Ok(cookies)
}

/// Parse CORS value for request CORS
fn parse_cors_value(s: &str) -> (Option<String>, bool) {
    if s == "*" || s.is_empty() {
        return (Some("*".to_string()), true);
    }

    // Check for credentials flag
    if s.contains("credentials") {
        let origin = s.replace("credentials", "").trim().to_string();
        return (
            if origin.is_empty() {
                Some("*".to_string())
            } else {
                Some(origin)
            },
            true,
        );
    }

    (Some(s.to_string()), false)
}

/// Parse response CORS options
fn parse_response_cors(s: &str) -> Result<RuleAction> {
    // Simple * means allow all
    if s == "*" || s.is_empty() {
        return Ok(RuleAction::ResponseCors {
            origin: Some("*".to_string()),
            methods: Some("GET,POST,PUT,DELETE,OPTIONS,PATCH".to_string()),
            headers: Some("*".to_string()),
            credentials: true,
            max_age: Some(86400),
        });
    }

    // Try JSON
    if s.starts_with('{') {
        #[derive(Deserialize)]
        struct CorsOptions {
            origin: Option<String>,
            methods: Option<String>,
            headers: Option<String>,
            credentials: Option<bool>,
            #[serde(rename = "maxAge")]
            max_age: Option<u64>,
        }

        if let Ok(opts) = serde_json::from_str::<CorsOptions>(s) {
            return Ok(RuleAction::ResponseCors {
                origin: opts.origin,
                methods: opts.methods,
                headers: opts.headers,
                credentials: opts.credentials.unwrap_or(false),
                max_age: opts.max_age,
            });
        }
    }

    // Simple origin value
    Ok(RuleAction::ResponseCors {
        origin: Some(s.to_string()),
        methods: Some("GET,POST,PUT,DELETE,OPTIONS,PATCH".to_string()),
        headers: Some("*".to_string()),
        credentials: true,
        max_age: Some(86400),
    })
}

/// Parse proxy address (host:port or user:pass@host:port)
fn parse_proxy_address(s: &str) -> Result<(String, u16, Option<ProxyAuth>)> {
    let (auth, host_port) = if s.contains('@') {
        let parts: Vec<&str> = s.rsplitn(2, '@').collect();
        let auth_str = parts[1];
        let (username, password) = parse_auth(auth_str)?;
        (Some(ProxyAuth { username, password }), parts[0])
    } else {
        (None, s)
    };

    let parts: Vec<&str> = host_port.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(PostGateError::RuleParse(format!(
            "Invalid proxy address, expected host:port: {}",
            s
        )));
    }

    let port: u16 = parts[0]
        .parse()
        .map_err(|_| PostGateError::RuleParse(format!("Invalid port: {}", parts[0])))?;

    Ok((parts[1].to_string(), port, auth))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_rule() {
        let rules = parse_rules("example.com host://127.0.0.1:8080").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].pattern, Pattern::Domain(_)));
        assert!(matches!(rules[0].actions[0], RuleAction::Host { .. }));
    }

    #[test]
    fn test_parse_wildcard_rule() {
        let rules = parse_rules("*.example.com statusCode://404").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].pattern, Pattern::Wildcard(_)));
    }

    #[test]
    fn test_parse_regex_rule() {
        let rules = parse_rules("^https://.*\\.example\\.com$ host://localhost:3000").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].pattern, Pattern::Regex(_)));
    }

    #[test]
    fn test_parse_regex_slash_format() {
        let rules = parse_rules("/api\\/v[0-9]+/ host://localhost:3000").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].pattern, Pattern::Regex(_)));
    }

    #[test]
    fn test_skip_comments() {
        let content = r#"
# This is a comment
// This is also a comment
example.com host://127.0.0.1
"#;
        let rules = parse_rules(content).unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_parse_method_filter() {
        let rules = parse_rules("example.com m:GET,POST host://127.0.0.1").unwrap();
        assert_eq!(rules.len(), 1);
        let filters = rules[0].filters.as_ref().unwrap();
        assert_eq!(filters.methods, vec!["GET", "POST"]);
    }

    #[test]
    fn test_parse_cors() {
        let rules = parse_rules("*.api.com resCors://*").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].actions[0], RuleAction::ResponseCors { .. }));
    }

    #[test]
    fn test_parse_delay() {
        let rules = parse_rules("slow.api.com reqDelay://1000 resDelay://500").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].actions.len(), 2);
    }

    #[test]
    fn test_parse_header_json() {
        let rules =
            parse_rules(r#"example.com resHeaders://{"Content-Type":"application/json"}"#).unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::ResponseHeaders { modifications } = &rules[0].actions[0] {
            assert_eq!(modifications.set.get("Content-Type").unwrap(), "application/json");
        } else {
            panic!("Expected ResponseHeaders action");
        }
    }

    #[test]
    fn test_parse_proxy() {
        let rules = parse_rules("*.internal.com proxy://127.0.0.1:8888").unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::HttpProxy { host, port, .. } = &rules[0].actions[0] {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(*port, 8888);
        } else {
            panic!("Expected HttpProxy action");
        }
    }

    #[test]
    fn test_parse_url_params() {
        let rules = parse_rules(r#"api.com urlParams://{"debug":"true","cache":null}"#).unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::UrlParams { modifications } = &rules[0].actions[0] {
            assert_eq!(modifications.set.get("debug").unwrap(), "true");
            assert!(modifications.remove.contains(&"cache".to_string()));
        } else {
            panic!("Expected UrlParams action");
        }
    }

    #[test]
    fn test_parse_url_to_url_proxy() {
        // Whistle syntax: source URL -> target URL (bare URL as host redirect)
        let rules = parse_rules("https://v.qq.com/biu/u/history/ http://127.0.0.1:3000/browser").unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::Host { target } = &rules[0].actions[0] {
            assert_eq!(target, "http://127.0.0.1:3000/browser");
        } else {
            panic!("Expected Host action, got {:?}", rules[0].actions[0]);
        }
    }

    #[test]
    fn test_parse_bare_https_url() {
        let rules = parse_rules("example.com https://proxy.example.com/api").unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::Host { target } = &rules[0].actions[0] {
            assert_eq!(target, "https://proxy.example.com/api");
        } else {
            panic!("Expected Host action");
        }
    }
}
