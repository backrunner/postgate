use super::{
    CaptureBodyEncoding, CaptureBodyInput, CaptureBodyResult, CaptureBodySide, CaptureBodySource,
    CaptureSearchInput, CaptureSearchResult, PostGateApi,
};
use crate::capture_index::{capture_matches, CaptureIndexQuery};
use crate::error::Result;
use crate::state::CapturedRequestData;
use crate::storage::StoredCapturedRequest;
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const PERSISTED_SCAN_PAGE_SIZE: i32 = 500;

impl PostGateApi {
    pub async fn search_captures(&self, input: CaptureSearchInput) -> Result<CaptureSearchResult> {
        let offset = input
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or(0);
        let limit = input.limit.unwrap_or(50).clamp(1, 500);
        let query = CaptureIndexQuery {
            search: input.search,
            methods: input.methods,
            hosts: input.hosts,
            protocols: input.protocols,
            status_codes: input.status_codes,
            content_types: input.content_types,
            has_rules: input.has_rules,
            since: input.since,
            until: input.until,
            offset: 0,
            limit,
        };

        let mut merged = self.state.capture_index.matching(&query);
        let mut seen: HashSet<_> = merged.iter().map(|item| item.id.clone()).collect();

        let storage = self.state.get_captured_storage().await?;
        let mut page = 1;
        loop {
            let persisted = storage
                .get_requests_paginated(page, PERSISTED_SCAN_PAGE_SIZE)
                .await?;
            for stored in persisted.items {
                if seen.contains(&stored.id) {
                    continue;
                }
                let data = stored_to_capture_data(stored);
                if !capture_matches(&data, &query) {
                    continue;
                }
                seen.insert(data.id.clone());
                merged.push(data);
            }

            if !persisted.has_more {
                break;
            }
            page += 1;
        }

        merged.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.id.cmp(&a.id)));

        let total = merged.len();
        let start = offset.min(total);
        let mut items = merged
            .into_iter()
            .skip(start)
            .take(limit)
            .collect::<Vec<_>>();

        if input.redact {
            for item in &mut items {
                redact_capture_headers(item);
            }
        }

        let next_offset = start + items.len();
        let has_more = next_offset < total;
        Ok(CaptureSearchResult {
            total,
            has_more,
            cursor: has_more.then(|| next_offset.to_string()),
            items,
        })
    }

    pub async fn get_capture(&self, id: &str, redact: bool) -> Result<Option<CapturedRequestData>> {
        if let Some(mut data) = self.state.capture_index.get(id) {
            if redact {
                redact_capture_headers(&mut data);
            }
            return Ok(Some(data));
        }

        let storage = self.state.get_captured_storage().await?;
        let mut data = storage.get_request(id).await?.map(stored_to_capture_data);
        if redact {
            if let Some(data) = &mut data {
                redact_capture_headers(data);
            }
        }
        Ok(data)
    }

    pub async fn get_capture_body(
        &self,
        input: CaptureBodyInput,
    ) -> Result<Option<CaptureBodyResult>> {
        let is_request = matches!(input.side, CaptureBodySide::Request);
        let max_bytes = input
            .max_bytes
            .unwrap_or(1024 * 1024)
            .clamp(1, 10 * 1024 * 1024);

        let memory = async {
            if is_request {
                self.state.body_storage.get_request_body(&input.id).await
            } else {
                self.state.body_storage.get_response_body(&input.id).await
            }
        };

        let (source, body, original_size, captured_truncated) = match input.source {
            CaptureBodySource::Memory => match memory.await {
                Some(body) => ("memory", body.data.to_vec(), body.size, body.truncated),
                None => return Ok(None),
            },
            CaptureBodySource::Persisted => {
                let storage = self.state.get_captured_storage().await?;
                match storage.get_body(&input.id, is_request).await? {
                    Some(body) => {
                        let len = body.len();
                        ("persisted", body.to_vec(), len, false)
                    }
                    None => return Ok(None),
                }
            }
            CaptureBodySource::Auto => {
                if let Some(body) = memory.await {
                    ("memory", body.data.to_vec(), body.size, body.truncated)
                } else {
                    let storage = self.state.get_captured_storage().await?;
                    match storage.get_body(&input.id, is_request).await? {
                        Some(body) => {
                            let len = body.len();
                            ("persisted", body.to_vec(), len, false)
                        }
                        None => return Ok(None),
                    }
                }
            }
        };

        let mut content_type = self
            .state
            .capture_index
            .get(&input.id)
            .and_then(|data| data.content_type);
        if content_type.is_none() {
            let storage = self.state.get_captured_storage().await?;
            content_type = storage
                .get_request(&input.id)
                .await?
                .and_then(|stored| stored.content_type);
        }

        let limited_len = body.len().min(max_bytes);
        let limited = &body[..limited_len];
        let max_truncated = limited_len < body.len();
        let sha256 = Sha256::digest(&body);
        let sha256 = sha256
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect::<String>();

        let (encoding, content) =
            encode_body_content(limited, content_type.as_deref(), &input.encoding);

        Ok(Some(CaptureBodyResult {
            id: input.id.clone(),
            side: if is_request { "request" } else { "response" }.to_string(),
            source: source.to_string(),
            content_type,
            size: original_size,
            captured_bytes: limited_len,
            truncated: captured_truncated || max_truncated,
            encoding,
            content,
            sha256,
            redacted: false,
        }))
    }

    pub async fn clear_capture_history(&self) -> Result<()> {
        self.state.body_storage.clear().await;
        self.state.capture_index.clear();
        let storage = self.state.get_captured_storage().await?;
        storage.clear_all().await?;
        Ok(())
    }
}

pub(crate) fn stored_to_capture_data(stored: StoredCapturedRequest) -> CapturedRequestData {
    CapturedRequestData {
        id: stored.id,
        timestamp: stored.timestamp,
        method: stored.method,
        url: stored.url,
        host: stored.host,
        path: stored.path,
        request_headers: stored.request_headers,
        response_status: stored.response_status,
        response_headers: stored.response_headers,
        duration_ms: stored.duration_ms,
        matched_rules: stored.matched_rules,
        protocol: stored.protocol,
        content_type: stored.content_type,
        request_size: stored.request_size,
        response_size: stored.response_size,
        error: stored.error,
        tls_version: stored.tls_version,
        remote_addr: stored.remote_addr,
    }
}

fn redact_capture_headers(data: &mut CapturedRequestData) {
    if let Some(headers) = &mut data.request_headers {
        redact_headers(headers);
    }
    if let Some(headers) = &mut data.response_headers {
        redact_headers(headers);
    }
}

fn redact_headers(headers: &mut HashMap<String, String>) {
    for (name, value) in headers.iter_mut() {
        if is_sensitive_header(name) {
            *value = "[redacted]".to_string();
        }
    }
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "cookie" | "set-cookie" | "proxy-authorization"
    )
}

fn encode_body_content(
    bytes: &[u8],
    content_type: Option<&str>,
    requested: &CaptureBodyEncoding,
) -> (String, String) {
    match requested {
        CaptureBodyEncoding::Utf8 => (
            "utf8".to_string(),
            String::from_utf8_lossy(bytes).to_string(),
        ),
        CaptureBodyEncoding::Base64 => (
            "base64".to_string(),
            general_purpose::STANDARD.encode(bytes),
        ),
        CaptureBodyEncoding::Auto => {
            if looks_textual(content_type) {
                return (
                    "utf8".to_string(),
                    String::from_utf8_lossy(bytes).to_string(),
                );
            }
            match std::str::from_utf8(bytes) {
                Ok(text) => ("utf8".to_string(), text.to_string()),
                Err(_) => (
                    "base64".to_string(),
                    general_purpose::STANDARD.encode(bytes),
                ),
            }
        }
    }
}

fn looks_textual(content_type: Option<&str>) -> bool {
    let Some(content_type) = content_type else {
        return false;
    };
    let content_type = content_type.to_ascii_lowercase();
    content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
        || content_type.contains("html")
        || content_type.contains("css")
        || content_type.contains("form")
}
