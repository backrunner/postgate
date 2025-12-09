//! Connection pool for upstream servers
//!
//! This module provides connection pooling to improve performance by reusing
//! TCP and TLS connections to upstream servers.

use crate::error::{PostGateError, Result};
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use super::tls::{create_tls_connector, parse_server_name};

/// Default maximum connections per host
const DEFAULT_MAX_CONNECTIONS_PER_HOST: usize = 10;

/// Default connection idle timeout
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Default connection max lifetime
const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(300);

/// Pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum connections per host
    pub max_connections_per_host: usize,
    /// How long an idle connection can stay in the pool
    pub idle_timeout: Duration,
    /// Maximum lifetime of a connection
    pub max_lifetime: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_host: DEFAULT_MAX_CONNECTIONS_PER_HOST,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            max_lifetime: DEFAULT_MAX_LIFETIME,
        }
    }
}

/// A pooled connection wrapper
pub struct PooledConnection<T> {
    inner: Option<T>,
    created_at: Instant,
    last_used: Instant,
    key: String,
    pool: Arc<ConnectionPool>,
}

impl<T> PooledConnection<T> {
    /// Get a reference to the inner connection
    pub fn inner(&self) -> &T {
        self.inner.as_ref().expect("Connection already taken")
    }

    /// Get a mutable reference to the inner connection
    pub fn inner_mut(&mut self) -> &mut T {
        self.inner.as_mut().expect("Connection already taken")
    }

    /// Take the inner connection (prevents returning to pool)
    pub fn take(mut self) -> T {
        self.inner.take().expect("Connection already taken")
    }

    /// Check if the connection is still valid
    pub fn is_valid(&self, config: &PoolConfig) -> bool {
        let now = Instant::now();
        now.duration_since(self.last_used) < config.idle_timeout
            && now.duration_since(self.created_at) < config.max_lifetime
    }
}

impl<T> Drop for PooledConnection<T> {
    fn drop(&mut self) {
        // Return connection to pool if it wasn't taken
        if let Some(conn) = self.inner.take() {
            // We can't easily return generic connections to the pool
            // This is handled by specific pool implementations
        }
    }
}

/// Entry in the connection pool
struct PoolEntry<T> {
    connection: T,
    created_at: Instant,
    last_used: Instant,
}

impl<T> PoolEntry<T> {
    fn new(connection: T) -> Self {
        let now = Instant::now();
        Self {
            connection,
            created_at: now,
            last_used: now,
        }
    }

    fn is_valid(&self, config: &PoolConfig) -> bool {
        let now = Instant::now();
        now.duration_since(self.last_used) < config.idle_timeout
            && now.duration_since(self.created_at) < config.max_lifetime
    }

    fn touch(&mut self) {
        self.last_used = Instant::now();
    }
}

/// Pool for a single host
struct HostPool<T> {
    connections: Mutex<VecDeque<PoolEntry<T>>>,
    active_count: AtomicUsize,
}

impl<T> HostPool<T> {
    fn new() -> Self {
        Self {
            connections: Mutex::new(VecDeque::new()),
            active_count: AtomicUsize::new(0),
        }
    }
}

/// Connection pool for HTTP connections
pub struct ConnectionPool {
    config: PoolConfig,
    tls_connector: TlsConnector,
    // Pool for plain TCP connections
    tcp_pools: DashMap<String, Arc<HostPool<TcpStream>>>,
    // Pool for TLS connections
    tls_pools: DashMap<String, Arc<HostPool<TlsStream<TcpStream>>>>,
    // Statistics
    stats: PoolStats,
}

/// Pool statistics
#[derive(Default)]
pub struct PoolStats {
    pub hits: AtomicUsize,
    pub misses: AtomicUsize,
    pub connections_created: AtomicUsize,
    pub connections_reused: AtomicUsize,
    pub connections_expired: AtomicUsize,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(config: PoolConfig) -> Result<Self> {
        let tls_connector = create_tls_connector()?;
        
        Ok(Self {
            config,
            tls_connector,
            tcp_pools: DashMap::new(),
            tls_pools: DashMap::new(),
            stats: PoolStats::default(),
        })
    }

    /// Create a pool with default configuration
    pub fn with_defaults() -> Result<Self> {
        Self::new(PoolConfig::default())
    }

    /// Get or create a TCP connection to a host
    pub async fn get_tcp(&self, host: &str, port: u16) -> Result<TcpStream> {
        let key = format!("{}:{}", host, port);

        // Try to get from pool
        let pool = self.tcp_pools
            .entry(key.clone())
            .or_insert_with(|| Arc::new(HostPool::new()))
            .clone();

        // Try to get an existing connection
        {
            let mut connections = pool.connections.lock().await;
            while let Some(mut entry) = connections.pop_front() {
                if entry.is_valid(&self.config) {
                    entry.touch();
                    self.stats.hits.fetch_add(1, Ordering::Relaxed);
                    self.stats.connections_reused.fetch_add(1, Ordering::Relaxed);
                    return Ok(entry.connection);
                } else {
                    self.stats.connections_expired.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Create new connection
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        self.stats.connections_created.fetch_add(1, Ordering::Relaxed);
        
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

        // Configure TCP options
        stream.set_nodelay(true).ok();

        Ok(stream)
    }

    /// Return a TCP connection to the pool
    pub async fn return_tcp(&self, host: &str, port: u16, stream: TcpStream) {
        let key = format!("{}:{}", host, port);

        let pool = self.tcp_pools
            .entry(key)
            .or_insert_with(|| Arc::new(HostPool::new()))
            .clone();

        let mut connections = pool.connections.lock().await;
        
        if connections.len() < self.config.max_connections_per_host {
            connections.push_back(PoolEntry::new(stream));
        }
        // If pool is full, connection is dropped
    }

    /// Get or create a TLS connection to a host
    pub async fn get_tls(&self, host: &str, port: u16) -> Result<TlsStream<TcpStream>> {
        let key = format!("tls:{}:{}", host, port);

        // Try to get from pool
        let pool = self.tls_pools
            .entry(key.clone())
            .or_insert_with(|| Arc::new(HostPool::new()))
            .clone();

        // Try to get an existing connection
        {
            let mut connections = pool.connections.lock().await;
            while let Some(mut entry) = connections.pop_front() {
                if entry.is_valid(&self.config) {
                    entry.touch();
                    self.stats.hits.fetch_add(1, Ordering::Relaxed);
                    self.stats.connections_reused.fetch_add(1, Ordering::Relaxed);
                    return Ok(entry.connection);
                } else {
                    self.stats.connections_expired.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Create new connection
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        self.stats.connections_created.fetch_add(1, Ordering::Relaxed);

        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| PostGateError::Proxy(format!("Failed to connect to {}: {}", addr, e)))?;

        stream.set_nodelay(true).ok();

        let server_name = parse_server_name(host)?;
        let tls_stream = self.tls_connector
            .connect(server_name, stream)
            .await
            .map_err(|e| PostGateError::Proxy(format!("TLS handshake failed: {}", e)))?;

        Ok(tls_stream)
    }

    /// Return a TLS connection to the pool
    pub async fn return_tls(&self, host: &str, port: u16, stream: TlsStream<TcpStream>) {
        let key = format!("tls:{}:{}", host, port);

        let pool = self.tls_pools
            .entry(key)
            .or_insert_with(|| Arc::new(HostPool::new()))
            .clone();

        let mut connections = pool.connections.lock().await;
        
        if connections.len() < self.config.max_connections_per_host {
            connections.push_back(PoolEntry::new(stream));
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// Clear all connections from the pool
    pub async fn clear(&self) {
        self.tcp_pools.clear();
        self.tls_pools.clear();
    }

    /// Remove expired connections from all pools
    pub async fn cleanup(&self) {
        // Cleanup TCP pools
        for entry in self.tcp_pools.iter() {
            let pool = entry.value();
            let mut connections = pool.connections.lock().await;
            let before = connections.len();
            connections.retain(|e| e.is_valid(&self.config));
            let removed = before - connections.len();
            if removed > 0 {
                self.stats.connections_expired.fetch_add(removed, Ordering::Relaxed);
            }
        }

        // Cleanup TLS pools
        for entry in self.tls_pools.iter() {
            let pool = entry.value();
            let mut connections = pool.connections.lock().await;
            let before = connections.len();
            connections.retain(|e| e.is_valid(&self.config));
            let removed = before - connections.len();
            if removed > 0 {
                self.stats.connections_expired.fetch_add(removed, Ordering::Relaxed);
            }
        }
    }

    /// Get the total number of pooled connections
    pub async fn pooled_count(&self) -> usize {
        let mut count = 0;
        
        for entry in self.tcp_pools.iter() {
            let pool = entry.value();
            count += pool.connections.lock().await.len();
        }
        
        for entry in self.tls_pools.iter() {
            let pool = entry.value();
            count += pool.connections.lock().await.len();
        }
        
        count
    }
}

/// Start a background task to periodically clean up expired connections
pub fn start_cleanup_task(pool: Arc<ConnectionPool>, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(interval);
        loop {
            interval_timer.tick().await;
            pool.cleanup().await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_creation() {
        let pool = ConnectionPool::with_defaults();
        assert!(pool.is_ok());
    }

    #[tokio::test]
    async fn test_pool_config() {
        let config = PoolConfig {
            max_connections_per_host: 5,
            idle_timeout: Duration::from_secs(30),
            max_lifetime: Duration::from_secs(120),
        };
        let pool = ConnectionPool::new(config.clone());
        assert!(pool.is_ok());
    }
}
