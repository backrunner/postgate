use bytes::{Bytes, BytesMut};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Maximum body size to capture (10MB)
pub const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

/// Captured body data with metadata
#[derive(Debug, Clone)]
pub struct CapturedBody {
    pub data: Bytes,
    pub size: usize,
    pub truncated: bool,
}

impl CapturedBody {
    pub fn empty() -> Self {
        Self {
            data: Bytes::new(),
            size: 0,
            truncated: false,
        }
    }

    pub fn new(data: Bytes, truncated: bool) -> Self {
        let size = data.len();
        Self {
            data,
            size,
            truncated,
        }
    }
}

/// Collect body from an Incoming stream with size limit
pub async fn collect_body(body: Incoming, max_size: usize) -> Result<CapturedBody, hyper::Error> {
    let mut collected = BytesMut::new();
    let mut truncated = false;
    let mut total_size = 0usize;

    let mut body = body;

    while let Some(frame) = body.frame().await {
        let frame = frame?;
        if let Some(chunk) = frame.data_ref() {
            total_size += chunk.len();
            
            if collected.len() < max_size {
                let remaining = max_size - collected.len();
                if chunk.len() <= remaining {
                    collected.extend_from_slice(chunk);
                } else {
                    collected.extend_from_slice(&chunk[..remaining]);
                    truncated = true;
                }
            } else {
                truncated = true;
            }
        }
    }

    Ok(CapturedBody {
        data: collected.freeze(),
        size: total_size,
        truncated,
    })
}

/// Body storage for captured requests
pub struct BodyStorage {
    /// Request bodies indexed by request ID
    request_bodies: Arc<RwLock<std::collections::HashMap<String, CapturedBody>>>,
    /// Response bodies indexed by request ID
    response_bodies: Arc<RwLock<std::collections::HashMap<String, CapturedBody>>>,
    /// Maximum number of bodies to store
    max_entries: usize,
}

impl BodyStorage {
    pub fn new(max_entries: usize) -> Self {
        Self {
            request_bodies: Arc::new(RwLock::new(std::collections::HashMap::new())),
            response_bodies: Arc::new(RwLock::new(std::collections::HashMap::new())),
            max_entries,
        }
    }

    pub async fn store_request_body(&self, id: &str, body: CapturedBody) {
        let mut bodies = self.request_bodies.write().await;
        
        // Evict old entries if needed
        if bodies.len() >= self.max_entries {
            // Remove oldest entries (simple FIFO)
            let to_remove: Vec<_> = bodies.keys().take(100).cloned().collect();
            for key in to_remove {
                bodies.remove(&key);
            }
        }
        
        bodies.insert(id.to_string(), body);
    }

    pub async fn store_response_body(&self, id: &str, body: CapturedBody) {
        let mut bodies = self.response_bodies.write().await;
        
        if bodies.len() >= self.max_entries {
            let to_remove: Vec<_> = bodies.keys().take(100).cloned().collect();
            for key in to_remove {
                bodies.remove(&key);
            }
        }
        
        bodies.insert(id.to_string(), body);
    }

    pub async fn get_request_body(&self, id: &str) -> Option<CapturedBody> {
        self.request_bodies.read().await.get(id).cloned()
    }

    pub async fn get_response_body(&self, id: &str) -> Option<CapturedBody> {
        self.response_bodies.read().await.get(id).cloned()
    }

    pub async fn remove(&self, id: &str) {
        self.request_bodies.write().await.remove(id);
        self.response_bodies.write().await.remove(id);
    }

    pub async fn clear(&self) {
        self.request_bodies.write().await.clear();
        self.response_bodies.write().await.clear();
    }
}

impl Default for BodyStorage {
    fn default() -> Self {
        Self::new(10000)
    }
}
