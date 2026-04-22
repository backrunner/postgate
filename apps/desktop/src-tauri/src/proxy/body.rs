use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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

/// Body storage for captured requests.
///
/// Backed by `DashMap` so reads/writes don't require `.await` on the hot path.
/// A monotonic insertion counter drives FIFO eviction when we exceed
/// `max_entries`; this is much cheaper than scanning the whole map under a
/// tokio RwLock as the old implementation did.
pub struct BodyStorage {
    request_bodies: Arc<DashMap<String, (u64, CapturedBody)>>,
    response_bodies: Arc<DashMap<String, (u64, CapturedBody)>>,
    counter: AtomicU64,
    max_entries: usize,
}

impl BodyStorage {
    pub fn new(max_entries: usize) -> Self {
        Self {
            request_bodies: Arc::new(DashMap::new()),
            response_bodies: Arc::new(DashMap::new()),
            counter: AtomicU64::new(0),
            max_entries,
        }
    }

    fn next_seq(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Evict ~10% oldest entries when we exceed `max_entries`. Called from
    /// insert paths; runs rarely and off the fast path.
    fn maybe_evict(map: &DashMap<String, (u64, CapturedBody)>, max_entries: usize) {
        if map.len() <= max_entries {
            return;
        }
        let to_remove = map.len() - max_entries + (max_entries / 10).max(1);
        // Collect the N smallest sequence numbers (oldest).
        let mut seqs: Vec<(u64, String)> = map
            .iter()
            .map(|entry| (entry.value().0, entry.key().clone()))
            .collect();
        seqs.sort_unstable_by_key(|(seq, _)| *seq);
        for (_, key) in seqs.into_iter().take(to_remove) {
            map.remove(&key);
        }
    }

    pub async fn store_request_body(&self, id: &str, body: CapturedBody) {
        let seq = self.next_seq();
        self.request_bodies.insert(id.to_string(), (seq, body));
        Self::maybe_evict(&self.request_bodies, self.max_entries);
    }

    pub async fn store_response_body(&self, id: &str, body: CapturedBody) {
        let seq = self.next_seq();
        self.response_bodies.insert(id.to_string(), (seq, body));
        Self::maybe_evict(&self.response_bodies, self.max_entries);
    }

    pub async fn get_request_body(&self, id: &str) -> Option<CapturedBody> {
        self.request_bodies.get(id).map(|e| e.value().1.clone())
    }

    pub async fn get_response_body(&self, id: &str) -> Option<CapturedBody> {
        self.response_bodies.get(id).map(|e| e.value().1.clone())
    }

    pub async fn remove(&self, id: &str) {
        self.request_bodies.remove(id);
        self.response_bodies.remove(id);
    }

    pub async fn clear(&self) {
        self.request_bodies.clear();
        self.response_bodies.clear();
    }
}

impl Default for BodyStorage {
    fn default() -> Self {
        Self::new(10000)
    }
}
