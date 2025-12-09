//! Speed throttling for request/response streams
//!
//! Implements bandwidth limiting for proxy traffic to simulate slow connections.

use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::time::Sleep;

/// Throttle configuration
#[derive(Debug, Clone)]
pub struct ThrottleConfig {
    /// Bytes per second limit (0 = unlimited)
    pub bytes_per_second: u64,
    /// Minimum chunk size to emit
    pub min_chunk_size: usize,
}

impl ThrottleConfig {
    /// Create from kbps (kilobits per second)
    pub fn from_kbps(kbps: u64) -> Self {
        Self {
            // kbps to bytes per second: kbps * 1000 / 8
            bytes_per_second: kbps * 125,
            min_chunk_size: 1024, // 1KB minimum chunk
        }
    }

    /// Check if throttling is enabled
    pub fn is_enabled(&self) -> bool {
        self.bytes_per_second > 0
    }
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            bytes_per_second: 0, // Unlimited
            min_chunk_size: 1024,
        }
    }
}

/// Throttled body wrapper that limits bandwidth
pub struct ThrottledBody {
    inner: Bytes,
    config: ThrottleConfig,
    position: usize,
    bytes_sent_this_second: u64,
    second_start: Instant,
    pending_sleep: Option<Pin<Box<Sleep>>>,
}

impl ThrottledBody {
    /// Create a new throttled body
    pub fn new(body: Bytes, config: ThrottleConfig) -> Self {
        Self {
            inner: body,
            config,
            position: 0,
            bytes_sent_this_second: 0,
            second_start: Instant::now(),
            pending_sleep: None,
        }
    }

    /// Get the next chunk respecting throttle limits
    pub fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<Bytes>> {
        // Check if we have a pending sleep
        if let Some(ref mut sleep) = self.pending_sleep {
            match Pin::as_mut(sleep).poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(()) => {
                    self.pending_sleep = None;
                }
            }
        }

        // Check if we're done
        if self.position >= self.inner.len() {
            return Poll::Ready(None);
        }

        // No throttling
        if !self.config.is_enabled() {
            let chunk = self.inner.slice(self.position..);
            self.position = self.inner.len();
            return Poll::Ready(Some(chunk));
        }

        // Reset counter if we're in a new second
        let elapsed = self.second_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.bytes_sent_this_second = 0;
            self.second_start = Instant::now();
        }

        // Check if we've exceeded the limit for this second
        if self.bytes_sent_this_second >= self.config.bytes_per_second {
            // Calculate how long to wait
            let wait_time = Duration::from_secs(1) - elapsed;
            if wait_time > Duration::ZERO {
                self.pending_sleep = Some(Box::pin(tokio::time::sleep(wait_time)));
                return self.poll_chunk(cx);
            } else {
                // Start a new second
                self.bytes_sent_this_second = 0;
                self.second_start = Instant::now();
            }
        }

        // Calculate how much we can send
        let remaining_bandwidth = self.config.bytes_per_second - self.bytes_sent_this_second;
        let remaining_data = self.inner.len() - self.position;
        let chunk_size = remaining_bandwidth
            .min(remaining_data as u64)
            .max(self.config.min_chunk_size as u64) as usize;
        let chunk_size = chunk_size.min(remaining_data);

        // Get the chunk
        let end = self.position + chunk_size;
        let chunk = self.inner.slice(self.position..end);
        self.position = end;
        self.bytes_sent_this_second += chunk_size as u64;

        Poll::Ready(Some(chunk))
    }

    /// Check if there's more data
    pub fn is_end(&self) -> bool {
        self.position >= self.inner.len()
    }
}

/// Async throttled read function
pub async fn throttled_send(body: Bytes, kbps: u64) -> ThrottledSender {
    ThrottledSender::new(body, ThrottleConfig::from_kbps(kbps))
}

/// Helper struct for sending throttled data
pub struct ThrottledSender {
    body: ThrottledBody,
}

impl ThrottledSender {
    pub fn new(body: Bytes, config: ThrottleConfig) -> Self {
        Self {
            body: ThrottledBody::new(body, config),
        }
    }

    /// Get the next chunk, waiting if necessary for throttling
    pub async fn next_chunk(&mut self) -> Option<Bytes> {
        if self.body.is_end() {
            return None;
        }

        std::future::poll_fn(|cx| self.body.poll_chunk(cx)).await
    }

    /// Consume all chunks and return them combined
    pub async fn collect(mut self) -> Bytes {
        let mut result = Vec::with_capacity(self.body.inner.len());
        while let Some(chunk) = self.next_chunk().await {
            result.extend_from_slice(&chunk);
        }
        Bytes::from(result)
    }
}

/// Apply throttling to a body if configured
pub async fn apply_throttle(body: Bytes, kbps: Option<u64>) -> Bytes {
    match kbps {
        Some(kbps) if kbps > 0 => {
            let sender = ThrottledSender::new(body, ThrottleConfig::from_kbps(kbps));
            sender.collect().await
        }
        _ => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throttle_config_from_kbps() {
        let config = ThrottleConfig::from_kbps(100);
        // 100 kbps = 100 * 1000 / 8 = 12500 bytes/sec
        assert_eq!(config.bytes_per_second, 12500);
    }

    #[test]
    fn test_throttle_disabled() {
        let config = ThrottleConfig::default();
        assert!(!config.is_enabled());
    }

    #[tokio::test]
    async fn test_throttled_small_body() {
        let body = Bytes::from("Hello, World!");
        let result = apply_throttle(body.clone(), Some(1000)).await;
        assert_eq!(result, body);
    }

    #[tokio::test]
    async fn test_no_throttle() {
        let body = Bytes::from("Hello, World!");
        let result = apply_throttle(body.clone(), None).await;
        assert_eq!(result, body);
    }
}
