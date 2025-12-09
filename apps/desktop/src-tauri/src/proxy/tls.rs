use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use rustls::ServerConfig;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsAcceptor as TokioTlsAcceptor;

/// TLS acceptor for MITM connections
pub struct TlsAcceptor {
    inner: TokioTlsAcceptor,
}

impl TlsAcceptor {
    /// Create a new TLS acceptor for a specific host
    pub fn new(ca: &CertificateAuthority, host: &str) -> Result<Self> {
        let certified_key = ca.get_cert_for_host(host)?;

        let key = certified_key
            .key
            .clone_key();

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certified_key.cert_chain.clone(), key)
            .map_err(|e| PostGateError::Certificate(format!("TLS config error: {}", e)))?;

        Ok(Self {
            inner: TokioTlsAcceptor::from(Arc::new(config)),
        })
    }

    /// Accept a TLS connection from a TokioIo-wrapped stream
    pub async fn accept<S>(&self, stream: TokioIo<S>) -> Result<tokio_rustls::server::TlsStream<TokioIo<S>>>
    where
        TokioIo<S>: AsyncRead + AsyncWrite + Unpin,
    {
        self.inner
            .accept(stream)
            .await
            .map_err(|e| PostGateError::Certificate(format!("TLS accept error: {}", e)))
    }
}

/// Create a TLS connector for connecting to upstream servers
pub fn create_tls_connector() -> Result<tokio_rustls::TlsConnector> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

/// Parse a host string into a ServerName
pub fn parse_server_name(host: &str) -> Result<ServerName<'static>> {
    ServerName::try_from(host.to_string())
        .map_err(|_| PostGateError::Certificate(format!("Invalid server name: {}", host)))
}
