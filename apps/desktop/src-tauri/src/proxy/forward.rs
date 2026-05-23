//! Unified request forwarding module
//!
//! This module handles forwarding requests to upstream servers with support for:
//! - Protocol conversion (HTTP <-> HTTPS)
//! - Whistle-compatible path forwarding
//! - All protocol combinations (HTTP/1.1, HTTP/2, HTTPS)

use crate::error::{PostGateError, Result};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use url::Url;

use super::tls::{create_tls_connector, parse_server_name};

/// Target URL information parsed from rule action
#[derive(Debug, Clone)]
pub struct ForwardTarget {
    /// Target scheme (http or https)
    pub scheme: String,
    /// Target host
    pub host: String,
    /// Target port
    pub port: u16,
    /// Target path (from the rule, e.g., /browser)
    pub path: String,
    /// Remaining path to append (from pattern match)
    pub remaining_path: String,
}

impl ForwardTarget {
    /// Parse a target URL string and remaining path into ForwardTarget
    ///
    /// # Arguments
    /// * `target` - Target URL like "http://127.0.0.1:3000/browser" or "127.0.0.1:3000"
    /// * `remaining_path` - Remaining path from pattern match (e.g., "/page/1?id=123")
    /// * `original_scheme` - Original request scheme (http/https) as fallback
    pub fn parse(target: &str, remaining_path: &str, original_scheme: &str) -> Result<Self> {
        // Check if target is a full URL
        if target.starts_with("http://") || target.starts_with("https://") {
            let url = Url::parse(target)
                .map_err(|e| PostGateError::Proxy(format!("Invalid target URL: {}", e)))?;

            let scheme = url.scheme().to_string();
            let host = url
                .host_str()
                .ok_or_else(|| PostGateError::Proxy("Missing host in target URL".into()))?
                .to_string();
            let port = url
                .port()
                .unwrap_or(if scheme == "https" { 443 } else { 80 });
            let path = url.path().to_string();

            Ok(ForwardTarget {
                scheme,
                host,
                port,
                path,
                remaining_path: remaining_path.to_string(),
            })
        } else {
            // Target is just host:port or host (no scheme specified)
            // Whistle behavior: bare host:port defaults to HTTP, not the original scheme.
            // Only when no port is specified AND original was HTTPS, keep HTTPS.
            let (host, port, scheme) = if target.contains(':') {
                let parts: Vec<&str> = target.rsplitn(2, ':').collect();
                let port: u16 = parts[0].parse().map_err(|_| {
                    PostGateError::Proxy(format!("Invalid port in target: {}", target))
                })?;
                // Bare host:port → default to HTTP (user explicitly specified a port,
                // likely a local dev server). Use HTTPS only for port 443.
                let scheme = if port == 443 { "https" } else { "http" };
                (parts[1].to_string(), port, scheme.to_string())
            } else {
                let port = if original_scheme == "https" { 443 } else { 80 };
                (target.to_string(), port, original_scheme.to_string())
            };

            Ok(ForwardTarget {
                scheme,
                host,
                port,
                path: String::new(),
                remaining_path: remaining_path.to_string(),
            })
        }
    }

    /// Build the final URL for the request
    ///
    /// Whistle-compatible path joining:
    /// - Target: http://127.0.0.1:3000/browser
    /// - Remaining: /page/1?id=123
    /// - Result: http://127.0.0.1:3000/browser/page/1?id=123
    pub fn build_url(&self) -> String {
        let base = format!("{}://{}:{}", self.scheme, self.host, self.port);

        // Join target path and remaining path
        let full_path = join_paths(&self.path, &self.remaining_path);

        format!("{}{}", base, full_path)
    }

    /// Build the path for HTTP/1.1 requests (just path, no host)
    pub fn build_path(&self) -> String {
        join_paths(&self.path, &self.remaining_path)
    }

    /// Check if this target requires HTTPS
    pub fn is_https(&self) -> bool {
        self.scheme == "https"
    }
}

/// Apply request-side URL rewrites from `urlParams://` and
/// `pathReplace://`/`urlReplace://` to an already resolved absolute upstream
/// URL. Host rewrites happen first via `ForwardTarget`; this final pass keeps
/// all proxy entry points aligned.
pub fn apply_request_url_modifications(
    base_url: &str,
    path: Option<&str>,
    query_params: Option<&str>,
) -> Result<String> {
    if path.is_none() && query_params.is_none() {
        return Ok(base_url.to_string());
    }

    let mut url = Url::parse(base_url)
        .map_err(|e| PostGateError::Proxy(format!("Invalid upstream URL {}: {}", base_url, e)))?;

    if let Some(path) = path {
        let (path_part, query_from_path) = path.split_once('?').unwrap_or((path, ""));
        let normalized_path = if path_part.is_empty() {
            "/".to_string()
        } else if path_part.starts_with('/') {
            path_part.to_string()
        } else {
            format!("/{}", path_part)
        };
        url.set_path(&normalized_path);

        // `pathReplace://` normally targets only the path, but accepting a
        // replacement that includes `?query` makes `urlReplace://` useful too.
        if !query_from_path.is_empty() && query_params.is_none() {
            url.set_query(Some(query_from_path));
        }
    }

    if let Some(query_params) = query_params {
        if query_params.is_empty() {
            url.set_query(None);
        } else {
            url.set_query(Some(query_params));
        }
    }

    Ok(url.to_string())
}

pub fn path_and_query_from_url(uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    let mut path = url.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    Some(path)
}

/// Join two paths whistle-style
///
/// Examples:
/// - ("/browser", "/page/1") -> "/browser/page/1"
/// - ("/browser/", "/page/1") -> "/browser/page/1"
/// - ("/browser", "?id=123") -> "/browser?id=123"
/// - ("", "/page/1") -> "/page/1"
fn join_paths(base: &str, remaining: &str) -> String {
    if base.is_empty() {
        if remaining.is_empty() {
            return "/".to_string();
        }
        if !remaining.starts_with('/') && !remaining.starts_with('?') {
            return format!("/{}", remaining);
        }
        return remaining.to_string();
    }

    if remaining.is_empty() {
        if base.is_empty() {
            return "/".to_string();
        }
        return base.to_string();
    }

    // Handle query string in remaining
    if remaining.starts_with('?') {
        return format!("{}{}", base.trim_end_matches('/'), remaining);
    }

    // Join with single slash
    let base_trimmed = base.trim_end_matches('/');
    let remaining_trimmed = remaining.trim_start_matches('/');

    format!("{}/{}", base_trimmed, remaining_trimmed)
}

/// Forward a request to the target, handling protocol conversion
///
/// This is the main entry point for all forwarding, supporting:
/// - HTTP -> HTTP
/// - HTTP -> HTTPS
/// - HTTPS -> HTTP
/// - HTTPS -> HTTPS
pub async fn forward_request(
    method: hyper::Method,
    target: &ForwardTarget,
    headers: &std::collections::HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    if target.is_https() {
        forward_https(method, target, headers, body).await
    } else {
        forward_http(method, target, headers, body).await
    }
}

/// Forward request to HTTP target
async fn forward_http(
    method: hyper::Method,
    target: &ForwardTarget,
    headers: &std::collections::HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    // Connect to upstream
    let addr = format!("{}:{}", target.host, target.port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

    let io = TokioIo::new(stream);

    // Create HTTP connection
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP handshake error: {}", e)))?;

    // Spawn connection driver
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("HTTP connection error: {}", e);
        }
    });

    // Build request with target path
    let uri = target.build_path();
    let mut builder = Request::builder().method(method).uri(&uri);

    // Copy headers, updating Host header
    for (key, value) in headers {
        let key_lower = key.to_lowercase();
        if key_lower == "host" {
            // Update host header to target host
            let host_value = if target.port == 80 {
                target.host.clone()
            } else {
                format!("{}:{}", target.host, target.port)
            };
            builder = builder.header("host", host_value);
        } else {
            if let (Ok(name), Ok(value)) = (
                hyper::header::HeaderName::from_bytes(key.as_bytes()),
                hyper::header::HeaderValue::from_str(value),
            ) {
                builder = builder.header(name, value);
            }
        }
    }

    let req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;

    // Send request
    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP request error: {}", e)))?;

    // Collect response body
    let (parts, incoming) = resp.into_parts();
    let response_body = collect_body(incoming, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;
    let resp_without_body = Response::from_parts(parts, ());

    Ok((resp_without_body, response_body))
}

/// Forward request to HTTPS target
async fn forward_https(
    method: hyper::Method,
    target: &ForwardTarget,
    headers: &std::collections::HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    // Connect to upstream
    let addr = format!("{}:{}", target.host, target.port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

    // TLS handshake with upstream
    let connector = create_tls_connector()?;
    let server_name = parse_server_name(&target.host)?;

    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| PostGateError::Proxy(format!("TLS connect error: {}", e)))?;

    let io = TokioIo::new(tls_stream);

    // Create HTTP connection
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTPS handshake error: {}", e)))?;

    // Spawn connection driver
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("HTTPS connection error: {}", e);
        }
    });

    // Build request with target path
    let uri = target.build_path();
    let mut builder = Request::builder().method(method).uri(&uri);

    // Copy headers, updating Host header
    for (key, value) in headers {
        let key_lower = key.to_lowercase();
        if key_lower == "host" {
            // Update host header to target host
            let host_value = if target.port == 443 {
                target.host.clone()
            } else {
                format!("{}:{}", target.host, target.port)
            };
            builder = builder.header("host", host_value);
        } else {
            if let (Ok(name), Ok(value)) = (
                hyper::header::HeaderName::from_bytes(key.as_bytes()),
                hyper::header::HeaderValue::from_str(value),
            ) {
                builder = builder.header(name, value);
            }
        }
    }

    let req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;

    // Send request
    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTPS request error: {}", e)))?;

    // Collect response body
    let (parts, incoming) = resp.into_parts();
    let response_body = collect_body(incoming, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response: {}", e)))?;
    let resp_without_body = Response::from_parts(parts, ());

    Ok((resp_without_body, response_body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_paths() {
        assert_eq!(join_paths("/browser", "/page/1"), "/browser/page/1");
        assert_eq!(join_paths("/browser/", "/page/1"), "/browser/page/1");
        assert_eq!(join_paths("/browser", "page/1"), "/browser/page/1");
        assert_eq!(join_paths("/browser", "?id=123"), "/browser?id=123");
        assert_eq!(join_paths("", "/page/1"), "/page/1");
        assert_eq!(join_paths("/browser", ""), "/browser");
        assert_eq!(join_paths("", ""), "/");
    }

    #[test]
    fn test_forward_target_parse_full_url() {
        let target =
            ForwardTarget::parse("http://127.0.0.1:3000/browser", "/page/1?id=123", "https")
                .unwrap();

        assert_eq!(target.scheme, "http");
        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 3000);
        assert_eq!(target.path, "/browser");
        assert_eq!(target.remaining_path, "/page/1?id=123");
        assert_eq!(
            target.build_url(),
            "http://127.0.0.1:3000/browser/page/1?id=123"
        );
    }

    #[test]
    fn test_forward_target_parse_host_only() {
        // Bare host:port should default to HTTP, not inherit HTTPS
        let target = ForwardTarget::parse("127.0.0.1:3000", "/api/users", "https").unwrap();

        assert_eq!(target.scheme, "http");
        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 3000);
        assert_eq!(target.path, "");
        assert_eq!(target.remaining_path, "/api/users");
        assert_eq!(target.build_url(), "http://127.0.0.1:3000/api/users");
    }

    #[test]
    fn test_forward_target_bare_host_port443_keeps_https() {
        // Port 443 should use HTTPS
        let target = ForwardTarget::parse("example.com:443", "/api", "https").unwrap();

        assert_eq!(target.scheme, "https");
    }

    #[test]
    fn test_forward_target_bare_host_no_port_keeps_original() {
        // No port → inherit original scheme
        let target = ForwardTarget::parse("example.com", "/api", "https").unwrap();

        assert_eq!(target.scheme, "https");
        assert_eq!(target.port, 443);
    }

    #[test]
    fn test_forward_target_localhost_8080_uses_http() {
        // localhost:8080 from HTTPS context should use HTTP (the TLS bug fix)
        let target = ForwardTarget::parse("localhost:8080", "/x/cover/page.html", "https").unwrap();

        assert_eq!(target.scheme, "http");
        assert_eq!(target.host, "localhost");
        assert_eq!(target.port, 8080);
        assert!(!target.is_https());
    }

    #[test]
    fn test_forward_target_https_to_http() {
        // Rule: https://v.qq.com/biu/u/history/ http://127.0.0.1:3000/browser
        // Request: https://v.qq.com/biu/u/history/page/1?id=123
        // Remaining: /page/1?id=123
        // Expected: http://127.0.0.1:3000/browser/page/1?id=123

        let target =
            ForwardTarget::parse("http://127.0.0.1:3000/browser", "/page/1?id=123", "https")
                .unwrap();

        assert!(!target.is_https());
        assert_eq!(
            target.build_url(),
            "http://127.0.0.1:3000/browser/page/1?id=123"
        );
    }

    #[test]
    fn test_forward_target_exact_match_no_remaining() {
        // Rule: https://v.qq.com/biu/u/history/ http://127.0.0.1:3000/browser
        // Request: https://v.qq.com/biu/u/history/
        // Remaining: "" (empty)
        // Expected: http://127.0.0.1:3000/browser

        let target = ForwardTarget::parse("http://127.0.0.1:3000/browser", "", "https").unwrap();

        assert_eq!(target.build_url(), "http://127.0.0.1:3000/browser");
    }

    #[test]
    fn test_apply_request_url_modifications_path_and_query() {
        let url = apply_request_url_modifications(
            "https://example.com/api/users?debug=false",
            Some("/v2/users"),
            Some("debug=true&cache=off"),
        )
        .unwrap();

        assert_eq!(url, "https://example.com/v2/users?debug=true&cache=off");
    }

    #[test]
    fn test_apply_request_url_modifications_empty_query_removes_query() {
        let url =
            apply_request_url_modifications("https://example.com/api?debug=true", None, Some(""))
                .unwrap();

        assert_eq!(url, "https://example.com/api");
    }
}
