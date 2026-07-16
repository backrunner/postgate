use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::proxy::handler::{handle_connection, ProxyContext};
use crate::proxy::resource::RemoteResourceCache;
use crate::proxy::upstream::build_upstream_client;
use crate::proxy::BodyStorage;
use crate::rules::RuleEngine;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

/// Proxy server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub port: u16,
    #[serde(rename = "enableHttp2")]
    pub enable_http2: bool,
    #[serde(rename = "enableQuic")]
    pub enable_quic: bool,
    #[serde(rename = "quicPort")]
    pub quic_port: Option<u16>,
    #[serde(rename = "debugPort", default = "default_debug_port")]
    pub debug_port: u16,
    /// Maximum connections per upstream host
    #[serde(rename = "maxConnectionsPerHost", default = "default_max_connections")]
    pub max_connections_per_host: usize,
    /// Connection idle timeout in seconds
    #[serde(rename = "connectionIdleTimeout", default = "default_idle_timeout")]
    pub connection_idle_timeout: u64,
}

fn default_max_connections() -> usize {
    10
}
fn default_idle_timeout() -> u64 {
    60
}
fn default_debug_port() -> u16 {
    9229
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            port: 8899,
            enable_http2: true,
            enable_quic: false,
            quic_port: None,
            debug_port: default_debug_port(),
            max_connections_per_host: default_max_connections(),
            connection_idle_timeout: default_idle_timeout(),
        }
    }
}

impl ProxyConfig {
    pub fn validate(&self) -> Result<()> {
        if self.port == 0 {
            return Err(PostGateError::InvalidState(
                "Proxy port must be between 1 and 65535".into(),
            ));
        }
        if self.debug_port == 0 {
            return Err(PostGateError::InvalidState(
                "Debug server port must be between 1 and 65535".into(),
            ));
        }
        if self.quic_port == Some(0) {
            return Err(PostGateError::InvalidState(
                "QUIC port must be between 1 and 65535".into(),
            ));
        }
        Ok(())
    }
}

/// Proxy server status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

/// MITM Proxy Server
pub struct ProxyServer {
    config: ProxyConfig,
    ctx: Arc<ProxyContext>,
    running: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    #[cfg(feature = "quic")]
    quic_server: Option<super::quic::QuicServer>,
}

impl ProxyServer {
    /// Create a new proxy server
    pub fn new(
        config: ProxyConfig,
        ca: CertificateAuthority,
        rule_engine: Arc<RuleEngine>,
        body_storage: Arc<BodyStorage>,
        app_state: Arc<AppState>,
    ) -> Self {
        // Build the shared upstream client once — its connection pool is what
        // makes the proxy fast. See proxy/upstream.rs.
        let upstream_client = build_upstream_client(
            config.enable_http2,
            config.max_connections_per_host.max(1),
            Duration::from_secs(config.connection_idle_timeout.max(1)),
        );

        let ctx = Arc::new(ProxyContext {
            ca: Arc::new(ca),
            rule_engine,
            body_storage,
            app_state,
            enable_http2: config.enable_http2,
            upstream_client,
            remote_resource_cache: RemoteResourceCache::new(),
        });

        Self {
            config,
            ctx,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            #[cfg(feature = "quic")]
            quic_server: None,
        }
    }

    /// Start the proxy server
    pub async fn start(&mut self) -> Result<()> {
        self.config.validate()?;

        if self.running.load(Ordering::SeqCst) {
            return Err(PostGateError::InvalidState("Proxy already running".into()));
        }

        #[cfg(not(feature = "quic"))]
        if self.config.enable_quic {
            return Err(PostGateError::InvalidState(
                "QUIC is enabled in proxy settings, but this build was compiled without the `quic` feature"
                    .into(),
            ));
        }

        let addr: SocketAddr = format!("127.0.0.1:{}", self.config.port)
            .parse()
            .map_err(|e| PostGateError::Proxy(format!("Invalid address: {}", e)))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Failed to bind to {}: {}", addr, e)))?;

        #[cfg(feature = "quic")]
        if self.config.enable_quic {
            let quic_addr = SocketAddr::from((
                std::net::Ipv4Addr::LOCALHOST,
                self.config.quic_port.unwrap_or(self.config.port),
            ));
            self.quic_server = Some(super::quic::QuicServer::start(
                quic_addr,
                addr,
                &self.ctx.ca,
                self.config.max_connections_per_host,
                Duration::from_secs(self.config.connection_idle_timeout.max(1)),
            )?);
            if let Some(server) = &self.quic_server {
                tracing::info!("HTTP/3 ingress listening on {}", server.local_addr()?);
            }
        }

        tracing::info!("Proxy server listening on {}", addr);

        self.running.store(true, Ordering::SeqCst);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let running = self.running.clone();
        let ctx = self.ctx.clone();

        // Spawn the accept loop
        tokio::spawn(async move {
            tokio::select! {
                _ = Self::accept_loop(listener, ctx, running.clone()) => {
                    tracing::info!("Accept loop ended");
                }
                _ = shutdown_rx => {
                    tracing::info!("Received shutdown signal");
                }
            }
            running.store(false, Ordering::SeqCst);
        });

        Ok(())
    }

    /// Accept loop for incoming connections
    async fn accept_loop(listener: TcpListener, ctx: Arc<ProxyContext>, running: Arc<AtomicBool>) {
        while running.load(Ordering::SeqCst) {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let ctx = ctx.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, peer_addr, ctx).await {
                            tracing::debug!("Connection error from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    if running.load(Ordering::SeqCst) {
                        tracing::error!("Failed to accept connection: {}", e);
                    }
                }
            }
        }
    }

    /// Stop the proxy server
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!("Stopping proxy server...");

        self.running.store(false, Ordering::SeqCst);

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        #[cfg(feature = "quic")]
        if let Some(server) = self.quic_server.take() {
            server.shutdown().await;
        }

        // Give connections time to close
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        tracing::info!("Proxy server stopped");
        Ok(())
    }

    /// Check if the proxy is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the current status
    pub fn status(&self) -> ProxyStatus {
        if self.running.load(Ordering::SeqCst) {
            ProxyStatus::Running
        } else {
            ProxyStatus::Stopped
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &ProxyConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::ProxyConfig;

    #[test]
    fn validates_listener_ports() {
        assert!(ProxyConfig::default().validate().is_ok());

        let config = ProxyConfig {
            port: 0,
            ..ProxyConfig::default()
        };
        assert!(config.validate().is_err());

        let config = ProxyConfig {
            debug_port: 0,
            ..ProxyConfig::default()
        };
        assert!(config.validate().is_err());

        let config = ProxyConfig {
            quic_port: Some(0),
            ..ProxyConfig::default()
        };
        assert!(config.validate().is_err());
    }
}
