use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::proxy::handler::{handle_connection, ProxyContext};
use crate::proxy::pool::{ConnectionPool, PoolConfig};
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
    /// Maximum connections per upstream host
    #[serde(rename = "maxConnectionsPerHost", default = "default_max_connections")]
    pub max_connections_per_host: usize,
    /// Connection idle timeout in seconds
    #[serde(rename = "connectionIdleTimeout", default = "default_idle_timeout")]
    pub connection_idle_timeout: u64,
}

fn default_max_connections() -> usize { 10 }
fn default_idle_timeout() -> u64 { 60 }

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            port: 8899,
            enable_http2: true,
            enable_quic: false,
            quic_port: None,
            max_connections_per_host: default_max_connections(),
            connection_idle_timeout: default_idle_timeout(),
        }
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
    cleanup_handle: Option<tokio::task::JoinHandle<()>>,
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
        // Create connection pool with config
        let pool_config = PoolConfig {
            max_connections_per_host: config.max_connections_per_host,
            idle_timeout: Duration::from_secs(config.connection_idle_timeout),
            max_lifetime: Duration::from_secs(300),
        };
        
        let connection_pool = ConnectionPool::new(pool_config)
            .expect("Failed to create connection pool");

        let ctx = Arc::new(ProxyContext {
            ca: Arc::new(ca),
            rule_engine,
            body_storage,
            app_state,
            connection_pool: Arc::new(connection_pool),
            enable_http2: config.enable_http2,
        });

        Self {
            config,
            ctx,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            cleanup_handle: None,
        }
    }

    /// Start the proxy server
    pub async fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(PostGateError::InvalidState("Proxy already running".into()));
        }

        let addr: SocketAddr = format!("127.0.0.1:{}", self.config.port)
            .parse()
            .map_err(|e| PostGateError::Proxy(format!("Invalid address: {}", e)))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Failed to bind to {}: {}", addr, e)))?;

        tracing::info!("Proxy server listening on {}", addr);

        self.running.store(true, Ordering::SeqCst);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let running = self.running.clone();
        let ctx = self.ctx.clone();

        // Start connection pool cleanup task
        let pool = self.ctx.connection_pool.clone();
        self.cleanup_handle = Some(super::pool::start_cleanup_task(
            pool,
            Duration::from_secs(30),
        ));

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
    async fn accept_loop(
        listener: TcpListener,
        ctx: Arc<ProxyContext>,
        running: Arc<AtomicBool>,
    ) {
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

        // Stop cleanup task
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }

        // Clear connection pool
        self.ctx.connection_pool.clear().await;

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

    /// Get connection pool statistics
    pub fn pool_stats(&self) -> &super::pool::PoolStats {
        self.ctx.connection_pool.stats()
    }
}
