use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Maximum body size to capture (10MB)
pub const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

/// Default byte budget for the in-memory body cache (500 MB, split between
/// request and response maps via `maybe_evict`). Chosen to sit comfortably
/// under typical desktop RAM headroom while still holding minutes of
/// realistic capture: ~500 KB/response × 1000 entries = 500 MB.
///
/// Compare to the previous cap: 10,000 entries × up to 10 MB each = 100 GB
/// worst-case before eviction ever triggered.
pub const DEFAULT_BODY_BUDGET_BYTES: usize = 500 * 1024 * 1024;
/// Default hard cap on entry count — still useful as a belt-and-braces for
/// pathological flows of many tiny responses.
pub const DEFAULT_MAX_ENTRIES: usize = 20_000;

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
/// either the entry-count cap or the byte budget; tracking both is cheap
/// (one `AtomicUsize` add per insert / sub per remove) and protects us
/// against the two failure modes of a pure count cap: huge bodies blowing
/// RAM, and tiny bodies gaming a byte budget into an unbounded entry count.
pub struct BodyStorage {
    request_bodies: Arc<DashMap<String, (u64, CapturedBody)>>,
    response_bodies: Arc<DashMap<String, (u64, CapturedBody)>>,
    counter: AtomicU64,
    max_entries: usize,
    /// Total bytes currently retained across both request + response maps
    /// (counting the captured slice, not the upstream body size when
    /// truncated). Shared so the byte budget applies to the pool as a whole
    /// rather than per-map — otherwise a surge of one direction could
    /// silently double our actual footprint.
    total_bytes: Arc<AtomicUsize>,
    max_total_bytes: usize,
}

impl BodyStorage {
    pub fn new(max_entries: usize) -> Self {
        Self::with_limits(max_entries, DEFAULT_BODY_BUDGET_BYTES)
    }

    pub fn with_limits(max_entries: usize, max_total_bytes: usize) -> Self {
        Self {
            request_bodies: Arc::new(DashMap::new()),
            response_bodies: Arc::new(DashMap::new()),
            counter: AtomicU64::new(0),
            max_entries,
            total_bytes: Arc::new(AtomicUsize::new(0)),
            max_total_bytes,
        }
    }

    fn next_seq(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Current live bytes across both request + response maps.
    pub fn current_bytes(&self) -> usize {
        self.total_bytes.load(Ordering::Relaxed)
    }

    /// Insert a body under a fresh sequence number and, if necessary, evict
    /// older entries to stay within both the entry and byte budgets.
    fn insert(&self, map: &DashMap<String, (u64, CapturedBody)>, id: &str, body: CapturedBody) {
        let seq = self.next_seq();
        let new_size = body.data.len();

        // If we're replacing an existing entry, credit its bytes back first
        // so the running total stays accurate.
        if let Some((_, (_, old))) = map.remove(id) {
            self.total_bytes
                .fetch_sub(old.data.len(), Ordering::Relaxed);
        }

        self.total_bytes.fetch_add(new_size, Ordering::Relaxed);
        map.insert(id.to_string(), (seq, body));

        self.maybe_evict();
    }

    /// Evict oldest entries across BOTH maps when either the entry-count
    /// cap or the byte budget is exceeded. We look at the pool as a whole
    /// (not per-map) because body sizes are wildly skewed toward responses,
    /// and a per-map budget would either waste the request-side budget or
    /// under-protect the response-side one.
    fn maybe_evict(&self) {
        let req = &self.request_bodies;
        let resp = &self.response_bodies;

        loop {
            let total_entries = req.len() + resp.len();
            let total_bytes = self.total_bytes.load(Ordering::Relaxed);
            let over_entries = total_entries > self.max_entries;
            let over_bytes = total_bytes > self.max_total_bytes;

            if !over_entries && !over_bytes {
                return;
            }

            // Collect (seq, is_request, key) for the oldest entries across
            // both maps. We only need enough to make meaningful progress —
            // drop ~10% of the cap, or 32 entries, whichever is larger.
            let target_drop = (self.max_entries / 10).max(32);

            let mut victims: Vec<(u64, bool, String)> = Vec::with_capacity(total_entries);
            for entry in req.iter() {
                victims.push((entry.value().0, true, entry.key().clone()));
            }
            for entry in resp.iter() {
                victims.push((entry.value().0, false, entry.key().clone()));
            }
            if victims.len() > target_drop {
                // We only need the oldest batch, not a fully sorted list.
                // `select_nth_unstable_by_key` keeps eviction O(n) instead of
                // O(n log n) when the cache hovers around its byte budget.
                victims.select_nth_unstable_by_key(target_drop, |(seq, _, _)| *seq);
                victims.truncate(target_drop);
            } else {
                victims.sort_unstable_by_key(|(seq, _, _)| *seq);
            }

            let mut dropped = 0usize;
            for (_, is_req, key) in victims {
                let map = if is_req { req } else { resp };
                if let Some((_, (_, body))) = map.remove(&key) {
                    self.total_bytes
                        .fetch_sub(body.data.len(), Ordering::Relaxed);
                }
                dropped += 1;

                // Stop as soon as we're under both limits again, or we've
                // dropped our target batch. Re-checking avoids evicting
                // more than necessary for large bodies.
                let now_bytes = self.total_bytes.load(Ordering::Relaxed);
                let now_entries = req.len() + resp.len();
                if now_bytes <= self.max_total_bytes
                    && now_entries <= self.max_entries
                    && dropped >= target_drop.min(1)
                {
                    return;
                }
                if dropped >= target_drop {
                    break;
                }
            }

            // Safety: if we somehow can't make progress (both maps empty
            // but total_bytes is non-zero due to a bug), reset the counter
            // rather than loop forever.
            if req.is_empty() && resp.is_empty() {
                self.total_bytes.store(0, Ordering::Relaxed);
                return;
            }
        }
    }

    pub async fn store_request_body(&self, id: &str, body: CapturedBody) {
        self.insert(&self.request_bodies, id, body);
    }

    pub async fn store_response_body(&self, id: &str, body: CapturedBody) {
        self.insert(&self.response_bodies, id, body);
    }

    pub async fn get_request_body(&self, id: &str) -> Option<CapturedBody> {
        self.request_bodies.get(id).map(|e| e.value().1.clone())
    }

    pub async fn get_response_body(&self, id: &str) -> Option<CapturedBody> {
        self.response_bodies.get(id).map(|e| e.value().1.clone())
    }

    pub async fn remove(&self, id: &str) {
        if let Some((_, (_, body))) = self.request_bodies.remove(id) {
            self.total_bytes
                .fetch_sub(body.data.len(), Ordering::Relaxed);
        }
        if let Some((_, (_, body))) = self.response_bodies.remove(id) {
            self.total_bytes
                .fetch_sub(body.data.len(), Ordering::Relaxed);
        }
    }

    pub async fn clear(&self) {
        self.request_bodies.clear();
        self.response_bodies.clear();
        self.total_bytes.store(0, Ordering::Relaxed);
    }
}

impl Default for BodyStorage {
    fn default() -> Self {
        Self::with_limits(DEFAULT_MAX_ENTRIES, DEFAULT_BODY_BUDGET_BYTES)
    }
}
