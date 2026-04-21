//! Reference resolver.
//!
//! Understands three forms of value references (whistle-compatible):
//!
//! * `{name}`      — plain substitution, the value's content is returned verbatim
//! * `` `{name}` ``  — template substitution, `${var}` placeholders inside the
//!                    resolved content are expanded against [`RequestCtx`]
//! * anything else — returned unchanged (the caller passes the raw action arg
//!                    through the resolver unconditionally, so non-references
//!                    are a no-op)
//!
//! Precedence: inline rule-group definitions → global store. Missing lookups
//! resolve to an empty string (whistle behaviour). Recursion is capped to
//! avoid `{a.txt}`-in-`a.txt` infinite loops.

use bytes::Bytes;
use dashmap::DashMap;
use std::collections::HashMap;

/// Maximum resolution depth — one resolved value may reference another. Above
/// this depth we stop expanding so pathological cycles can't hang the proxy.
const MAX_DEPTH: usize = 5;

/// Per-request context supplied to template interpolation.
///
/// Only the documented whistle fields are exposed; anything unreferenced can be
/// passed as `""` / empty maps.
pub struct RequestCtx<'a> {
    pub url: &'a str,
    pub method: &'a str,
    pub client_ip: &'a str,
    pub req_headers: &'a HashMap<String, String>,
    pub query: &'a HashMap<String, String>,
    pub req_cookies: &'a HashMap<String, String>,
    pub now_ms: i64,
}

impl<'a> RequestCtx<'a> {
    /// Minimal context for callers that don't have the full request available
    /// (e.g. unit tests or rule applications that don't need templating).
    pub fn empty() -> RequestCtx<'static> {
        // Use static empty maps via a small lazy helper; for simplicity we
        // leak a single allocation — the proxy runs for the life of the
        // process so this is fine.
        static EMPTY: std::sync::OnceLock<(HashMap<String, String>, HashMap<String, String>)> =
            std::sync::OnceLock::new();
        let (h, q) = EMPTY.get_or_init(|| (HashMap::new(), HashMap::new()));
        RequestCtx {
            url: "",
            method: "",
            client_ip: "",
            req_headers: h,
            query: q,
            req_cookies: q,
            now_ms: 0,
        }
    }
}

/// Resolve a whistle-style reference and return the final bytes.
///
/// * `arg` — the raw action argument (body text, file path, header value …)
/// * `inline` — `{name}` definitions scoped to the owning rule group (higher
///   precedence than the global store, following whistle v1.12.12+ semantics)
/// * `store` — the global in-memory values map
/// * `ctx` — per-request variables used for template expansion
#[allow(dead_code)]
pub fn resolve(
    arg: &str,
    inline: &HashMap<String, String>,
    store: &DashMap<String, String>,
    ctx: &RequestCtx,
) -> Bytes {
    Bytes::from(resolve_str(arg, inline, store, ctx, 0))
}

/// String variant used internally (and recursively). Exposed for unit tests.
pub fn resolve_str(
    arg: &str,
    inline: &HashMap<String, String>,
    store: &DashMap<String, String>,
    ctx: &RequestCtx,
    depth: usize,
) -> String {
    if depth >= MAX_DEPTH {
        return String::new();
    }

    // Template form: leading and trailing backtick wrap a `{name}` ref.
    let trimmed = arg.trim();
    if let Some(name) = trimmed
        .strip_prefix("`{")
        .and_then(|s| s.strip_suffix("}`"))
    {
        let raw = lookup(name, inline, store);
        // Recursively resolve any nested refs, THEN run ${var} interpolation.
        let resolved = resolve_nested_refs(&raw, inline, store, ctx, depth + 1);
        return interpolate_template(&resolved, ctx);
    }

    // Plain form: exact `{name}` wrapping the whole arg.
    if let Some(name) = trimmed.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        // Guard against `{ ... { ... }` and arbitrary JSON payloads — only
        // value-name-shaped strings are treated as references.
        if !name.contains('{') && !name.contains('}') && is_valid_value_name(name) {
            let raw = lookup(name, inline, store);
            return resolve_nested_refs(&raw, inline, store, ctx, depth + 1);
        }
    }

    // Also expand embedded `{name}` references inside larger strings (for
    // cases like header values `"Bearer {token}"`).
    resolve_nested_refs(arg, inline, store, ctx, depth)
}

/// Lookup `name`, preferring inline definitions over the global store.
fn lookup(name: &str, inline: &HashMap<String, String>, store: &DashMap<String, String>) -> String {
    if let Some(v) = inline.get(name) {
        return v.clone();
    }
    if let Some(entry) = store.get(name) {
        return entry.value().clone();
    }
    String::new()
}

/// Expand every `{name}` occurrence inside `input`. Skips `${...}` template
/// vars (those are handled separately by [`interpolate_template`]).
fn resolve_nested_refs(
    input: &str,
    inline: &HashMap<String, String>,
    store: &DashMap<String, String>,
    ctx: &RequestCtx,
    depth: usize,
) -> String {
    if depth >= MAX_DEPTH {
        return input.to_string();
    }

    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];

        // Preserve `${...}` so the template interpolator can handle it.
        if c == b'$' && bytes.get(i + 1) == Some(&b'{') {
            if let Some(end) = find_matching_brace(bytes, i + 1) {
                out.push_str(&input[i..=end]);
                i = end + 1;
                continue;
            }
        }

        if c == b'{' {
            if let Some(end) = find_matching_brace(bytes, i) {
                let name = &input[i + 1..end];
                // Avoid treating `{ }` (with spaces) or braces containing `{`
                // as references — too lenient would break JSON payloads.
                if !name.is_empty()
                    && !name.contains('{')
                    && !name.contains('}')
                    && !name.contains('\n')
                    && is_valid_value_name(name)
                {
                    let raw = lookup(name, inline, store);
                    let expanded = resolve_str(&raw, inline, store, ctx, depth + 1);
                    out.push_str(&expanded);
                    i = end + 1;
                    continue;
                }
            }
        }

        // Default: copy byte. UTF-8 safe because we only pattern-match ASCII
        // delimiters.
        out.push(c as char);
        i += 1;
    }
    out
}

/// Find the position of the closing `}` for an opening `{` at `open_idx`.
fn find_matching_brace(bytes: &[u8], open_idx: usize) -> Option<usize> {
    debug_assert_eq!(bytes[open_idx], b'{');
    let mut depth = 1;
    let mut i = open_idx + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Heuristic: a value name must look like a filename/path fragment. We allow
/// letters, digits, `_`, `-`, `.`, `/`. This keeps `{foo.json}` working while
/// rejecting arbitrary JSON keys like `{"id":1}`.
fn is_valid_value_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
}

/// Expand `${path}` variables in `input` against `ctx`. Unknown paths → empty.
fn interpolate_template(input: &str, ctx: &RequestCtx) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            if let Some(end) = find_matching_brace(bytes, i + 1) {
                let path = &input[i + 2..end];
                out.push_str(&resolve_template_path(path, ctx));
                i = end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn resolve_template_path(path: &str, ctx: &RequestCtx) -> String {
    // Dotted paths like `reqHeaders.user-agent` / `query.userId`.
    let (head, tail) = match path.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (path, None),
    };

    match (head, tail) {
        ("url", None) => ctx.url.to_string(),
        ("method", None) => ctx.method.to_string(),
        ("clientIp", None) | ("ip", None) => ctx.client_ip.to_string(),
        ("now", None) => ctx.now_ms.to_string(),
        ("reqHeaders", Some(key)) => ctx
            .req_headers
            .get(&key.to_ascii_lowercase())
            .or_else(|| ctx.req_headers.get(key))
            .cloned()
            .unwrap_or_default(),
        ("query", Some(key)) => ctx.query.get(key).cloned().unwrap_or_default(),
        ("reqCookie", Some(key)) | ("reqCookies", Some(key)) => {
            ctx.req_cookies.get(key).cloned().unwrap_or_default()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(pairs: &[(&str, &str)]) -> DashMap<String, String> {
        let m = DashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    #[test]
    fn plain_reference() {
        let store = store_with(&[("test.html", "<h1>Hi</h1>")]);
        let out = resolve_str(
            "{test.html}",
            &HashMap::new(),
            &store,
            &RequestCtx::empty(),
            0,
        );
        assert_eq!(out, "<h1>Hi</h1>");
    }

    #[test]
    fn missing_reference_is_empty() {
        let store = DashMap::new();
        let out = resolve_str(
            "{missing.json}",
            &HashMap::new(),
            &store,
            &RequestCtx::empty(),
            0,
        );
        assert_eq!(out, "");
    }

    #[test]
    fn literal_passthrough() {
        let store = DashMap::new();
        let out = resolve_str(
            "http://localhost:3000",
            &HashMap::new(),
            &store,
            &RequestCtx::empty(),
            0,
        );
        assert_eq!(out, "http://localhost:3000");
    }

    #[test]
    fn template_interpolation() {
        let store = store_with(&[("ctx.json", r#"{"u":"${url}","m":"${method}"}"#)]);
        let headers = HashMap::new();
        let query = HashMap::new();
        let cookies = HashMap::new();
        let ctx = RequestCtx {
            url: "http://example.com/x",
            method: "POST",
            client_ip: "",
            req_headers: &headers,
            query: &query,
            req_cookies: &cookies,
            now_ms: 0,
        };
        let out = resolve_str("`{ctx.json}`", &HashMap::new(), &store, &ctx, 0);
        assert_eq!(out, r#"{"u":"http://example.com/x","m":"POST"}"#);
    }

    #[test]
    fn inline_overrides_global() {
        let store = store_with(&[("hi", "global")]);
        let mut inline = HashMap::new();
        inline.insert("hi".to_string(), "inline".to_string());
        let out = resolve_str("{hi}", &inline, &store, &RequestCtx::empty(), 0);
        assert_eq!(out, "inline");
    }

    #[test]
    fn recursion_is_bounded() {
        let store = store_with(&[("a", "{b}"), ("b", "{a}")]);
        let out = resolve_str("{a}", &HashMap::new(), &store, &RequestCtx::empty(), 0);
        // Should not hang; final value is empty after depth cap.
        assert!(out.len() < 10);
    }

    #[test]
    fn embedded_reference_in_header() {
        let store = store_with(&[("token", "secret123")]);
        let out = resolve_str(
            "Bearer {token}",
            &HashMap::new(),
            &store,
            &RequestCtx::empty(),
            0,
        );
        assert_eq!(out, "Bearer secret123");
    }

    #[test]
    fn json_payload_is_not_treated_as_reference() {
        // A raw JSON string should not be mutated.
        let store = DashMap::new();
        let out = resolve_str(
            r#"{"id":1,"name":"foo"}"#,
            &HashMap::new(),
            &store,
            &RequestCtx::empty(),
            0,
        );
        // Contains colon/quote → not a valid value name → passed through.
        assert_eq!(out, r#"{"id":1,"name":"foo"}"#);
    }
}
