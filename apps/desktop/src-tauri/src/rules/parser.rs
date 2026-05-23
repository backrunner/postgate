//! Whistle-compatible rule parser
//!
//! Parses rules in whistle syntax format, supporting both normal and reverse syntax:
//! - Normal: `pattern operatorURI [operatorURI2] [filter1] [filter2]`
//! - Reverse: `operatorURI pattern [pattern2]`
//!
//! Examples:
//! - example.com host://127.0.0.1:8080
//! - /api/* resHeaders://{content-type: application/json}
//! - ^https://.*\.example\.com$ resCors://* reqDelay://1000
//! - host://127.0.0.1:8080 example.com (reverse syntax)

use super::types::{
    parse_regex_with_flags, BodyContent, CookieOptions, HeaderModifications, Pattern, ProxyAuth,
    Rule, RuleAction, RuleFilters, UrlParamModifications,
};
use crate::error::{PostGateError, Result};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

// =============================================================================
// Public API
// =============================================================================

/// Parse whistle-compatible rules from text
pub fn parse_rules(content: &str) -> Result<Vec<Rule>> {
    parse_rules_with_inline(content).map(|(rules, _)| rules)
}

/// Parse rules AND inline value definitions from a rule-group's raw content.
///
/// Inline values are fenced blocks of the form:
/// ````text
/// ``` name
/// body line 1
/// body line 2
/// ```
/// ````
/// The opening fence is three backticks followed by a name; the body runs
/// until a matching closing fence of three backticks on its own line. These
/// definitions have higher precedence than the global Values store
/// (whistle v1.12.12+ behaviour).
pub fn parse_rules_with_inline(content: &str) -> Result<(Vec<Rule>, HashMap<String, String>)> {
    let (rule_lines, inline_values) = extract_inline_values(content);

    let mut rules = Vec::new();
    for (line_num, line) in rule_lines.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments.
        // Whistle: only `#` starts a comment. `//` is NOT a comment (no-schema pattern).
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match parse_rule_line(line) {
            Ok(mut parsed_rules) => rules.append(&mut parsed_rules),
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

    Ok((rules, inline_values))
}

/// Strip fenced ``` name\n…\n``` blocks from `content`, returning the cleaned
/// rule text and a name→body map.
fn extract_inline_values(content: &str) -> (String, HashMap<String, String>) {
    let mut cleaned = String::with_capacity(content.len());
    let mut values: HashMap<String, String> = HashMap::new();

    let mut in_block = false;
    let mut current_name = String::new();
    let mut current_body = String::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_block {
                // Closing fence: the payload becomes the value.
                values.insert(current_name.clone(), current_body.clone());
                current_name.clear();
                current_body.clear();
                in_block = false;
                // Drop the closing fence from rule content.
                continue;
            } else {
                // Opening fence: anything after the backticks is the name.
                // Accept whistle plugin-namespace form ```whistle.plugin/name
                let name = trimmed.trim_start_matches('`').trim();
                if !name.is_empty() {
                    current_name = name.to_string();
                    current_body.clear();
                    in_block = true;
                    continue;
                }
                // Fence with no name → treat as literal rule content.
            }
        }

        if in_block {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        } else {
            cleaned.push_str(line);
            cleaned.push('\n');
        }
    }

    // If the file ends mid-block, still capture what we have.
    if in_block && !current_name.is_empty() {
        values.insert(current_name, current_body);
    }

    (cleaned, values)
}

// =============================================================================
// Line Parsing — Normal & Reverse Syntax
// =============================================================================

/// Parse a single rule line, returning one or more rules.
/// Supports both normal syntax (pattern op1 op2 filter) and reverse syntax (op1 pattern1 pattern2).
fn parse_rule_line(line: &str) -> Result<Vec<Rule>> {
    let tokens = split_action_string(line);

    if tokens.len() < 2 {
        return Err(PostGateError::RuleParse(format!(
            "Invalid rule format (need at least 2 tokens): {}",
            line
        )));
    }

    // Apply shorthand expansions to each token
    let tokens: Vec<String> = tokens.into_iter().map(|t| format_shorthand(&t)).collect();

    // Find the pattern index using whistle's algorithm
    let pattern_index = index_of_pattern(&tokens);

    if pattern_index == 0 {
        // Normal syntax: pattern op1 op2 filter1 filter2
        parse_normal_syntax(&tokens, line)
    } else if pattern_index > 0 {
        // Reverse syntax: op1 [op2] pattern1 [pattern2]
        parse_reverse_syntax(&tokens, pattern_index, line)
    } else {
        // Couldn't determine pattern — treat first token as pattern (fallback)
        parse_normal_syntax(&tokens, line)
    }
}

/// Normal syntax: first token is pattern, rest are operators + filters
fn parse_normal_syntax(tokens: &[String], raw_line: &str) -> Result<Vec<Rule>> {
    let pattern_str = &tokens[0];
    let rest = &tokens[1..];

    let (negated, pattern) = parse_pattern(pattern_str)?;
    let (actions, filters) = parse_actions_and_filters_from_tokens(rest)?;

    if actions.is_empty() {
        return Err(PostGateError::RuleParse(format!(
            "No valid actions found in: {}",
            raw_line
        )));
    }

    Ok(vec![Rule {
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
        raw_line: raw_line.to_string(),
        negated,
    }])
}

/// Reverse syntax: tokens before pattern_index are operators, tokens at/after are patterns.
/// Generate cross-product: each operator × each pattern.
fn parse_reverse_syntax(
    tokens: &[String],
    pattern_index: usize,
    raw_line: &str,
) -> Result<Vec<Rule>> {
    let mut operators = Vec::new();
    let mut patterns = Vec::new();
    let mut filters = RuleFilters::default();

    // Tokens before pattern_index: could be operators or filters
    for token in &tokens[..pattern_index] {
        if parse_filter_token(token, &mut filters)? {
            continue;
        }
        operators.push(token.as_str());
    }

    // Tokens at/after pattern_index: patterns, but some might still be operators or filters
    for token in &tokens[pattern_index..] {
        if parse_filter_token(token, &mut filters)? {
            continue;
        }
        if is_pattern_token(token) || is_host_value(token).is_none() && !has_protocol(token) {
            patterns.push(token.as_str());
        } else {
            operators.push(token);
        }
    }

    if operators.is_empty() || patterns.is_empty() {
        return Err(PostGateError::RuleParse(format!(
            "Reverse syntax needs at least 1 operator and 1 pattern: {}",
            raw_line
        )));
    }

    // Generate cross-product
    let mut rules = Vec::new();
    for op_str in &operators {
        let action = parse_single_action(op_str)?;
        if let Some(action) = action {
            for pat_str in &patterns {
                let (negated, pattern) = parse_pattern(pat_str)?;
                rules.push(Rule {
                    id: Uuid::new_v4().to_string(),
                    pattern,
                    filters: if filters.is_empty() {
                        None
                    } else {
                        Some(filters.clone())
                    },
                    actions: vec![action.clone()],
                    enabled: true,
                    priority: 0,
                    raw_line: raw_line.to_string(),
                    negated,
                });
            }
        }
    }

    if rules.is_empty() {
        return Err(PostGateError::RuleParse(format!(
            "No valid rules generated from reverse syntax: {}",
            raw_line
        )));
    }

    Ok(rules)
}

// =============================================================================
// Pattern Detection (whistle's indexOfPattern / isPattern)
// =============================================================================

/// Find the index of the first pattern token in the list.
/// Returns 0 if first token is a pattern (normal syntax), >0 for reverse syntax.
fn index_of_pattern(tokens: &[String]) -> usize {
    let mut ip_index: Option<usize> = None;

    for (i, token) in tokens.iter().enumerate() {
        if is_pattern_token(token) {
            return i;
        }
        if !has_protocol(token) {
            if is_host_value(token).is_none() {
                // Not a protocol action, not a bare IP — must be a pattern
                return i;
            } else if ip_index.is_none() {
                ip_index = Some(i);
            }
        }
    }

    // Fallback: first IP-like token is the pattern
    ip_index.unwrap_or(0)
}

/// Check if a token looks like a pattern (not an operator).
/// Whistle's isPattern() logic.
fn is_pattern_token(token: &str) -> bool {
    // Port pattern: :8080 or !:8080
    if is_port_pattern(token) {
        return true;
    }
    // Exact pattern: starts with $
    if token.starts_with('$') {
        return true;
    }
    // Regex URL: starts with ^
    if token.starts_with('^') {
        return true;
    }
    // No-schema pattern: starts with // (but not ///)
    if token.starts_with("//") && !token.starts_with("///") {
        return true;
    }
    // Negative pattern: starts with !
    if token.starts_with('!') {
        return true;
    }
    // Web protocol URL (http://, https://, ws://, wss://, tunnel://)
    if token.starts_with("http://")
        || token.starts_with("https://")
        || token.starts_with("ws://")
        || token.starts_with("wss://")
        || token.starts_with("tunnel://")
    {
        return true;
    }
    // Regex: /pattern/ format
    if is_regex_pattern(token) {
        return true;
    }
    // Dot-suffix pattern: .json, .html$, etc.
    if is_dot_suffix_pattern(token) {
        return true;
    }
    // Wildcard in domain-like token (contains * but has dots)
    if token.contains('*') && token.contains('.') && !token.contains("://") {
        return true;
    }
    false
}

/// Check if token is a regex pattern: /something/[flags]
fn is_regex_pattern(s: &str) -> bool {
    if !s.starts_with('/') || s.len() < 3 {
        return false;
    }
    // Must have a closing /
    if let Some(end) = s[1..].rfind('/') {
        let end = end + 1;
        // After closing /, only valid flags
        let flags = &s[end + 1..];
        return flags
            .chars()
            .all(|c| matches!(c, 'i' | 'u' | 'g' | 'm' | 's'));
    }
    false
}

/// Check if token is a port pattern: :1234 or !:1234
fn is_port_pattern(s: &str) -> bool {
    let s = s.strip_prefix('!').unwrap_or(s);
    if let Some(rest) = s.strip_prefix(':') {
        rest.len() <= 5 && !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Check if token is a dot-suffix pattern: .json, .html$, etc.
fn is_dot_suffix_pattern(s: &str) -> bool {
    let s = s.strip_prefix('!').unwrap_or(s);
    if !s.starts_with('.') || s.len() < 2 {
        return false;
    }
    let rest = s.strip_suffix('$').unwrap_or(s);
    rest[1..].chars().all(|c| c.is_alphanumeric() || c == '-')
}

/// Check if a token has a known protocol (action format like host://...)
fn has_protocol(token: &str) -> bool {
    if let Some(idx) = token.find("://") {
        let proto = &token[..idx].to_lowercase();
        // Check if it's a known action protocol (not a web URL protocol)
        matches!(
            proto.as_str(),
            "host"
                | "hosts"
                | "xhost"
                | "file"
                | "redirect"
                | "statuscode"
                | "status"
                | "replacestatus"
                | "style"
                | "rule"
                | "pipe"
                | "pac"
                | "https2http-proxy"
                | "http2https-proxy"
                | "internal-proxy"
                | "reqheaders"
                | "reqheader"
                | "reqbody"
                | "reqtype"
                | "reqcharset"
                | "urlparams"
                | "params"
                | "reqmerge"
                | "pathreplace"
                | "urlreplace"
                | "method"
                | "cache"
                | "responsefor"
                | "rulesfile"
                | "rulefile"
                | "rulescript"
                | "rulescripts"
                | "reqscript"
                | "reqrules"
                | "resscript"
                | "resrules"
                | "framescript"
                | "ua"
                | "useragent"
                | "referer"
                | "referrer"
                | "auth"
                | "reqcookies"
                | "reqcookie"
                | "forwardedfor"
                | "xff"
                | "reqreplace"
                | "resheaders"
                | "resheader"
                | "headerreplace"
                | "resmerge"
                | "resbody"
                | "htmlbody"
                | "cssbody"
                | "jsbody"
                | "resreplace"
                | "rescookies"
                | "rescookie"
                | "restype"
                | "contenttype"
                | "rescharset"
                | "charset"
                | "trailers"
                | "attachment"
                | "download"
                | "htmlappend"
                | "htmlprepend"
                | "jsappend"
                | "jsprepend"
                | "cssappend"
                | "cssprepend"
                | "html"
                | "js"
                | "css"
                | "reqcors"
                | "rescors"
                | "cors"
                | "reqdelay"
                | "delay"
                | "resdelay"
                | "reqspeed"
                | "resspeed"
                | "speed"
                | "debug"
                | "weinre"
                | "plugin"
                | "log"
                | "ignore"
                | "filter"
                | "skip"
                | "enable"
                | "disable"
                | "proxy"
                | "http-proxy"
                | "httpproxy"
                | "https-proxy"
                | "httpsproxy"
                | "xproxy"
                | "xhttp-proxy"
                | "socks"
                | "socks5"
                | "socks4"
                | "jsonbody"
                | "timeout"
                | "delete"
                | "echo"
                | "mock"
                | "htmlreplace"
                | "jsreplace"
                | "cssreplace"
                | "reqprepend"
                | "reqappend"
                | "resprepend"
                | "resappend"
                | "reqwrite"
                | "reswrite"
                | "reqwriteraw"
                | "reswriteraw"
                | "cipher"
                | "tlsoptions"
                | "snicallback"
                | "301"
                | "302"
                | "307"
                | "308"
                | "includefilter"
                | "excludefilter"
        )
    } else {
        false
    }
}

/// Check if token is a bare IP address (with optional port).
/// Returns Some((host, port)) if it's an IP, None otherwise.
fn is_host_value(token: &str) -> Option<(String, Option<u16>)> {
    // IPv6 bracket format: [::1]:8080 or [::1]
    if token.starts_with('[') {
        if let Some(bracket_end) = token.find(']') {
            let ip = &token[1..bracket_end];
            let rest = &token[bracket_end + 1..];
            if rest.is_empty() {
                return Some((ip.to_string(), None));
            }
            if let Some(port_str) = rest.strip_prefix(':') {
                if let Ok(port) = port_str.parse::<u16>() {
                    return Some((ip.to_string(), Some(port)));
                }
            }
        }
        return None;
    }

    // IPv4 or numeric IP with optional port: 127.0.0.1:8080
    if token.chars().next().is_some_and(|c| c.is_ascii_digit()) && token.contains('.') {
        if let Some(colon_idx) = token.rfind(':') {
            let ip = &token[..colon_idx];
            let port_str = &token[colon_idx + 1..];
            if port_str.chars().all(|c| c.is_ascii_digit()) && !port_str.is_empty() {
                if let Ok(port) = port_str.parse::<u16>() {
                    return Some((ip.to_string(), Some(port)));
                }
            }
        }
        // IP without port
        return Some((token.to_string(), None));
    }

    None
}

// =============================================================================
// Pattern Parsing
// =============================================================================

/// Parse a pattern string into a Pattern, returning (negated, Pattern).
fn parse_pattern(s: &str) -> Result<(bool, Pattern)> {
    let (negated, s) = if let Some(stripped) = s.strip_prefix('!') {
        (true, stripped)
    } else {
        (false, s)
    };

    // Exact match: $pattern
    if let Some(rest) = s.strip_prefix('$') {
        if rest.is_empty() {
            return Err(PostGateError::RuleParse("Empty exact pattern".into()));
        }
        return Ok((negated, Pattern::Exact(rest.to_string())));
    }

    // Port pattern: :8080
    if let Some(rest) = s.strip_prefix(':') {
        if let Ok(port) = rest.parse::<u16>() {
            return Ok((negated, Pattern::Port(port)));
        }
    }

    // No-schema pattern: //host/path (but not ///)
    if s.starts_with("//") && !s.starts_with("///") {
        let rest = &s[2..];
        let host_end = rest.find(&['/', '?', '#'][..]).unwrap_or(rest.len());
        let host = rest[..host_end].to_string();

        let path = if host_end < rest.len() {
            let path_part = &rest[host_end..];
            let path_end = path_part.find(&['?', '#'][..]).unwrap_or(path_part.len());
            let path_str = &path_part[..path_end];
            if path_str.is_empty() {
                None
            } else {
                Some(path_str.to_string())
            }
        } else {
            None
        };

        return Ok((negated, Pattern::NoSchema { host, path }));
    }

    // Dot-suffix pattern: .json, .html$
    if is_dot_suffix_pattern(s) {
        let anchored = s.ends_with('$');
        let suffix = if anchored { &s[..s.len() - 1] } else { s };
        let regex_str = if anchored {
            format!("{}$", regex::escape(suffix))
        } else {
            format!("{}([?#/]|$)", regex::escape(suffix))
        };
        let regex = Regex::new(&regex_str)
            .map_err(|e| PostGateError::RuleParse(format!("Invalid dot-suffix regex: {}", e)))?;
        return Ok((negated, Pattern::Regex(regex)));
    }

    // Regex URL pattern: ^ prefix with wildcard expansion
    // Whistle: ^ = case-insensitive, ^^ = case-sensitive
    if s.starts_with('^') {
        let caret_count = s.chars().take_while(|&c| c == '^').count();
        let rest = &s[caret_count..];
        let case_insensitive = caret_count == 1;

        // Check if this is a simple regex (contains typical regex metacharacters after ^)
        // vs a whistle regex URL (domain-like with wildcards)
        if rest.contains("://") || rest.contains('.') || rest.contains('*') {
            // Whistle regex URL pattern — compile with wildcard expansion
            let regex_str = compile_regex_url_pattern(rest, case_insensitive, s.ends_with('$'));
            let regex = Regex::new(&regex_str)
                .map_err(|e| PostGateError::RuleParse(format!("Invalid regex URL: {}", e)))?;
            return Ok((negated, Pattern::Regex(regex)));
        } else {
            // Simple regex: ^pattern$
            let full = format!("^{}", rest);
            let regex = Regex::new(&full)
                .map_err(|e| PostGateError::RuleParse(format!("Invalid regex: {}", e)))?;
            return Ok((negated, Pattern::Regex(regex)));
        }
    }

    // Regex pattern (enclosed in /regex/flags)
    if is_regex_pattern(s) {
        if let Some(regex_str) = parse_regex_with_flags(s) {
            let regex = Regex::new(&regex_str)
                .map_err(|e| PostGateError::RuleParse(format!("Invalid regex: {}", e)))?;
            return Ok((negated, Pattern::Regex(regex)));
        }
    }

    // URL pattern with protocol
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("ws://")
        || s.starts_with("wss://")
    {
        return Ok((negated, parse_url_pattern(s)?));
    }

    // Wildcard pattern
    if s.contains('*') || s.contains('?') {
        return Ok((negated, Pattern::Wildcard(s.to_string())));
    }

    // Path prefix (starts with /)
    if s.starts_with('/') {
        return Ok((negated, Pattern::PathPrefix(s.to_string())));
    }

    // Domain pattern (contains dots but no /)
    if s.contains('.') && !s.contains('/') {
        return Ok((negated, Pattern::Domain(s.to_string())));
    }

    // Host+path pattern without protocol (e.g., vm.gtimg.cn/path/to/file.js)
    // Whistle treats bare host/path as no-schema patterns (match any protocol)
    if s.contains('.') && s.contains('/') {
        if let Some(slash_idx) = s.find('/') {
            let host = s[..slash_idx].to_string();
            let path_part = &s[slash_idx..];
            // Strip query and hash from path
            let path_end = path_part.find(&['?', '#'][..]).unwrap_or(path_part.len());
            let path_str = &path_part[..path_end];
            let path = if path_str.is_empty() {
                None
            } else {
                Some(path_str.to_string())
            };
            return Ok((negated, Pattern::NoSchema { host, path }));
        }
    }

    // Default to exact match
    Ok((negated, Pattern::Exact(s.to_string())))
}

/// Compile a whistle ^ prefix regex URL pattern.
/// Handles domain/path/query wildcard expansion.
fn compile_regex_url_pattern(pattern: &str, case_insensitive: bool, end_anchored: bool) -> String {
    let mut result = String::new();

    if case_insensitive {
        result.push_str("(?i)");
    }

    // Split into protocol, domain, path, query
    let (proto, rest) = if let Some(idx) = pattern.find("://") {
        (&pattern[..idx + 3], &pattern[idx + 3..])
    } else {
        ("", pattern)
    };

    let (domain_path, query) = if let Some(idx) = rest.find('?') {
        (&rest[..idx], Some(&rest[idx..]))
    } else {
        (rest, None)
    };

    let (domain, path) = if let Some(idx) = domain_path.find('/') {
        (&domain_path[..idx], Some(&domain_path[idx..]))
    } else {
        (domain_path, None)
    };

    // Protocol
    if proto.is_empty() {
        result.push_str("[a-z]+://");
    } else {
        result.push_str(&regex::escape(proto));
    }

    // Domain — expand wildcards
    result.push_str(&expand_wildcards_domain(domain));

    // Optional port
    if !domain.contains(':') {
        result.push_str("(?::\\d+)?");
    }

    // Path
    if let Some(p) = path {
        result.push_str(&expand_wildcards_path(p));
    }

    // Query
    if let Some(q) = query {
        result.push_str(&expand_wildcards_query(q));
    }

    if end_anchored {
        result.push('$');
    }

    result
}

/// Expand wildcards in domain part for regex URL patterns
fn expand_wildcards_domain(domain: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = domain.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '*' {
            let count = chars[i..].iter().take_while(|&&c| c == '*').count();
            i += count;
            if count >= 3 && i < chars.len() && chars[i] == '.' {
                result.push_str("(?:[^/?]*\\.)?");
                i += 1;
            } else if count >= 2 {
                result.push_str("[^/?]*");
            } else {
                result.push_str("[^/?.]*");
            }
        } else {
            result.push_str(&regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }

    result
}

/// Expand wildcards in path part for regex URL patterns
fn expand_wildcards_path(path: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = path.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '*' {
            let count = chars[i..].iter().take_while(|&&c| c == '*').count();
            i += count;
            if count >= 3 {
                result.push_str(".*");
            } else if count >= 2 {
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

/// Expand wildcards in query part for regex URL patterns
fn expand_wildcards_query(query: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '*' {
            let count = chars[i..].iter().take_while(|&&c| c == '*').count();
            i += count;
            if count >= 2 {
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

/// Parse a URL pattern into Pattern::Url
fn parse_url_pattern(s: &str) -> Result<Pattern> {
    let (protocol, rest) = if let Some(idx) = s.find("://") {
        (Some(s[..idx].to_string()), &s[idx + 3..])
    } else {
        (None, s)
    };

    // Find host boundary: could be /, ?, or #
    let host_end = rest.find(&['/', '?', '#'][..]).unwrap_or(rest.len());
    let host = rest[..host_end].to_string();

    let path = if host_end < rest.len() {
        let path_part = &rest[host_end..];
        // Strip query and hash from the path
        let path_end = path_part.find(&['?', '#'][..]).unwrap_or(path_part.len());
        let path_str = &path_part[..path_end];
        if path_str.is_empty() {
            None
        } else {
            Some(path_str.to_string())
        }
    } else {
        None
    };

    Ok(Pattern::Url {
        protocol,
        host,
        path,
    })
}

// =============================================================================
// Shorthand Expansion
// =============================================================================

/// Apply whistle shorthand expansions to a token
fn format_shorthand(token: &str) -> String {
    // File path shorthand: /absolute/path → file:///absolute/path
    // (but NOT /regex/ patterns or /path patterns that start the rule)
    // We handle this carefully: only in action context, not pattern context
    // The caller context determines if this is useful.

    // <path> → file://<path>
    if token.starts_with('<') && token.ends_with('>') {
        return format!("file://{}", token);
    }

    // (value) → file://(value)
    if token.starts_with('(') && token.ends_with(')') {
        return format!("file://{}", token);
    }

    // No changes for most tokens
    token.to_string()
}

// =============================================================================
// Actions & Filters Parsing
// =============================================================================

/// Parse actions and filters from a slice of tokens
fn parse_actions_and_filters_from_tokens(
    tokens: &[String],
) -> Result<(Vec<RuleAction>, RuleFilters)> {
    let mut actions = Vec::new();
    let mut filters = RuleFilters::default();

    for token in tokens {
        if parse_filter_token(token, &mut filters)? {
            continue;
        }
        if let Some(action) = parse_single_action(token)? {
            actions.push(action);
        }
    }

    Ok((actions, filters))
}

/// Try to parse a token as a filter. Returns true if it was a filter.
fn parse_filter_token(s: &str, filters: &mut RuleFilters) -> Result<bool> {
    // Check includeFilter/excludeFilter BEFORE the :// guard,
    // because these use :// as part of their prefix
    if let Some(value) = s
        .strip_prefix("excludeFilter://")
        .or_else(|| s.strip_prefix("excludeFilter:"))
    {
        filters.exclude.push(value.to_string());
        return Ok(true);
    }

    if let Some(value) = s
        .strip_prefix("includeFilter://")
        .or_else(|| s.strip_prefix("includeFilter:"))
    {
        filters.include.push(value.to_string());
        return Ok(true);
    }

    // filter:// protocol format (whistle unified filter syntax)
    // filter:///regex/i → pattern filter (include)
    // filter://m:GET → method filter
    // filter://s:404 → status filter
    // filter://h:content-type=json → header filter
    if let Some(value) = s.strip_prefix("filter://") {
        return parse_filter_value(value, filters, false);
    }

    // IMPORTANT: Skip if this looks like an action (protocol://value format)
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

    // Content type filter
    if let Some(value) = s
        .strip_prefix("ct:")
        .or_else(|| s.strip_prefix("contentType:"))
        .or_else(|| s.strip_prefix("reqContentType:"))
        .or_else(|| s.strip_prefix("resContentType:"))
    {
        filters.content_types = value.split(',').map(|s| s.trim().to_string()).collect();
        return Ok(true);
    }

    // IP filter
    if let Some(value) = s
        .strip_prefix("i:")
        .or_else(|| s.strip_prefix("ip:"))
        .or_else(|| s.strip_prefix("clientIp:"))
    {
        filters.client_ips = value.split(',').map(|s| s.trim().to_string()).collect();
        return Ok(true);
    }

    // Host filter
    if let Some(value) = s
        .strip_prefix("h:")
        .or_else(|| s.strip_prefix("host:"))
        .or_else(|| s.strip_prefix("hostname:"))
    {
        filters.hosts = value.split(',').map(|s| s.trim().to_string()).collect();
        return Ok(true);
    }

    // Status code filter
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

    Ok(false)
}

/// Parse filter:// value content into the appropriate filter field.
/// Handles: m:, s:, h:, /regex/, etc.
fn parse_filter_value(value: &str, filters: &mut RuleFilters, _is_exclude: bool) -> Result<bool> {
    // Method sub-filter: filter://m:GET
    if let Some(v) = value
        .strip_prefix("m:")
        .or_else(|| value.strip_prefix("method:"))
    {
        filters.methods = v.split(',').map(|s| s.trim().to_uppercase()).collect();
        return Ok(true);
    }

    // Status code sub-filter: filter://s:404
    if let Some(v) = value
        .strip_prefix("s:")
        .or_else(|| value.strip_prefix("statusCode:"))
    {
        filters.status_codes = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        return Ok(true);
    }

    // Header sub-filter: filter://h:content-type=json
    if let Some(v) = value
        .strip_prefix("h:")
        .or_else(|| value.strip_prefix("header:"))
    {
        if let Some(eq_idx) = v.find('=') {
            let key = v[..eq_idx].to_string();
            let val = v[eq_idx + 1..].to_string();
            filters.headers.insert(key, val);
        }
        return Ok(true);
    }

    // IP sub-filter: filter://i:127.0.0.1
    if let Some(v) = value
        .strip_prefix("i:")
        .or_else(|| value.strip_prefix("ip:"))
    {
        filters.client_ips = v.split(',').map(|s| s.trim().to_string()).collect();
        return Ok(true);
    }

    // Protocol sub-filter: filter://p:https
    if let Some(v) = value
        .strip_prefix("p:")
        .or_else(|| value.strip_prefix("protocol:"))
    {
        filters.protocols = v.split(',').map(|s| s.trim().to_lowercase()).collect();
        return Ok(true);
    }

    // Port sub-filter: filter://port:443
    if let Some(v) = value.strip_prefix("port:") {
        filters.ports = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        return Ok(true);
    }

    // Regex pattern filter: filter:///regex/i
    // Treated as include filter
    if value.starts_with('/') || value.starts_with('^') || value.contains(".*") {
        filters.include.push(value.to_string());
        return Ok(true);
    }

    // Default: treat as include pattern
    filters.include.push(value.to_string());
    Ok(true)
}

/// Parse a single action token into a RuleAction
fn parse_single_action(s: &str) -> Result<Option<RuleAction>> {
    // IMPORTANT: Check for bare URLs FIRST before protocol://value parsing
    if s.starts_with("http://") || s.starts_with("https://") {
        return Ok(Some(RuleAction::Host {
            target: s.to_string(),
        }));
    }

    // Parse protocol://value format
    if let Some(idx) = s.find("://") {
        let protocol = &s[..idx].to_lowercase();
        let value = &s[idx + 3..];

        let action = match protocol.as_str() {
            // === HOST/REDIRECT ACTIONS ===
            "host" | "hosts" | "xhost" => RuleAction::Host {
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

            "statuscode" | "status" | "replacestatus" => {
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

            "urlparams" | "params" | "reqmerge" => {
                let modifications = parse_url_param_modifications(value)?;
                RuleAction::UrlParams { modifications }
            }

            "pathreplace" | "urlreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                RuleAction::PathReplace {
                    pattern,
                    replacement,
                }
            }

            "method" => RuleAction::Method {
                method: value.to_uppercase(),
            },

            "reqtype" => RuleAction::RequestType {
                content_type: value.to_string(),
            },

            "reqcharset" => RuleAction::RequestCharset {
                charset: value.to_string(),
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
            "resheaders" | "resheader" | "headerreplace" => {
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

            "attachment" | "download" => RuleAction::Attachment {
                filename: if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                },
            },

            // === INJECTION ACTIONS ===
            "htmlappend" | "html" => RuleAction::HtmlAppend {
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

            "rescors" | "cors" => parse_response_cors(value)?,

            // === PERFORMANCE ACTIONS ===
            "reqdelay" | "delay" => {
                let ms: u64 = value
                    .parse()
                    .map_err(|_| PostGateError::RuleParse(format!("Invalid delay: {}", value)))?;
                RuleAction::Delay {
                    request_ms: Some(ms),
                    response_ms: None,
                }
            }

            "resdelay" => {
                let ms: u64 = value
                    .parse()
                    .map_err(|_| PostGateError::RuleParse(format!("Invalid delay: {}", value)))?;
                RuleAction::Delay {
                    request_ms: None,
                    response_ms: Some(ms),
                }
            }

            "reqspeed" => {
                let kbps: u64 = value
                    .parse()
                    .map_err(|_| PostGateError::RuleParse(format!("Invalid speed: {}", value)))?;
                RuleAction::Speed {
                    request_kbps: Some(kbps),
                    response_kbps: None,
                }
            }

            "resspeed" => {
                let kbps: u64 = value
                    .parse()
                    .map_err(|_| PostGateError::RuleParse(format!("Invalid speed: {}", value)))?;
                RuleAction::Speed {
                    request_kbps: None,
                    response_kbps: Some(kbps),
                }
            }

            "speed" => {
                let kbps: u64 = value
                    .parse()
                    .map_err(|_| PostGateError::RuleParse(format!("Invalid speed: {}", value)))?;
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

            "ignore" | "filter" | "skip" => RuleAction::Ignore,

            "enable" => RuleAction::Enable {
                features: parse_feature_list(value),
            },

            "disable" => RuleAction::Disable {
                features: parse_feature_list(value),
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
                let ms: u64 = value
                    .parse()
                    .map_err(|_| PostGateError::RuleParse(format!("Invalid timeout: {}", value)))?;
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
                RuleAction::HtmlReplace {
                    pattern,
                    replacement,
                    regex: is_regex,
                }
            }

            "jsreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                let is_regex = pattern.starts_with('/') || pattern.starts_with('^');
                RuleAction::JsReplace {
                    pattern,
                    replacement,
                    regex: is_regex,
                }
            }

            "cssreplace" => {
                let (pattern, replacement) = parse_replace_pair(value)?;
                let is_regex = pattern.starts_with('/') || pattern.starts_with('^');
                RuleAction::CssReplace {
                    pattern,
                    replacement,
                    regex: is_regex,
                }
            }

            "reqprepend" => RuleAction::RequestPrepend {
                content: value.to_string(),
            },

            "reqappend" => RuleAction::RequestAppend {
                content: value.to_string(),
            },

            "resprepend" | "resPrepend" => RuleAction::ResponsePrepend {
                content: value.to_string(),
            },

            "resappend" => RuleAction::ResponseAppend {
                content: value.to_string(),
            },

            "reqwrite" => RuleAction::RequestWrite {
                path: value.to_string(),
                raw: false,
            },

            "reqwriteraw" => RuleAction::RequestWrite {
                path: value.to_string(),
                raw: true,
            },

            "reswrite" => RuleAction::ResponseWrite {
                path: value.to_string(),
                raw: false,
            },

            "reswriteraw" => RuleAction::ResponseWrite {
                path: value.to_string(),
                raw: true,
            },

            "responsefor" => RuleAction::ResponseFor {
                value: value.to_string(),
            },

            "style" | "rule" | "pipe" | "pac" | "https2http-proxy" | "http2https-proxy"
            | "internal-proxy" | "cache" | "rulesfile" | "rulefile" | "rulescript"
            | "rulescripts" | "reqscript" | "reqrules" | "resscript" | "resrules"
            | "framescript" | "resmerge" | "trailers" | "cipher" | "tlsoptions" | "snicallback"
            | "xproxy" | "xhttp-proxy" => RuleAction::Unsupported {
                protocol: protocol.to_string(),
                value: value.to_string(),
            },

            _ => {
                tracing::warn!("Unsupported or unknown action protocol: {}", protocol);
                RuleAction::Unsupported {
                    protocol: protocol.to_string(),
                    value: value.to_string(),
                }
            }
        };

        return Ok(Some(action));
    }

    // Handle IPv6 bracket format: [::1]:8080
    if s.starts_with('[') {
        if let Some((host, port)) = is_host_value(s) {
            let target = if let Some(p) = port {
                format!("[{}]:{}", host, p)
            } else {
                format!("[{}]", host)
            };
            return Ok(Some(RuleAction::Host { target }));
        }
    }

    // Handle IP:port as host redirect (e.g., 127.0.0.1:8080)
    if s.contains(':') && s.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
        return Ok(Some(RuleAction::Host {
            target: s.to_string(),
        }));
    }

    // Handle hostname:port as host redirect (e.g., localhost:8080)
    if let Some(colon_idx) = s.rfind(':') {
        let host_part = &s[..colon_idx];
        let port_part = &s[colon_idx + 1..];
        if !host_part.is_empty()
            && port_part.chars().all(|c| c.is_ascii_digit())
            && !port_part.is_empty()
            && host_part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Ok(Some(RuleAction::Host {
                target: s.to_string(),
            }));
        }
    }

    Ok(None)
}

// =============================================================================
// Tokenizer
// =============================================================================

/// Split action string respecting quotes and JSON braces
fn split_action_string(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut brace_depth: usize = 0;
    let mut bracket_depth: usize = 0;

    for ch in s.chars() {
        match ch {
            '"' | '\'' if brace_depth == 0 && bracket_depth == 0 => {
                if in_quotes && ch == quote_char {
                    in_quotes = false;
                } else if !in_quotes {
                    in_quotes = true;
                    quote_char = ch;
                }
                current.push(ch);
            }
            '{' if !in_quotes => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' if !in_quotes => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            '[' if !in_quotes => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' if !in_quotes => {
                bracket_depth = bracket_depth.saturating_sub(1);
                current.push(ch);
            }
            ' ' | '\t' if !in_quotes && brace_depth == 0 && bracket_depth == 0 => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

// =============================================================================
// Helper Parsers
// =============================================================================

/// Parse header modifications from JSON or key=value format
fn parse_header_modifications(s: &str) -> Result<HeaderModifications> {
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

    let mut modifications = HeaderModifications::default();

    for pair in s.split(',') {
        let pair = pair.trim();
        if let Some(stripped) = pair.strip_prefix('-') {
            modifications.remove.push(stripped.to_string());
        } else if let Some(rest) = pair.strip_prefix('+') {
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
    if s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("~/")
        || s.starts_with("http://")
        || s.starts_with("https://")
    {
        return Ok(BodyContent::File { path: s.into() });
    }
    if s.starts_with('{') || s.starts_with('[') {
        if let Ok(value) = serde_json::from_str(s) {
            return Ok(BodyContent::Json { value });
        }
    }
    if let Some(stripped) = s.strip_prefix("base64:") {
        return Ok(BodyContent::Base64 {
            data: stripped.to_string(),
        });
    }
    if s.is_empty() || s == "empty" {
        return Ok(BodyContent::Empty);
    }
    Ok(BodyContent::Text {
        content: s.to_string(),
        content_type: "text/plain".to_string(),
    })
}

fn parse_feature_list(value: &str) -> Vec<String> {
    value
        .split([',', '|'])
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(|feature| feature.to_ascii_lowercase())
        .collect()
}

/// Parse URL parameter modifications
fn parse_url_param_modifications(s: &str) -> Result<UrlParamModifications> {
    let mut modifications = UrlParamModifications::default();

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

    for pair in s.split('&') {
        let pair = pair.trim();
        if let Some(stripped) = pair.strip_prefix('-') {
            modifications.remove.push(stripped.to_string());
        } else if let Some(rest) = pair.strip_prefix('+') {
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

    if s.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<HashMap<String, String>>(s) {
            return Ok(json);
        }
    }

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

    if s.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<HashMap<String, CookieOptions>>(s) {
            return Ok(json);
        }
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
    if s == "*" || s.is_empty() {
        return Ok(RuleAction::ResponseCors {
            origin: Some("*".to_string()),
            methods: Some("GET,POST,PUT,DELETE,OPTIONS,PATCH".to_string()),
            headers: Some("*".to_string()),
            credentials: true,
            max_age: Some(86400),
        });
    }

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

// =============================================================================
// Tests
// =============================================================================

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
    fn test_comment_handling() {
        let content = r#"
# This is a comment
example.com host://127.0.0.1
//example.com/api host://localhost:3000
"#;
        let rules = parse_rules(content).unwrap();
        // # comment is skipped, // is NOT a comment (it's a no-schema pattern)
        assert_eq!(rules.len(), 2);
        // Second rule should be a NoSchema pattern
        assert!(matches!(rules[1].pattern, Pattern::NoSchema { .. }));
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
        assert!(matches!(
            rules[0].actions[0],
            RuleAction::ResponseCors { .. }
        ));
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
            assert_eq!(
                modifications.set.get("Content-Type").unwrap(),
                "application/json"
            );
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
    fn test_parse_additional_whistle_protocols() {
        let rules = parse_rules(
            "example.com reqType://application/json reqCharset://utf-8 reqWrite:///tmp/req.bin reqWriteRaw:///tmp/req.raw resWrite:///tmp/res.bin resWriteRaw:///tmp/res.raw responseFor://client",
        )
        .unwrap();
        assert_eq!(rules.len(), 1);

        assert!(matches!(
            rules[0].actions[0],
            RuleAction::RequestType { ref content_type } if content_type == "application/json"
        ));
        assert!(matches!(
            rules[0].actions[1],
            RuleAction::RequestCharset { ref charset } if charset == "utf-8"
        ));
        assert!(matches!(
            rules[0].actions[2],
            RuleAction::RequestWrite { ref path, raw: false } if path == "/tmp/req.bin"
        ));
        assert!(matches!(
            rules[0].actions[3],
            RuleAction::RequestWrite { ref path, raw: true } if path == "/tmp/req.raw"
        ));
        assert!(matches!(
            rules[0].actions[4],
            RuleAction::ResponseWrite { ref path, raw: false } if path == "/tmp/res.bin"
        ));
        assert!(matches!(
            rules[0].actions[5],
            RuleAction::ResponseWrite { ref path, raw: true } if path == "/tmp/res.raw"
        ));
        assert!(matches!(
            rules[0].actions[6],
            RuleAction::ResponseFor { ref value } if value == "client"
        ));
    }

    #[test]
    fn test_parse_enable_disable_feature_lists_with_pipes() {
        let rules =
            parse_rules("example.com enable://forceReqWrite|forceResWrite disable://capture,abort")
                .unwrap();
        assert_eq!(rules.len(), 1);

        assert!(matches!(
            &rules[0].actions[0],
            RuleAction::Enable { features }
                if features == &vec!["forcereqwrite".to_string(), "forcereswrite".to_string()]
        ));
        assert!(matches!(
            &rules[0].actions[1],
            RuleAction::Disable { features }
                if features == &vec!["capture".to_string(), "abort".to_string()]
        ));
    }

    #[test]
    fn test_parse_unsupported_protocols_are_preserved() {
        let rules = parse_rules("example.com style://foo unknownThing://bar").unwrap();
        assert_eq!(rules.len(), 1);

        assert!(matches!(
            rules[0].actions[0],
            RuleAction::Unsupported { ref protocol, ref value }
                if protocol == "style" && value == "foo"
        ));
        assert!(matches!(
            rules[0].actions[1],
            RuleAction::Unsupported { ref protocol, ref value }
                if protocol == "unknownthing" && value == "bar"
        ));
    }

    #[test]
    fn test_url_pattern_with_query_is_stripped() {
        // Regression: URL pattern containing query/hash should have them
        // stripped from the path so matching works correctly.
        let rules = parse_rules("https://example.com/api?q=1 host://127.0.0.1:3000").unwrap();
        assert_eq!(rules.len(), 1);
        if let Pattern::Url {
            protocol,
            host,
            path,
        } = &rules[0].pattern
        {
            assert_eq!(protocol.as_deref(), Some("https"));
            assert_eq!(host, "example.com");
            assert_eq!(path.as_deref(), Some("/api"));
        } else {
            panic!("Expected Url pattern, got {:?}", rules[0].pattern);
        }
    }

    #[test]
    fn test_url_pattern_with_hash_is_stripped() {
        let rules = parse_rules("https://example.com/api#section host://127.0.0.1:3000").unwrap();
        assert_eq!(rules.len(), 1);
        if let Pattern::Url {
            protocol,
            host,
            path,
        } = &rules[0].pattern
        {
            assert_eq!(protocol.as_deref(), Some("https"));
            assert_eq!(host, "example.com");
            assert_eq!(path.as_deref(), Some("/api"));
        } else {
            panic!("Expected Url pattern, got {:?}", rules[0].pattern);
        }
    }

    #[test]
    fn test_no_schema_pattern_with_query_is_stripped() {
        let rules = parse_rules("//example.com/api?q=1 host://127.0.0.1:3000").unwrap();
        assert_eq!(rules.len(), 1);
        if let Pattern::NoSchema { host, path } = &rules[0].pattern {
            assert_eq!(host, "example.com");
            assert_eq!(path.as_deref(), Some("/api"));
        } else {
            panic!("Expected NoSchema pattern, got {:?}", rules[0].pattern);
        }
    }

    #[test]
    fn test_url_pattern_matches_request_with_query() {
        use crate::rules::types::Pattern;
        let pattern = Pattern::Url {
            protocol: Some("https".to_string()),
            host: "example.com".to_string(),
            path: Some("/api".to_string()),
        };
        // Rule path is /api, request has query — should still match
        assert!(pattern.matches("https://example.com/api?q=1"));
        assert!(pattern.matches("https://example.com/api#hash"));
        assert!(pattern.matches("https://example.com/api/users?q=1"));
    }

    #[test]
    fn test_parse_url_to_url_proxy() {
        let rules =
            parse_rules("https://v.qq.com/biu/u/history/ http://127.0.0.1:3000/browser").unwrap();
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

    #[test]
    fn test_include_filter_with_regex_flags() {
        let rules = parse_rules(
            r#"v.qq.com localhost:8080 includeFilter:///https?:\/\/v.qq.com\/(@|packages|common|node_modules|src|x\/(cover|page|skeleton)|__vite_hmr)/i"#
        ).unwrap();
        assert_eq!(rules.len(), 1);
        let rule = &rules[0];

        assert!(matches!(&rule.pattern, Pattern::Domain(d) if d == "v.qq.com"));

        let filters = rule.filters.as_ref().expect("Filters should be present");
        assert_eq!(filters.include.len(), 1);

        let headers = HashMap::new();

        assert!(
            filters.matches(
                "GET",
                "https",
                443,
                &headers,
                "https://v.qq.com/x/cover/mzc00200rgazpwa/h4102454htt.html"
            ),
            "includeFilter should match /x/cover/ URL"
        );
        assert!(
            filters.matches(
                "GET",
                "https",
                443,
                &headers,
                "https://v.qq.com/x/page/something"
            ),
            "includeFilter should match /x/page/ URL"
        );
        assert!(
            !filters.matches(
                "GET",
                "https",
                443,
                &headers,
                "https://v.qq.com/other/stuff.html"
            ),
            "includeFilter should NOT match unrelated path"
        );
    }

    #[test]
    fn test_include_filter_case_insensitive() {
        let rules =
            parse_rules(r#"example.com host://localhost:3000 includeFilter:///api/i"#).unwrap();
        assert_eq!(rules.len(), 1);
        let filters = rules[0]
            .filters
            .as_ref()
            .expect("Filters should be present");

        let headers = HashMap::new();

        assert!(filters.matches(
            "GET",
            "https",
            443,
            &headers,
            "https://example.com/API/users"
        ));
        assert!(filters.matches(
            "GET",
            "https",
            443,
            &headers,
            "https://example.com/api/users"
        ));
    }

    #[test]
    fn test_regex_pattern_with_flags() {
        let rules = parse_rules(r#"/example\.com/i host://localhost:3000"#).unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0].pattern, Pattern::Regex(_)));
    }

    // === New tests for whistle alignment ===

    #[test]
    fn test_exact_pattern_dollar_prefix() {
        let rules = parse_rules("$https://example.com/exact statusCode://404").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0].pattern, Pattern::Exact(s) if s == "https://example.com/exact"));
    }

    #[test]
    fn test_negative_pattern() {
        let rules = parse_rules("!example.com host://other.com").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(rules[0].negated);
        assert!(matches!(&rules[0].pattern, Pattern::Domain(d) if d == "example.com"));
    }

    #[test]
    fn test_port_pattern() {
        let rules = parse_rules(":8080 host://localhost:3000").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0].pattern, Pattern::Port(8080)));
    }

    #[test]
    fn test_no_schema_pattern() {
        let rules = parse_rules("//example.com/api host://localhost").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0].pattern, Pattern::NoSchema { .. }));
    }

    #[test]
    fn test_dot_suffix_pattern() {
        let rules = parse_rules(".json file:///mock.json").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0].pattern, Pattern::Regex(_)));
        // Should match URLs ending in .json
        assert!(rules[0].pattern.matches("https://api.com/data.json"));
        assert!(!rules[0].pattern.matches("https://api.com/data.jsonp"));
    }

    #[test]
    fn test_body_remote_http_resource_parses_as_file_content() {
        let rules = parse_rules("example.com resBody://https://assets.example/mock.json").unwrap();
        assert_eq!(rules.len(), 1);
        match &rules[0].actions[0] {
            RuleAction::ResponseBody {
                content: BodyContent::File { path },
            } => assert_eq!(path.to_string_lossy(), "https://assets.example/mock.json"),
            other => panic!("Expected remote resBody file content, got {:?}", other),
        }
    }

    #[test]
    fn test_reverse_syntax() {
        let rules = parse_rules("host://127.0.0.1:8080 example.com").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0].pattern, Pattern::Domain(d) if d == "example.com"));
        if let RuleAction::Host { target } = &rules[0].actions[0] {
            assert_eq!(target, "127.0.0.1:8080");
        } else {
            panic!("Expected Host action");
        }
    }

    #[test]
    fn test_reverse_syntax_multiple_patterns() {
        let rules = parse_rules("host://127.0.0.1:8080 example.com test.com").unwrap();
        assert_eq!(rules.len(), 2);
        assert!(matches!(&rules[0].pattern, Pattern::Domain(d) if d == "example.com"));
        assert!(matches!(&rules[1].pattern, Pattern::Domain(d) if d == "test.com"));
    }

    #[test]
    fn test_reverse_syntax_known_whistle_protocols_are_not_patterns() {
        let rules = parse_rules(
            "hosts://127.0.0.1 xhost://127.0.0.2 reqMerge://debug=true headerReplace://x-a=b example.com",
        )
        .unwrap();
        assert_eq!(rules.len(), 4);
        assert!(rules
            .iter()
            .all(|rule| matches!(&rule.pattern, Pattern::Domain(d) if d == "example.com")));
        assert!(matches!(rules[0].actions[0], RuleAction::Host { .. }));
        assert!(matches!(rules[1].actions[0], RuleAction::Host { .. }));
        assert!(matches!(rules[2].actions[0], RuleAction::UrlParams { .. }));
        assert!(matches!(
            rules[3].actions[0],
            RuleAction::ResponseHeaders { .. }
        ));
    }

    #[test]
    fn test_filter_protocol_format() {
        let rules = parse_rules("example.com filter://m:GET host://localhost").unwrap();
        assert_eq!(rules.len(), 1);
        let filters = rules[0]
            .filters
            .as_ref()
            .expect("Filters should be present");
        assert_eq!(filters.methods, vec!["GET"]);
    }

    #[test]
    fn test_filter_regex_protocol_format() {
        let rules = parse_rules("example.com filter:///api/i host://localhost").unwrap();
        assert_eq!(rules.len(), 1);
        let filters = rules[0]
            .filters
            .as_ref()
            .expect("Filters should be present");
        assert_eq!(filters.include.len(), 1);
    }

    #[test]
    fn test_protocol_alias_skip() {
        let rules = parse_rules("example.com skip://").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].actions[0], RuleAction::Ignore));
    }

    #[test]
    fn test_protocol_alias_download() {
        let rules = parse_rules("example.com/file.pdf download://report.pdf").unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::Attachment { filename } = &rules[0].actions[0] {
            assert_eq!(filename.as_deref(), Some("report.pdf"));
        } else {
            panic!("Expected Attachment action");
        }
    }

    #[test]
    fn test_protocol_alias_urlreplace() {
        let rules = parse_rules("example.com urlReplace:///old->new").unwrap();
        assert_eq!(rules.len(), 1);
        assert!(matches!(
            rules[0].actions[0],
            RuleAction::PathReplace { .. }
        ));
    }

    #[test]
    fn test_ipv6_host() {
        let rules = parse_rules("example.com [::1]:8080").unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::Host { target } = &rules[0].actions[0] {
            assert_eq!(target, "[::1]:8080");
        } else {
            panic!("Expected Host action, got {:?}", rules[0].actions[0]);
        }
    }

    #[test]
    fn test_localhost_host_action() {
        let rules = parse_rules("v.qq.com localhost:8080").unwrap();
        assert_eq!(rules.len(), 1);
        if let RuleAction::Host { target } = &rules[0].actions[0] {
            assert_eq!(target, "localhost:8080");
        } else {
            panic!("Expected Host action");
        }
    }

    #[test]
    fn test_bare_host_path_parsed_as_no_schema() {
        // Patterns like vm.gtimg.cn/path should be NoSchema, not Exact
        let rules = parse_rules(
            "vm.gtimg.cn/thumbplayer/core/1.63.2/txhlsjs-kernel.js https://vm.gtimg.cn/thumbplayer/canary/vsite-ssr/core/1.63.2-next.2/txhlsjs-kernel.js"
        ).unwrap();
        assert_eq!(rules.len(), 1);
        if let Pattern::NoSchema { host, path } = &rules[0].pattern {
            assert_eq!(host, "vm.gtimg.cn");
            assert_eq!(
                path.as_deref(),
                Some("/thumbplayer/core/1.63.2/txhlsjs-kernel.js")
            );
        } else {
            panic!("Expected NoSchema pattern, got {:?}", rules[0].pattern);
        }
        // The action should be Host with the full target URL
        if let RuleAction::Host { target } = &rules[0].actions[0] {
            assert_eq!(
                target,
                "https://vm.gtimg.cn/thumbplayer/canary/vsite-ssr/core/1.63.2-next.2/txhlsjs-kernel.js"
            );
        } else {
            panic!("Expected Host action");
        }
    }

    #[test]
    fn test_bare_host_path_matches_any_protocol() {
        // vm.gtimg.cn/path should match both http:// and https:// URLs
        let rules = parse_rules(
            "vm.gtimg.cn/thumbplayer/core/1.63.2/txhlsjs-kernel.js https://vm.gtimg.cn/thumbplayer/canary/vsite-ssr/core/1.63.2-next.2/txhlsjs-kernel.js"
        ).unwrap();
        let pattern = &rules[0].pattern;
        assert!(
            pattern.matches("https://vm.gtimg.cn/thumbplayer/core/1.63.2/txhlsjs-kernel.js"),
            "Should match https URL"
        );
        assert!(
            pattern.matches("http://vm.gtimg.cn/thumbplayer/core/1.63.2/txhlsjs-kernel.js"),
            "Should match http URL"
        );
        assert!(
            !pattern.matches("https://other.com/thumbplayer/core/1.63.2/txhlsjs-kernel.js"),
            "Should not match different host"
        );
    }

    #[test]
    fn test_bare_host_path_no_schema_with_subpath() {
        // example.com/api should match example.com/api/users too
        let rules = parse_rules("example.com/api host://localhost:3000").unwrap();
        assert_eq!(rules.len(), 1);
        if let Pattern::NoSchema { host, path } = &rules[0].pattern {
            assert_eq!(host, "example.com");
            assert_eq!(path.as_deref(), Some("/api"));
        } else {
            panic!("Expected NoSchema pattern, got {:?}", rules[0].pattern);
        }
        let pattern = &rules[0].pattern;
        assert!(pattern.matches("https://example.com/api/users"));
        assert!(pattern.matches("http://example.com/api"));
        assert!(!pattern.matches("https://example.com/other"));
    }
}
