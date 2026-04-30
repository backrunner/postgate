use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use hyper_util::rt::TokioIo;
use moka::sync::Cache;
use rustls::pki_types::ServerName;
use rustls::ServerConfig;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsAcceptor as TokioTlsAcceptor;

const SERVER_CONFIG_CACHE_MAX_CAPACITY: u64 = 1000;
const SERVER_CONFIG_CACHE_TTL_HOURS: u64 = 23;

/// Global cache of pre-built rustls `ServerConfig`s keyed by `(host, h2)`.
///
/// Building a `ServerConfig` involves copying the certificate chain, cloning
/// the private key and hashing the ALPN list; doing it per CONNECT is pure
/// overhead. The underlying `CertifiedKey` is already cached in `ca.rs`, we
/// just bolt another cache on top to skip the rustls builder path entirely.
fn server_config_cache() -> &'static Cache<(String, bool), Arc<ServerConfig>> {
    static CACHE: OnceLock<Cache<(String, bool), Arc<ServerConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Cache::builder()
            .max_capacity(SERVER_CONFIG_CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(SERVER_CONFIG_CACHE_TTL_HOURS * 3600))
            .build()
    })
}

/// TLS acceptor for MITM connections
pub struct TlsAcceptor {
    inner: TokioTlsAcceptor,
}

impl TlsAcceptor {
    /// Create a new TLS acceptor for a specific host.
    ///
    /// When `enable_http2` is true the server will offer both `h2` and
    /// `http/1.1` via ALPN; otherwise only `http/1.1`. This must match what
    /// the rest of the proxy actually supports — otherwise clients will pick
    /// h2 and the h2 code path ends up handling requests the user asked to
    /// disable.
    pub fn new(ca: &CertificateAuthority, host: &str, enable_http2: bool) -> Result<Self> {
        let cache = server_config_cache();
        let cache_key = (host.to_string(), enable_http2);
        if let Some(existing) = cache.get(&cache_key) {
            return Ok(Self {
                inner: TokioTlsAcceptor::from(existing),
            });
        }

        let certified_key = ca.get_cert_for_host(host)?;

        let key = certified_key.key.clone_key();

        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certified_key.cert_chain.clone(), key)
            .map_err(|e| PostGateError::Certificate(format!("TLS config error: {}", e)))?;

        config.alpn_protocols = if enable_http2 {
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        } else {
            vec![b"http/1.1".to_vec()]
        };

        let config = Arc::new(config);
        cache.insert(cache_key, config.clone());

        Ok(Self {
            inner: TokioTlsAcceptor::from(config),
        })
    }

    /// Invalidate the cached TLS configs. Called when the underlying CA
    /// changes or certificates are rotated.
    #[allow(dead_code)]
    pub fn clear_cache() {
        server_config_cache().invalidate_all();
    }

    /// Accept a TLS connection from a TokioIo-wrapped stream
    pub async fn accept<S>(
        &self,
        stream: TokioIo<S>,
    ) -> Result<tokio_rustls::server::TlsStream<TokioIo<S>>>
    where
        TokioIo<S>: AsyncRead + AsyncWrite + Unpin,
    {
        self.inner
            .accept(stream)
            .await
            .map_err(|e| PostGateError::Certificate(format!("TLS accept error: {}", e)))
    }
}

/// Create a TLS connector for connecting to upstream servers (HTTP/1.1, no ALPN).
pub fn create_tls_connector() -> Result<tokio_rustls::TlsConnector> {
    create_tls_connector_with_alpn(&[])
}

/// Create a TLS connector that advertises the given ALPN protocols to the
/// upstream server.
///
/// HTTP/2 clients MUST negotiate `h2` through ALPN. Without this the upstream
/// will fall back to HTTP/1.1 and the h2 preface we send afterwards is
/// interpreted as a plaintext HTTP/1.1 request, causing rustls record-layer
/// errors like `received corrupt message of type InvalidContentType` on the
/// next read.
pub fn create_tls_connector_with_alpn(alpn: &[&[u8]]) -> Result<tokio_rustls::TlsConnector> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    if !alpn.is_empty() {
        config.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
    }

    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

/// Parse a host string into a ServerName
pub fn parse_server_name(host: &str) -> Result<ServerName<'static>> {
    ServerName::try_from(host.to_string())
        .map_err(|_| PostGateError::Certificate(format!("Invalid server name: {}", host)))
}

/// Get TLS protocol version string from rustls ProtocolVersion
pub fn tls_version_string(version: Option<rustls::ProtocolVersion>) -> String {
    match version {
        Some(rustls::ProtocolVersion::TLSv1_0) => "TLS 1.0".to_string(),
        Some(rustls::ProtocolVersion::TLSv1_1) => "TLS 1.1".to_string(),
        Some(rustls::ProtocolVersion::TLSv1_2) => "TLS 1.2".to_string(),
        Some(rustls::ProtocolVersion::TLSv1_3) => "TLS 1.3".to_string(),
        Some(v) => format!("TLS {:?}", v),
        None => "Unknown".to_string(),
    }
}
