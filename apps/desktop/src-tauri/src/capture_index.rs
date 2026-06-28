use crate::state::CapturedRequestData;
use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::VecDeque;

pub const DEFAULT_CAPTURE_INDEX_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Default)]
pub struct CaptureIndexQuery {
    pub search: Option<String>,
    pub methods: Vec<String>,
    pub hosts: Vec<String>,
    pub protocols: Vec<String>,
    pub status_codes: Vec<u16>,
    pub content_types: Vec<String>,
    pub has_rules: Option<bool>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct CaptureIndexPage {
    pub items: Vec<CapturedRequestData>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

pub struct CaptureIndex {
    order: Mutex<VecDeque<String>>,
    entries: DashMap<String, CapturedRequestData>,
    max_entries: usize,
}

impl CaptureIndex {
    pub fn new(max_entries: usize) -> Self {
        Self {
            order: Mutex::new(VecDeque::new()),
            entries: DashMap::new(),
            max_entries,
        }
    }

    pub fn record(&self, data: CapturedRequestData) {
        let id = data.id.clone();
        let is_new = !self.entries.contains_key(&id);
        self.entries.insert(id.clone(), data);

        if !is_new {
            return;
        }

        let mut order = self.order.lock();
        order.push_front(id);
        while order.len() > self.max_entries {
            if let Some(removed) = order.pop_back() {
                self.entries.remove(&removed);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<CapturedRequestData> {
        self.entries.get(id).map(|entry| entry.clone())
    }

    pub fn clear(&self) {
        self.entries.clear();
        self.order.lock().clear();
    }

    pub fn search(&self, mut query: CaptureIndexQuery) -> CaptureIndexPage {
        if query.limit == 0 {
            query.limit = 50;
        }
        query.limit = query.limit.min(500);

        let matches = self.matching(&query);

        let total = matches.len();
        let items = matches
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect::<Vec<_>>();
        let has_more = query.offset + items.len() < total;

        CaptureIndexPage {
            items,
            total,
            offset: query.offset,
            limit: query.limit,
            has_more,
        }
    }

    pub fn matching(&self, query: &CaptureIndexQuery) -> Vec<CapturedRequestData> {
        let order = self.order.lock().clone();
        order
            .iter()
            .filter_map(|id| self.entries.get(id).map(|entry| entry.clone()))
            .filter(|data| capture_matches(data, query))
            .collect()
    }
}

impl Default for CaptureIndex {
    fn default() -> Self {
        Self::new(DEFAULT_CAPTURE_INDEX_LIMIT)
    }
}

pub(crate) fn capture_matches(data: &CapturedRequestData, query: &CaptureIndexQuery) -> bool {
    if let Some(since) = query.since {
        if data.timestamp < since {
            return false;
        }
    }
    if let Some(until) = query.until {
        if data.timestamp > until {
            return false;
        }
    }

    if !query.methods.is_empty()
        && !query
            .methods
            .iter()
            .any(|method| method.eq_ignore_ascii_case(&data.method))
    {
        return false;
    }

    if !query.hosts.is_empty()
        && !query
            .hosts
            .iter()
            .any(|host| data.host.eq_ignore_ascii_case(host) || data.host.contains(host))
    {
        return false;
    }

    if !query.protocols.is_empty()
        && !query
            .protocols
            .iter()
            .any(|protocol| data.protocol.eq_ignore_ascii_case(protocol))
    {
        return false;
    }

    if !query.status_codes.is_empty() {
        match data.response_status {
            Some(status) if query.status_codes.contains(&status) => {}
            _ => return false,
        }
    }

    if !query.content_types.is_empty() {
        let content_type = data.content_type.as_deref().unwrap_or_default();
        if !query.content_types.iter().any(|expected| {
            content_type
                .to_lowercase()
                .contains(&expected.to_lowercase())
        }) {
            return false;
        }
    }

    if let Some(has_rules) = query.has_rules {
        if data.matched_rules.is_empty() == has_rules {
            return false;
        }
    }

    if let Some(search) = &query.search {
        let needle = search.to_lowercase();
        if !data.url.to_lowercase().contains(&needle)
            && !data.host.to_lowercase().contains(&needle)
            && !data.path.to_lowercase().contains(&needle)
            && !data.method.to_lowercase().contains(&needle)
        {
            return false;
        }
    }

    true
}
