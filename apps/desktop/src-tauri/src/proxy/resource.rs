//! Remote rule resource fetch/cache.
//!
//! Whistle allows `file://`, `resBody://`, and `mock://` to point at
//! `http(s)` resources. The rule applicator stays synchronous; proxy code
//! fetches those resources ahead of rule application and passes the bytes in
//! via `ResolveCtx`.

use crate::proxy::body::{collect_body, MAX_BODY_SIZE};
use crate::proxy::upstream::SharedClient;
use crate::rules::{ResolvedResource, ResolvedResources};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::header::CONTENT_TYPE;
use hyper::{Method, Request};
use moka::sync::Cache;
use std::time::Duration;

const REMOTE_RESOURCE_TTL: Duration = Duration::from_secs(30);
const REMOTE_RESOURCE_TIMEOUT: Duration = Duration::from_secs(10);
const REMOTE_RESOURCE_CACHE_CAPACITY: u64 = 128;

#[derive(Clone)]
pub struct RemoteResourceCache {
    cache: Cache<String, ResolvedResource>,
}

impl RemoteResourceCache {
    pub fn new() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(REMOTE_RESOURCE_CACHE_CAPACITY)
                .time_to_live(REMOTE_RESOURCE_TTL)
                .build(),
        }
    }

    pub async fn fetch_all(&self, client: &SharedClient, urls: &[String]) -> ResolvedResources {
        let mut resolved = ResolvedResources::new();
        for url in urls {
            if let Some(resource) = self.cache.get(url) {
                resolved.insert(url.clone(), resource);
                continue;
            }

            match self.fetch_one(client, url).await {
                Ok(resource) => {
                    self.cache.insert(url.clone(), resource.clone());
                    resolved.insert(url.clone(), resource);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch remote rule resource {}: {}", url, e);
                }
            }
        }
        resolved
    }

    async fn fetch_one(
        &self,
        client: &SharedClient,
        url: &str,
    ) -> crate::error::Result<ResolvedResource> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(url)
            .body(Full::new(Bytes::new()).map_err(|_| unreachable!()).boxed())
            .map_err(|e| {
                crate::error::PostGateError::Proxy(format!(
                    "Failed to build remote resource request: {}",
                    e
                ))
            })?;

        let response = match tokio::time::timeout(REMOTE_RESOURCE_TIMEOUT, client.request(request))
            .await
        {
            Ok(result) => result.map_err(|e| {
                crate::error::PostGateError::Proxy(format!("Remote resource request failed: {}", e))
            })?,
            Err(_) => {
                return Err(crate::error::PostGateError::Proxy(format!(
                    "Remote resource request timed out after {} ms",
                    REMOTE_RESOURCE_TIMEOUT.as_millis()
                )));
            }
        };

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let captured = match tokio::time::timeout(
            REMOTE_RESOURCE_TIMEOUT,
            collect_body(response.into_body(), MAX_BODY_SIZE),
        )
        .await
        {
            Ok(result) => result.map_err(|e| {
                crate::error::PostGateError::Proxy(format!(
                    "Failed to read remote resource response: {}",
                    e
                ))
            })?,
            Err(_) => {
                return Err(crate::error::PostGateError::Proxy(format!(
                    "Remote resource body timed out after {} ms",
                    REMOTE_RESOURCE_TIMEOUT.as_millis()
                )));
            }
        };

        if captured.truncated {
            tracing::warn!(
                "Remote rule resource {} exceeded {} bytes; using truncated body",
                url,
                MAX_BODY_SIZE
            );
        }

        Ok(ResolvedResource {
            body: captured.data,
            content_type,
        })
    }
}

impl Default for RemoteResourceCache {
    fn default() -> Self {
        Self::new()
    }
}
