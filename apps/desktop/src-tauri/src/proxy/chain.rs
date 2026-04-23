//! Upstream proxy chaining.
//!
//! Supports whistle's `proxy://`, `http-proxy://`, `https-proxy://` and
//! `socks://` rules. Requests matching such a rule bypass the shared hyper
//! client (since it wouldn't know how to route through another proxy) and
//! use a one-shot forwarder that either:
//!
//! * opens a TCP connection to the proxy and sends a plain-text HTTP/1.1
//!   request with an absolute-form URI (HTTP targets via HTTP proxy), or
//! * opens a TCP connection to the proxy, sends HTTP CONNECT to tunnel, and
//!   does TLS / plain-HTTP over the tunnel (HTTPS targets via HTTP proxy).
//!
//! SOCKS is currently a TODO; the action is recognised but we log a warning
//! and fall back to direct forwarding.

use crate::error::{PostGateError, Result};
use crate::proxy::body::{collect_body, CapturedBody, MAX_BODY_SIZE};
use crate::proxy::tls::{create_tls_connector_with_alpn, parse_server_name};
use crate::rules::{ProxyCreds, UpstreamProxy, UpstreamProxyKind};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use url::Url;

/// Forward a request through an upstream HTTP/HTTPS proxy and collect the
/// response body. Returns the same shape as `upstream::forward_collect` so
/// callers can swap them based on whether a proxy rule matched.
pub async fn forward_via_proxy(
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
    proxy: &UpstreamProxy,
    timeout_ms: Option<u64>,
) -> Result<(Response<()>, CapturedBody)> {
    let fut = forward_via_proxy_inner(method, absolute_url, headers, body, proxy);
    match timeout_ms {
        Some(ms) => match tokio::time::timeout(std::time::Duration::from_millis(ms), fut).await {
            Ok(r) => r,
            Err(_) => Err(PostGateError::Proxy(format!(
                "Upstream request timed out after {} ms",
                ms
            ))),
        },
        None => fut.await,
    }
}

async fn forward_via_proxy_inner(
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
    proxy: &UpstreamProxy,
) -> Result<(Response<()>, CapturedBody)> {
    match proxy.kind {
        UpstreamProxyKind::Socks4 | UpstreamProxyKind::Socks5 => {
            let parsed = Url::parse(absolute_url).map_err(|e| {
                PostGateError::Proxy(format!("Invalid upstream URL: {}", e))
            })?;
            let target_is_https = parsed.scheme() == "https";
            let target_host = parsed
                .host_str()
                .ok_or_else(|| PostGateError::Proxy("Missing host in URL".into()))?
                .to_string();
            let target_port = parsed
                .port()
                .unwrap_or(if target_is_https { 443 } else { 80 });

            // Open a SOCKS connection that, once completed, tunnels transparent
            // bytes to target_host:target_port.
            let proxy_addr = format!("{}:{}", proxy.host, proxy.port);
            let tcp = TcpStream::connect(&proxy_addr)
                .await
                .map_err(|e| PostGateError::Proxy(format!("SOCKS connect: {}", e)))?;

            let tunneled = match proxy.kind {
                UpstreamProxyKind::Socks5 => {
                    socks5_handshake(tcp, &target_host, target_port, proxy.auth.as_ref()).await?
                }
                UpstreamProxyKind::Socks4 => {
                    socks4_handshake(tcp, &target_host, target_port, proxy.auth.as_ref()).await?
                }
                _ => unreachable!(),
            };

            if target_is_https {
                let connector = create_tls_connector_with_alpn(&[b"http/1.1"])?;
                let server_name = parse_server_name(&target_host)?;
                let tls = connector
                    .connect(server_name, tunneled)
                    .await
                    .map_err(|e| PostGateError::Proxy(format!("TLS over SOCKS failed: {}", e)))?;
                send_http1_and_collect(TokioIo::new(tls), method, &parsed, headers, body).await
            } else {
                send_http1_and_collect(TokioIo::new(tunneled), method, &parsed, headers, body).await
            }
        }
        UpstreamProxyKind::Http | UpstreamProxyKind::Https => {
            let parsed = Url::parse(absolute_url).map_err(|e| {
                PostGateError::Proxy(format!("Invalid upstream URL: {}", e))
            })?;
            let target_is_https = parsed.scheme() == "https";

            // Connect to the proxy. For HTTPS proxies we additionally TLS-wrap
            // the connection to the proxy itself.
            let proxy_addr = format!("{}:{}", proxy.host, proxy.port);
            let tcp = TcpStream::connect(&proxy_addr)
                .await
                .map_err(|e| PostGateError::Proxy(format!("Proxy connect error: {}", e)))?;

            if target_is_https {
                // HTTPS target: use HTTP CONNECT to tunnel, then TLS to the
                // origin server inside the tunnel.
                let target_host = parsed
                    .host_str()
                    .ok_or_else(|| PostGateError::Proxy("Missing host in URL".into()))?
                    .to_string();
                let target_port = parsed.port().unwrap_or(443);
                let tunneled = connect_tunnel(
                    tcp,
                    proxy,
                    &target_host,
                    target_port,
                )
                .await?;

                // TLS inside the CONNECT tunnel.
                let connector = create_tls_connector_with_alpn(&[b"http/1.1"])?;
                let server_name = parse_server_name(&target_host)?;
                let tls = connector
                    .connect(server_name, tunneled)
                    .await
                    .map_err(|e| PostGateError::Proxy(format!("TLS over proxy failed: {}", e)))?;

                send_http1_and_collect(TokioIo::new(tls), method, &parsed, headers, body).await
            } else {
                // HTTP target via HTTP proxy: send an absolute-form request
                // to the proxy directly. No CONNECT needed.
                send_http_via_absolute_form(tcp, proxy, method, absolute_url, headers, body).await
            }
        }
    }
}

/// Open an HTTP CONNECT tunnel through the proxy to `host:port`.
async fn connect_tunnel(
    mut stream: TcpStream,
    proxy: &UpstreamProxy,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream> {
    let authority = format!("{}:{}", target_host, target_port);
    let mut req = format!(
        "CONNECT {} HTTP/1.1\r\nHost: {}\r\n",
        authority, authority
    );
    if let Some(ref creds) = proxy.auth {
        req.push_str(&proxy_auth_header(creds));
    }
    req.push_str("Proxy-Connection: keep-alive\r\n\r\n");

    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| PostGateError::Proxy(format!("Proxy CONNECT write error: {}", e)))?;

    // Parse the response status line + headers. We read until \r\n\r\n.
    let mut buf = Vec::with_capacity(256);
    let mut tmp = [0u8; 256];
    loop {
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Proxy CONNECT read error: {}", e)))?;
        if n == 0 {
            return Err(PostGateError::Proxy("Proxy closed during CONNECT".into()));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            return Err(PostGateError::Proxy(
                "Proxy CONNECT response too large".into(),
            ));
        }
    }

    let head = String::from_utf8_lossy(&buf);
    let status_line = head.lines().next().unwrap_or("");
    if !status_line.contains(" 200 ") {
        return Err(PostGateError::Proxy(format!(
            "Proxy CONNECT rejected: {}",
            status_line
        )));
    }

    Ok(stream)
}

/// Send an HTTP/1.1 request in absolute-form (e.g. `GET http://host/path`)
/// directly to the proxy. Used for plain-HTTP targets.
async fn send_http_via_absolute_form(
    stream: TcpStream,
    proxy: &UpstreamProxy,
    method: hyper::Method,
    absolute_url: &str,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)> {
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP proxy handshake error: {}", e)))?;

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("HTTP proxy connection error: {}", e);
        }
    });

    let mut builder = Request::builder().method(method).uri(absolute_url);
    for (k, v) in headers {
        let k_lower = k.to_lowercase();
        if matches!(
            k_lower.as_str(),
            "connection" | "keep-alive" | "proxy-connection" | "transfer-encoding" | "upgrade"
        ) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(k_lower.as_bytes()),
            hyper::header::HeaderValue::from_str(v),
        ) {
            builder = builder.header(name, value);
        }
    }
    // Proxy auth
    if let Some(ref creds) = proxy.auth {
        let header_value = proxy_auth_header(creds);
        let header_value = header_value.trim_end_matches("\r\n");
        // proxy_auth_header returns "Proxy-Authorization: ...\r\n"; strip
        // the prefix for hyper's header API.
        if let Some(colon) = header_value.find(':') {
            let v = header_value[colon + 1..].trim();
            if let Ok(val) = hyper::header::HeaderValue::from_str(v) {
                builder = builder.header("proxy-authorization", val);
            }
        }
    }

    let req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build proxy request: {}", e)))?;

    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP via proxy failed: {}", e)))?;

    let (parts, incoming) = resp.into_parts();
    let body = collect_body(incoming, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read proxied response: {}", e)))?;
    Ok((Response::from_parts(parts, ()), body))
}

/// Helper: send an HTTP/1.1 request over a pre-established stream (e.g.
/// the TLS stream inside a CONNECT tunnel) and collect the response body.
async fn send_http1_and_collect<T>(
    io: TokioIo<T>,
    method: hyper::Method,
    target_url: &Url,
    headers: &HashMap<String, String>,
    body: Bytes,
) -> Result<(Response<()>, CapturedBody)>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(io)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Tunneled handshake error: {}", e)))?;

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("Tunneled connection error: {}", e);
        }
    });

    let path = target_url.path();
    let query = target_url.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let uri = format!("{}{}", path, query);
    let host_value = if let Some(port) = target_url.port() {
        format!("{}:{}", target_url.host_str().unwrap_or(""), port)
    } else {
        target_url.host_str().unwrap_or("").to_string()
    };

    let mut builder = Request::builder().method(method).uri(&uri);
    let mut host_sent = false;
    for (k, v) in headers {
        let k_lower = k.to_lowercase();
        if matches!(
            k_lower.as_str(),
            "connection" | "keep-alive" | "proxy-connection" | "transfer-encoding" | "upgrade"
        ) {
            continue;
        }
        if k_lower == "host" {
            if let Ok(val) = hyper::header::HeaderValue::from_str(&host_value) {
                builder = builder.header("host", val);
            }
            host_sent = true;
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(k_lower.as_bytes()),
            hyper::header::HeaderValue::from_str(v),
        ) {
            builder = builder.header(name, value);
        }
    }
    if !host_sent {
        if let Ok(val) = hyper::header::HeaderValue::from_str(&host_value) {
            builder = builder.header("host", val);
        }
    }

    let req = builder
        .body(Full::new(body))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;
    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Tunneled request failed: {}", e)))?;

    let (parts, incoming) = resp.into_parts();
    let body = collect_body(incoming, MAX_BODY_SIZE)
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read tunneled response: {}", e)))?;
    Ok((Response::from_parts(parts, ()), body))
}

fn proxy_auth_header(creds: &ProxyCreds) -> String {
    use base64::Engine;
    let raw = format!("{}:{}", creds.username, creds.password);
    let b64 = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
    format!("Proxy-Authorization: Basic {}\r\n", b64)
}

/// Perform a SOCKS5 handshake per RFC 1928 and return the tunneled stream
/// ready to forward bytes to `target_host:target_port`.
///
/// Flow:
/// 1. Greeting: advertise supported auth methods (No-auth + optionally
///    User/Pass when creds are supplied).
/// 2. Server picks a method; if User/Pass (0x02) was chosen, perform
///    RFC 1929 sub-negotiation.
/// 3. Send CONNECT request with domain ATYP (we let the SOCKS server do DNS).
/// 4. Parse the CONNECT reply, validate REP==0x00.
async fn socks5_handshake(
    mut stream: TcpStream,
    target_host: &str,
    target_port: u16,
    auth: Option<&ProxyCreds>,
) -> Result<TcpStream> {
    // --- 1. Greeting ---
    // Offer No-auth (0x00). Add User/Pass (0x02) when creds are provided.
    let greeting: Vec<u8> = if auth.is_some() {
        vec![0x05, 0x02, 0x00, 0x02]
    } else {
        vec![0x05, 0x01, 0x00]
    };
    stream
        .write_all(&greeting)
        .await
        .map_err(|e| PostGateError::Proxy(format!("SOCKS5 greet write: {}", e)))?;

    let mut method_reply = [0u8; 2];
    stream
        .read_exact(&mut method_reply)
        .await
        .map_err(|e| PostGateError::Proxy(format!("SOCKS5 greet read: {}", e)))?;
    if method_reply[0] != 0x05 {
        return Err(PostGateError::Proxy(format!(
            "SOCKS5 bad version: 0x{:02x}",
            method_reply[0]
        )));
    }
    let chosen = method_reply[1];
    match chosen {
        0x00 => {} // No-auth accepted.
        0x02 => {
            // RFC 1929 username/password sub-negotiation.
            let Some(creds) = auth else {
                return Err(PostGateError::Proxy(
                    "SOCKS5 server required user/pass but no creds supplied".into(),
                ));
            };
            let ubytes = creds.username.as_bytes();
            let pbytes = creds.password.as_bytes();
            if ubytes.len() > 255 || pbytes.len() > 255 {
                return Err(PostGateError::Proxy(
                    "SOCKS5 user/pass too long (>255 bytes)".into(),
                ));
            }
            let mut buf = Vec::with_capacity(3 + ubytes.len() + pbytes.len());
            buf.push(0x01); // sub-negotiation version
            buf.push(ubytes.len() as u8);
            buf.extend_from_slice(ubytes);
            buf.push(pbytes.len() as u8);
            buf.extend_from_slice(pbytes);
            stream
                .write_all(&buf)
                .await
                .map_err(|e| PostGateError::Proxy(format!("SOCKS5 auth write: {}", e)))?;

            let mut auth_reply = [0u8; 2];
            stream
                .read_exact(&mut auth_reply)
                .await
                .map_err(|e| PostGateError::Proxy(format!("SOCKS5 auth read: {}", e)))?;
            if auth_reply[1] != 0x00 {
                return Err(PostGateError::Proxy(format!(
                    "SOCKS5 auth rejected: status=0x{:02x}",
                    auth_reply[1]
                )));
            }
        }
        0xFF => {
            return Err(PostGateError::Proxy(
                "SOCKS5 server rejected all offered auth methods".into(),
            ));
        }
        other => {
            return Err(PostGateError::Proxy(format!(
                "SOCKS5 unsupported method: 0x{:02x}",
                other
            )));
        }
    }

    // --- 3. CONNECT request ---
    // We always send ATYP=domain so the SOCKS server resolves the target.
    let host_bytes = target_host.as_bytes();
    if host_bytes.len() > 255 {
        return Err(PostGateError::Proxy(
            "SOCKS5 target host >255 bytes".into(),
        ));
    }
    let mut req = Vec::with_capacity(7 + host_bytes.len());
    req.push(0x05); // VER
    req.push(0x01); // CMD = CONNECT
    req.push(0x00); // RSV
    req.push(0x03); // ATYP = DOMAIN
    req.push(host_bytes.len() as u8);
    req.extend_from_slice(host_bytes);
    req.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("SOCKS5 connect write: {}", e)))?;

    // --- 4. CONNECT reply ---
    let mut head = [0u8; 4];
    stream
        .read_exact(&mut head)
        .await
        .map_err(|e| PostGateError::Proxy(format!("SOCKS5 reply read: {}", e)))?;
    if head[0] != 0x05 {
        return Err(PostGateError::Proxy(format!(
            "SOCKS5 reply bad version: 0x{:02x}",
            head[0]
        )));
    }
    if head[1] != 0x00 {
        return Err(PostGateError::Proxy(format!(
            "SOCKS5 CONNECT rejected: REP=0x{:02x}",
            head[1]
        )));
    }
    // Drain BND.ADDR + BND.PORT so the stream is positioned at user payload.
    match head[3] {
        0x01 => {
            let mut skip = [0u8; 4 + 2];
            stream
                .read_exact(&mut skip)
                .await
                .map_err(|e| PostGateError::Proxy(format!("SOCKS5 drain v4: {}", e)))?;
        }
        0x04 => {
            let mut skip = [0u8; 16 + 2];
            stream
                .read_exact(&mut skip)
                .await
                .map_err(|e| PostGateError::Proxy(format!("SOCKS5 drain v6: {}", e)))?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream
                .read_exact(&mut len)
                .await
                .map_err(|e| PostGateError::Proxy(format!("SOCKS5 drain dom len: {}", e)))?;
            let mut skip = vec![0u8; len[0] as usize + 2];
            stream
                .read_exact(&mut skip)
                .await
                .map_err(|e| PostGateError::Proxy(format!("SOCKS5 drain dom: {}", e)))?;
        }
        other => {
            return Err(PostGateError::Proxy(format!(
                "SOCKS5 reply unknown ATYP: 0x{:02x}",
                other
            )));
        }
    }

    Ok(stream)
}

/// Perform a SOCKS4 / SOCKS4a handshake and return the tunneled stream.
///
/// SOCKS4 proper requires an IPv4 literal. We always use the SOCKS4a
/// extension (DSTIP = 0.0.0.1 + trailing hostname) so the SOCKS server
/// resolves the target — this also works transparently on servers that
/// advertise SOCKS4 only but never receive raw IPv4 literals from us.
///
/// The `USERID` field is used as the username when creds are present; SOCKS4
/// has no password field.
async fn socks4_handshake(
    mut stream: TcpStream,
    target_host: &str,
    target_port: u16,
    auth: Option<&ProxyCreds>,
) -> Result<TcpStream> {
    let userid = auth.map(|a| a.username.as_str()).unwrap_or("");
    let userid_bytes = userid.as_bytes();
    let host_bytes = target_host.as_bytes();

    // VN=4, CD=1 (CONNECT), DSTPORT(2), DSTIP=0.0.0.1 (SOCKS4a sentinel),
    // USERID, 0x00, HOSTNAME, 0x00.
    let mut req = Vec::with_capacity(9 + userid_bytes.len() + host_bytes.len());
    req.push(0x04); // VN
    req.push(0x01); // CD = CONNECT
    req.extend_from_slice(&target_port.to_be_bytes());
    req.extend_from_slice(&[0, 0, 0, 1]); // SOCKS4a sentinel DSTIP
    req.extend_from_slice(userid_bytes);
    req.push(0x00);
    req.extend_from_slice(host_bytes);
    req.push(0x00);

    stream
        .write_all(&req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("SOCKS4 write: {}", e)))?;

    // Reply: VN(0x00) CD DSTPORT(2) DSTIP(4) — 8 bytes total.
    let mut reply = [0u8; 8];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| PostGateError::Proxy(format!("SOCKS4 read: {}", e)))?;
    if reply[0] != 0x00 {
        return Err(PostGateError::Proxy(format!(
            "SOCKS4 reply bad VN: 0x{:02x}",
            reply[0]
        )));
    }
    if reply[1] != 0x5A {
        return Err(PostGateError::Proxy(format!(
            "SOCKS4 CONNECT rejected: CD=0x{:02x}",
            reply[1]
        )));
    }
    Ok(stream)
}
