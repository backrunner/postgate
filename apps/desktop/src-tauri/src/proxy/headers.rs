//! Multi-value-preserving header helpers.
//!
//! The rest of the proxy pipeline happily flattens `HeaderMap` into
//! `HashMap<String, String>` because rule matching / template resolution /
//! UI display only care about a single string per name. That's fine for
//! inspection — but if we then rebuild the *forwarded* request (or the
//! response sent back to the client) from that flat map we silently collapse
//! every multi-value header into one entry.
//!
//! In practice this destroys:
//! * HTTP/2 `cookie` headers — browsers split cookies into one frame per
//!   crumble per RFC 7540 §8.1.2.5. A `HashMap` keeps only the last one, so
//!   the upstream sees ~1/N of the cookie jar and treats the user as logged
//!   out.
//! * `Set-Cookie` from the upstream — servers very commonly emit several
//!   `Set-Cookie` headers in one response; collapsing them drops all but
//!   one, which is why users reported cookies "vanishing" when browsing
//!   through the proxy.
//! * Any other legitimately-repeated header (`Via`, `Warning`, `Link`,
//!   `WWW-Authenticate`, custom CORS echoes, etc.).
//!
//! The fix here is narrow: keep the flat `HashMap` path for rule plumbing,
//! but rebuild the outgoing `HeaderMap` starting from the original upstream
//! `HeaderMap` and only overlaying what the rules actually touched. Any name
//! the rules did not mention passes through with its full multi-value
//! fidelity intact.

use crate::rules::ResponseModification;
use bytes::Bytes;
use hyper::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::{HashMap, HashSet};

/// Flatten a `HeaderMap` into the `HashMap<String, String>` the rest of the
/// pipeline expects. This is lossy by design — it's consumed by rule
/// matching, template resolution, UI payloads and persistence, where a
/// single representative value is enough.
///
/// Multi-value folding:
/// * `cookie` joins with `"; "` to produce a syntactically valid RFC 6265
///   cookie string (browsers over HTTP/2 send one crumble per header).
/// * `set-cookie` keeps **only the first value** because joining set-cookie
///   with any separator is lossy and callers that round-trip this flat map
///   are the ones that already break multi-Set-Cookie anyway. The *real*
///   forwarding path uses the original `HeaderMap` and doesn't read this.
/// * Everything else joins with `", "` (the general list-form per RFC 7230).
pub fn headermap_to_flat(headers: &HeaderMap) -> HashMap<String, String> {
    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_ascii_lowercase();
        let v = value.to_str().unwrap_or("").to_string();
        grouped.entry(key).or_default().push(v);
    }

    let mut out = HashMap::with_capacity(grouped.len());
    for (k, mut values) in grouped {
        let joined = if values.len() == 1 {
            values.pop().unwrap()
        } else {
            match k.as_str() {
                "cookie" => values.join("; "),
                // Set-Cookie does not combine. Keep the first; real forwarding
                // reads the HeaderMap directly.
                "set-cookie" => values.into_iter().next().unwrap_or_default(),
                _ => values.join(", "),
            }
        };
        out.insert(k, joined);
    }
    out
}

/// Hop-by-hop request headers we never forward upstream.
///
/// The historical list (`Connection`, `Keep-Alive`, `Proxy-Connection`,
/// `Transfer-Encoding`, `Upgrade`, `Host`) lives in
/// [`crate::proxy::upstream::build_upstream_request`]; we keep it in one
/// place here so h1 / h2 / proxy-chain paths agree. `Host` is dropped
/// because hyper derives it from the absolute URI.
fn is_hop_by_hop_request_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-connection"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "te"
            | "trailer"
            | "proxy-authenticate"
            | "proxy-authorization"
    )
}

/// Build the outgoing request `HeaderMap` for the upstream call.
///
/// Starts from the *original* client `HeaderMap` so every multi-value header
/// (cookies over h2, repeated `Via`, etc.) is preserved byte-for-byte. We
/// then layer the rule modifications on top:
///
/// 1. Drop `headers_to_remove` entries.
/// 2. For every key in `modified_flat` whose value differs from
///    `original_flat` *or* whose key isn't present in `original_flat`,
///    replace all existing values with the modified single value. This
///    matches whistle's semantics where `reqHeaders://` / `reqCookies://` /
///    `auth://` overwrite the header.
/// 3. Strip hop-by-hop headers that would confuse pooled h1/h2 connections.
///
/// Keys the rules never touched pass through untouched.
pub fn build_forward_request_headers(
    original: &HeaderMap,
    original_flat: &HashMap<String, String>,
    modified_flat: &HashMap<String, String>,
    headers_to_remove: &[String],
) -> HeaderMap {
    let mut out = HeaderMap::with_capacity(original.len());

    // 1. Start from the original multi-value map, minus removed / hop-by-hop.
    let removed: HashSet<String> = headers_to_remove
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();

    for (name, value) in original.iter() {
        let key_lower = name.as_str().to_ascii_lowercase();
        if removed.contains(&key_lower) {
            continue;
        }
        if is_hop_by_hop_request_header(&key_lower) {
            continue;
        }
        // If the rules explicitly modified this header we'll overwrite it
        // below; skip here so the multi-value doesn't leak in as leftovers.
        if modified_flat
            .get(&key_lower)
            .map(|v| v != original_flat.get(&key_lower).unwrap_or(&String::new()))
            .unwrap_or(false)
        {
            continue;
        }
        if let Ok(hn) = HeaderName::from_bytes(name.as_str().as_bytes()) {
            out.append(hn, value.clone());
        }
    }

    // 2. Overlay rule-modified headers. A header is "modified" if:
    //    * its key isn't in original_flat (new header added by rule), OR
    //    * its value differs from original_flat's value (rule changed it).
    for (k, v) in modified_flat {
        let k_lower = k.to_ascii_lowercase();
        if removed.contains(&k_lower) {
            continue;
        }
        if is_hop_by_hop_request_header(&k_lower) {
            continue;
        }
        if k_lower.starts_with(':') {
            // Pseudo-headers like :method land here after h2 normalization;
            // never forward them as real HTTP headers.
            continue;
        }
        let is_new = !original_flat.contains_key(&k_lower);
        let is_changed = original_flat
            .get(&k_lower)
            .map(|orig| orig != v)
            .unwrap_or(false);
        if !is_new && !is_changed {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k_lower.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            // `insert` semantics: replace all existing values for this name.
            out.insert(name, value);
        }
    }

    out
}

/// Keep request entity headers consistent with the bytes we will actually
/// forward upstream. Body rewrite rules run before the forwarding `HeaderMap`
/// is built, so stale `Content-Length` / `Content-Encoding` from the client
/// must be fixed here.
pub fn sync_request_body_headers(
    headers: &mut HashMap<String, String>,
    original_body: &Bytes,
    final_body: &Bytes,
) {
    let body_changed = original_body != final_body;
    if body_changed {
        headers.remove("content-encoding");
        headers.remove("transfer-encoding");
    }

    if body_changed || headers.contains_key("content-length") || !final_body.is_empty() {
        headers.insert("content-length".to_string(), final_body.len().to_string());
    }
}

/// Build the outgoing response `HeaderMap` sent back to the client.
///
/// Mirrors [`build_forward_request_headers`] but for the response path and
/// with two extra responsibilities:
///
/// * `resCookies://` — append each generated `Set-Cookie` as its **own**
///   header entry (multi-value), never concatenated.
/// * Body-was-replaced hygiene — when the buffering path rewrites the body
///   we strip stale `Content-Encoding` / `Transfer-Encoding` and set a fresh
///   `Content-Length`, otherwise browsers trip on length/encoding mismatch.
///
/// `new_body_len` is `Some(n)` in the buffering path and `None` when
/// streaming through the upstream body unmodified.
pub fn build_forward_response_headers(
    original: &HeaderMap,
    original_flat: &HashMap<String, String>,
    modification: &ResponseModification,
    new_body_len: Option<usize>,
) -> HeaderMap {
    let mut out = HeaderMap::with_capacity(original.len() + modification.cookies.len());

    let removed: HashSet<String> = modification
        .headers_to_remove
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();

    // 1. Passthrough untouched headers from the upstream response with full
    //    multi-value fidelity (critical for Set-Cookie).
    for (name, value) in original.iter() {
        let key_lower = name.as_str().to_ascii_lowercase();
        if removed.contains(&key_lower) {
            continue;
        }
        // Skip keys we're about to overwrite below so the leftover
        // upstream values don't linger.
        let orig_val = original_flat.get(&key_lower);
        let mod_val = modification.headers.get(&key_lower);
        let will_overwrite = match (orig_val, mod_val) {
            (Some(o), Some(m)) if o != m => true,
            (None, Some(_)) => true,
            _ => false,
        };
        if will_overwrite {
            continue;
        }
        if let Ok(hn) = HeaderName::from_bytes(name.as_str().as_bytes()) {
            out.append(hn, value.clone());
        }
    }

    // 2. Overlay rule-modified response headers (status-code / cookies are
    //    handled separately). These are the keys the applicator actually
    //    touched — set(), append(), or remove()d earlier.
    for (k, v) in &modification.headers {
        let k_lower = k.to_ascii_lowercase();
        if removed.contains(&k_lower) {
            continue;
        }
        let is_new = !original_flat.contains_key(&k_lower);
        let is_changed = original_flat
            .get(&k_lower)
            .map(|orig| orig != v)
            .unwrap_or(false);
        if !is_new && !is_changed {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k_lower.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            out.insert(name, value);
        }
    }

    // 3. Append `resCookies://` Set-Cookie entries as their own header lines
    //    so the client sees N distinct `Set-Cookie` headers, not one joined
    //    string. Existing Set-Cookie values from upstream are already in
    //    `out` via step 1 (or, if the modification map also touched
    //    "set-cookie", via step 2 — which, combined with the multi-value
    //    upstream pass-through, is the one case that can't perfectly
    //    round-trip through a flat map). The appended cookies keep their
    //    own distinct lines regardless.
    if !modification.cookies.is_empty() {
        let name = HeaderName::from_static("set-cookie");
        for cookie in &modification.cookies {
            if let Ok(value) = HeaderValue::from_str(cookie) {
                out.append(name.clone(), value);
            }
        }
    }

    // 4. Body-was-replaced hygiene: if the upstream declared a specific
    //    length/encoding but we replaced the body, those headers are now
    //    lying to the client.
    if let Some(len) = new_body_len {
        let upstream_len = original_flat
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok());
        if upstream_len != Some(len) {
            out.remove("content-encoding");
            out.remove("transfer-encoding");
            out.remove("content-length");
            if let Ok(val) = HeaderValue::from_str(&len.to_string()) {
                out.insert(HeaderName::from_static("content-length"), val);
            }
        }
    }

    out
}

/// Convert a flat `HashMap<String, String>` into a `HeaderMap`, used by the
/// short-circuit / plugin / error paths where the response was authored as a
/// flat map to begin with. This is intentionally lossy for multi-value
/// headers because those paths produce a single string per name.
pub fn flat_to_headermap(flat: &HashMap<String, String>) -> HeaderMap {
    let mut out = HeaderMap::with_capacity(flat.len());
    for (k, v) in flat {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.to_ascii_lowercase().as_bytes()),
            HeaderValue::from_str(v),
        ) {
            out.insert(name, value);
        }
    }
    out
}

/// Apply a `HeaderMap` to a `hyper::http::response::Builder`, short-circuit
/// callers use this via a builder extension. The builder has an internal
/// `HeaderMap` we can mutate directly.
pub fn apply_headers_to_response_builder(
    builder: hyper::http::response::Builder,
    headers: &HeaderMap,
) -> hyper::http::response::Builder {
    let mut builder = builder;
    if let Some(target) = builder.headers_mut() {
        for (name, value) in headers.iter() {
            target.append(name.clone(), value.clone());
        }
    }
    builder
}

/// HTTP/2-safe version of [`apply_headers_to_response_builder`]: drops
/// h2-forbidden hop-by-hop headers and sanitizes the `TE` header, since h2
/// will reset the stream if it sees any of these.
pub fn apply_headers_to_response_builder_h2(
    builder: hyper::http::response::Builder,
    headers: &HeaderMap,
) -> hyper::http::response::Builder {
    let mut builder = builder;
    if let Some(target) = builder.headers_mut() {
        for (name, value) in headers.iter() {
            let key = name.as_str().to_ascii_lowercase();
            if matches!(
                key.as_str(),
                "connection"
                    | "keep-alive"
                    | "proxy-connection"
                    | "transfer-encoding"
                    | "upgrade"
                    | "host"
            ) {
                continue;
            }
            if key == "te" {
                let ok = value
                    .to_str()
                    .map(|s| s.eq_ignore_ascii_case("trailers"))
                    .unwrap_or(false);
                if !ok {
                    continue;
                }
            }
            target.append(name.clone(), value.clone());
        }
    }
    builder
}

/// A no-op helper referenced in tests; keeps `Bytes` import live so cargo
/// doesn't complain when the module grows conditional usage.
#[allow(dead_code)]
fn _touch_bytes() -> Bytes {
    Bytes::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::{HeaderMap, HeaderName, HeaderValue};

    fn h(name: &str, value: &str) -> (HeaderName, HeaderValue) {
        (
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        )
    }

    #[test]
    fn flat_joins_cookies_with_semicolon() {
        // Browsers over h2 send one cookie header per crumble.
        let mut map = HeaderMap::new();
        let (n1, v1) = h("cookie", "session=abc");
        let (n2, v2) = h("cookie", "theme=dark");
        map.append(n1, v1);
        map.append(n2, v2);

        let flat = headermap_to_flat(&map);
        assert_eq!(
            flat.get("cookie"),
            Some(&"session=abc; theme=dark".to_string())
        );
    }

    #[test]
    fn flat_preserves_first_set_cookie_only() {
        // Set-Cookie cannot be joined safely; the forward path uses the
        // HeaderMap directly, not this flat form.
        let mut map = HeaderMap::new();
        let (n1, v1) = h("set-cookie", "a=1; Path=/");
        let (n2, v2) = h("set-cookie", "b=2; Path=/");
        map.append(n1, v1);
        map.append(n2, v2);

        let flat = headermap_to_flat(&map);
        assert_eq!(flat.get("set-cookie"), Some(&"a=1; Path=/".to_string()));
    }

    #[test]
    fn forward_request_preserves_multi_value_cookie_when_untouched() {
        // No rule modifies cookies → both original values must survive.
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("session=abc"),
        );
        orig.append(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("theme=dark"),
        );
        orig.append(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("UA/1"),
        );

        let orig_flat = headermap_to_flat(&orig);
        // Modification is identical — no rule touched anything.
        let modified = orig_flat.clone();

        let out = build_forward_request_headers(&orig, &orig_flat, &modified, &[]);
        let cookie_vals: Vec<&str> = out
            .get_all("cookie")
            .iter()
            .map(|v| v.to_str().unwrap_or(""))
            .collect();
        assert_eq!(cookie_vals, vec!["session=abc", "theme=dark"]);
    }

    #[test]
    fn forward_request_overwrites_when_rule_changed_header() {
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("session=abc"),
        );
        orig.append(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("theme=dark"),
        );
        orig.append(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("UA/1"),
        );

        let orig_flat = headermap_to_flat(&orig);
        let mut modified = orig_flat.clone();
        // Rule replaced user-agent.
        modified.insert("user-agent".into(), "Postgate/test".into());

        let out = build_forward_request_headers(&orig, &orig_flat, &modified, &[]);
        assert_eq!(
            out.get("user-agent").and_then(|v| v.to_str().ok()),
            Some("Postgate/test")
        );
        // Cookies still intact.
        let cookie_count = out.get_all("cookie").iter().count();
        assert_eq!(cookie_count, 2);
    }

    #[test]
    fn forward_request_removes_explicit_headers() {
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer x"),
        );
        orig.append(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("session=abc"),
        );

        let orig_flat = headermap_to_flat(&orig);
        let modified = orig_flat.clone();
        let removed = vec!["authorization".to_string()];

        let out = build_forward_request_headers(&orig, &orig_flat, &modified, &removed);
        assert!(out.get("authorization").is_none());
        assert!(out.get("cookie").is_some());
    }

    #[test]
    fn forward_request_strips_hop_by_hop() {
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("connection"),
            HeaderValue::from_static("keep-alive"),
        );
        orig.append(
            HeaderName::from_static("keep-alive"),
            HeaderValue::from_static("timeout=5"),
        );
        orig.append(
            HeaderName::from_static("host"),
            HeaderValue::from_static("example.com"),
        );
        orig.append(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("*/*"),
        );

        let orig_flat = headermap_to_flat(&orig);
        let modified = orig_flat.clone();

        let out = build_forward_request_headers(&orig, &orig_flat, &modified, &[]);
        assert!(out.get("connection").is_none());
        assert!(out.get("keep-alive").is_none());
        assert!(out.get("host").is_none());
        assert_eq!(out.get("accept").and_then(|v| v.to_str().ok()), Some("*/*"));
    }

    #[test]
    fn forward_response_preserves_multiple_set_cookie() {
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("set-cookie"),
            HeaderValue::from_static("a=1; Path=/"),
        );
        orig.append(
            HeaderName::from_static("set-cookie"),
            HeaderValue::from_static("b=2; Path=/"),
        );
        orig.append(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("text/html"),
        );

        let orig_flat = headermap_to_flat(&orig);
        let modification = ResponseModification {
            headers: orig_flat.clone(),
            ..Default::default()
        };

        let out = build_forward_response_headers(&orig, &orig_flat, &modification, None);
        let set_cookies: Vec<&str> = out
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap_or(""))
            .collect();
        assert_eq!(set_cookies, vec!["a=1; Path=/", "b=2; Path=/"]);
    }

    #[test]
    fn forward_response_appends_rescookies_as_separate_headers() {
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("set-cookie"),
            HeaderValue::from_static("a=1"),
        );

        let orig_flat = headermap_to_flat(&orig);
        let modification = ResponseModification {
            headers: orig_flat.clone(),
            cookies: vec!["b=2; Path=/".into(), "c=3; Secure".into()],
            ..Default::default()
        };

        let out = build_forward_response_headers(&orig, &orig_flat, &modification, None);
        let set_cookies: Vec<&str> = out
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap_or(""))
            .collect();
        // Original + two appended.
        assert_eq!(set_cookies.len(), 3);
        assert!(set_cookies.contains(&"a=1"));
        assert!(set_cookies.contains(&"b=2; Path=/"));
        assert!(set_cookies.contains(&"c=3; Secure"));
    }

    #[test]
    fn forward_response_strips_stale_length_when_body_replaced() {
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("content-length"),
            HeaderValue::from_static("123"),
        );
        orig.append(
            HeaderName::from_static("content-encoding"),
            HeaderValue::from_static("gzip"),
        );
        orig.append(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("text/plain"),
        );

        let orig_flat = headermap_to_flat(&orig);
        let modification = ResponseModification {
            headers: orig_flat.clone(),
            ..Default::default()
        };

        let out = build_forward_response_headers(&orig, &orig_flat, &modification, Some(42));
        assert_eq!(
            out.get("content-length").and_then(|v| v.to_str().ok()),
            Some("42")
        );
        assert!(out.get("content-encoding").is_none());
    }

    #[test]
    fn forward_response_removes_explicit_headers() {
        let mut orig = HeaderMap::new();
        orig.append(
            HeaderName::from_static("x-powered-by"),
            HeaderValue::from_static("magic"),
        );
        orig.append(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("text/plain"),
        );

        let orig_flat = headermap_to_flat(&orig);
        let modification = ResponseModification {
            headers: orig_flat.clone(),
            headers_to_remove: vec!["x-powered-by".into()],
            ..Default::default()
        };

        let out = build_forward_response_headers(&orig, &orig_flat, &modification, None);
        assert!(out.get("x-powered-by").is_none());
        assert!(out.get("content-type").is_some());
    }
}
