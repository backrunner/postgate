use crate::error::{PostGateError, Result};
use crate::state::CapturedRequestData;
use bytes::Bytes;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration};

/// Threshold for inline body storage (64KB)
const INLINE_BODY_THRESHOLD: usize = 64 * 1024;

/// Captured request storage with SQLite backend
pub struct CapturedRequestStorage {
    pool: SqlitePool,
    bodies_dir: PathBuf,
}

/// Stored captured request (matches database schema)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCapturedRequest {
    pub id: String,
    pub timestamp: i64,
    pub method: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub protocol: String,
    pub request_headers: Option<HashMap<String, String>>,
    pub request_size: u64,
    pub response_status: Option<u16>,
    pub response_headers: Option<HashMap<String, String>>,
    pub response_size: Option<u64>,
    pub content_type: Option<String>,
    pub duration_ms: Option<u64>,
    pub matched_rules: Vec<String>,
    pub error: Option<String>,
    pub tls_version: Option<String>,
    pub remote_addr: Option<String>,
    pub is_complete: bool,
}

/// Paginated result wrapper
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    pub has_more: bool,
}

impl CapturedRequestStorage {
    pub fn new(pool: SqlitePool, data_dir: &Path) -> Self {
        let bodies_dir = data_dir.join("captured_bodies");
        Self { pool, bodies_dir }
    }

    /// Save captured request metadata (without body)
    pub async fn save_request(&self, data: &CapturedRequestData) -> Result<()> {
        let request_headers = data
            .request_headers
            .as_ref()
            .map(|h| serde_json::to_string(h).unwrap_or_default());
        let response_headers = data
            .response_headers
            .as_ref()
            .map(|h| serde_json::to_string(h).unwrap_or_default());
        let matched_rules =
            serde_json::to_string(&data.matched_rules).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO captured_requests
            (id, timestamp, method, url, host, path, protocol, 
             request_headers, request_size, response_status, response_headers, 
             response_size, content_type, duration_ms, matched_rules, 
             error, tls_version, remote_addr, is_complete, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                timestamp = excluded.timestamp,
                method = excluded.method,
                url = excluded.url,
                host = excluded.host,
                path = excluded.path,
                protocol = excluded.protocol,
                request_headers = COALESCE(excluded.request_headers, captured_requests.request_headers),
                request_size = excluded.request_size,
                response_status = excluded.response_status,
                response_headers = excluded.response_headers,
                response_size = excluded.response_size,
                content_type = COALESCE(excluded.content_type, captured_requests.content_type),
                duration_ms = excluded.duration_ms,
                matched_rules = excluded.matched_rules,
                error = excluded.error,
                tls_version = COALESCE(excluded.tls_version, captured_requests.tls_version),
                remote_addr = COALESCE(excluded.remote_addr, captured_requests.remote_addr),
                is_complete = excluded.is_complete
            "#,
        )
        .bind(&data.id)
        .bind(data.timestamp)
        .bind(&data.method)
        .bind(&data.url)
        .bind(&data.host)
        .bind(&data.path)
        .bind(&data.protocol)
        .bind(&request_headers)
        .bind(data.request_size as i64)
        .bind(data.response_status.map(|s| s as i32))
        .bind(&response_headers)
        .bind(data.response_size.map(|s| s as i64))
        .bind(&data.content_type)
        .bind(data.duration_ms.map(|d| d as i64))
        .bind(&matched_rules)
        .bind(&data.error)
        .bind(&data.tls_version)
        .bind(&data.remote_addr)
        .bind(data.response_status.is_some() as i32)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save captured request: {}", e)))?;

        Ok(())
    }

    /// Save body data (inline for small bodies, file for large ones)
    pub async fn save_body(&self, request_id: &str, body: &Bytes, is_request: bool) -> Result<()> {
        if body.len() < INLINE_BODY_THRESHOLD {
            // Inline storage
            let query = if is_request {
                "UPDATE captured_requests SET request_body_inline = ? WHERE id = ?"
            } else {
                "UPDATE captured_requests SET response_body_inline = ? WHERE id = ?"
            };
            self.update_body_column(query, body.to_vec(), request_id)
                .await
                .map_err(|e| {
                    PostGateError::Storage(format!("Failed to save body inline: {}", e))
                })?;
        } else {
            // File storage
            let dir = self.bodies_dir.join(request_id);
            let filename = if is_request {
                "request.bin"
            } else {
                "response.bin"
            };
            let path = dir.join(filename);

            if !self.wait_for_request_row(request_id).await? {
                return Ok(());
            }

            tokio::fs::create_dir_all(&dir).await?;
            tokio::fs::write(&path, body.as_ref()).await?;

            let query = if is_request {
                "UPDATE captured_requests SET request_body_path = ? WHERE id = ?"
            } else {
                "UPDATE captured_requests SET response_body_path = ? WHERE id = ?"
            };
            let updated = self
                .update_body_column(query, path.to_string_lossy().to_string(), request_id)
                .await
                .map_err(|e| PostGateError::Storage(format!("Failed to save body path: {}", e)))?;
            if !updated {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }

        Ok(())
    }

    async fn update_body_column<'a, T>(
        &self,
        query: &'static str,
        value: T,
        request_id: &'a str,
    ) -> std::result::Result<bool, sqlx::Error>
    where
        T: sqlx::Encode<'a, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Clone + Send + 'a,
    {
        // Body jobs can race ahead of the metadata worker. Retry briefly so
        // persistence stays best-effort without coupling body IO to the proxy
        // hot path.
        for attempt in 0..3 {
            let result = sqlx::query(query)
                .bind(value.clone())
                .bind(request_id)
                .execute(&self.pool)
                .await?;
            if result.rows_affected() > 0 {
                return Ok(true);
            }
            if attempt < 2 {
                sleep(Duration::from_millis(50)).await;
            }
        }
        Ok(false)
    }

    async fn wait_for_request_row(
        &self,
        request_id: &str,
    ) -> std::result::Result<bool, sqlx::Error> {
        for attempt in 0..3 {
            let exists: Option<(String,)> =
                sqlx::query_as("SELECT id FROM captured_requests WHERE id = ?")
                    .bind(request_id)
                    .fetch_optional(&self.pool)
                    .await?;
            if exists.is_some() {
                return Ok(true);
            }
            if attempt < 2 {
                sleep(Duration::from_millis(50)).await;
            }
        }
        Ok(false)
    }

    /// Get paginated captured requests
    pub async fn get_requests_paginated(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<PaginatedResult<StoredCapturedRequest>> {
        let offset = (page - 1) * page_size;

        // Get total count
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM captured_requests")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to count: {}", e)))?;

        // Get paginated data
        let rows = sqlx::query_as::<_, CapturedRequestRow>(
            r#"
            SELECT id, timestamp, method, url, host, path, protocol,
                   request_headers, request_size, response_status, response_headers,
                   response_size, content_type, duration_ms, matched_rules,
                   error, tls_version, remote_addr, is_complete
            FROM captured_requests
            ORDER BY timestamp DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch: {}", e)))?;

        let items = rows.into_iter().map(|r| r.into()).collect();

        Ok(PaginatedResult {
            items,
            total: total.0,
            page,
            page_size,
            has_more: (offset + page_size) < total.0 as i32,
        })
    }

    /// Get a captured request by ID.
    pub async fn get_request(&self, id: &str) -> Result<Option<StoredCapturedRequest>> {
        let row = sqlx::query_as::<_, CapturedRequestRow>(
            r#"
            SELECT id, timestamp, method, url, host, path, protocol,
                   request_headers, request_size, response_status, response_headers,
                   response_size, content_type, duration_ms, matched_rules,
                   error, tls_version, remote_addr, is_complete
            FROM captured_requests
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch request: {}", e)))?;

        Ok(row.map(Into::into))
    }

    /// Get body data
    pub async fn get_body(&self, request_id: &str, is_request: bool) -> Result<Option<Bytes>> {
        let query = if is_request {
            "SELECT request_body_inline, request_body_path FROM captured_requests WHERE id = ?"
        } else {
            "SELECT response_body_inline, response_body_path FROM captured_requests WHERE id = ?"
        };

        let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as(query)
            .bind(request_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to get body: {}", e)))?;

        match row {
            Some((Some(inline_data), _)) => Ok(Some(Bytes::from(inline_data))),
            Some((None, Some(path))) => {
                let data = tokio::fs::read(&path).await?;
                Ok(Some(Bytes::from(data)))
            }
            _ => Ok(None),
        }
    }

    /// Clear all captured requests
    pub async fn clear_all(&self) -> Result<()> {
        sqlx::query("DELETE FROM captured_requests")
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to clear: {}", e)))?;

        // Clean up files
        if self.bodies_dir.exists() {
            tokio::fs::remove_dir_all(&self.bodies_dir).await?;
        }

        Ok(())
    }

    /// Clear records before specified timestamp
    pub async fn clear_before(&self, before_timestamp: i64) -> Result<u64> {
        // Get IDs to delete (for file cleanup)
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM captured_requests WHERE timestamp < ?")
                .bind(before_timestamp)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| PostGateError::Storage(format!("Failed to query: {}", e)))?;

        // Delete files
        for (id,) in &ids {
            let dir = self.bodies_dir.join(id);
            if dir.exists() {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
        }

        // Delete database records
        let result = sqlx::query("DELETE FROM captured_requests WHERE timestamp < ?")
            .bind(before_timestamp)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to delete: {}", e)))?;

        Ok(result.rows_affected())
    }

    /// Get count of captured requests
    pub async fn count(&self) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM captured_requests")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to count: {}", e)))?;
        Ok(count)
    }
}

#[derive(sqlx::FromRow)]
struct CapturedRequestRow {
    id: String,
    timestamp: i64,
    method: String,
    url: String,
    host: String,
    path: String,
    protocol: String,
    request_headers: Option<String>,
    request_size: i64,
    response_status: Option<i32>,
    response_headers: Option<String>,
    response_size: Option<i64>,
    content_type: Option<String>,
    duration_ms: Option<i64>,
    matched_rules: Option<String>,
    error: Option<String>,
    tls_version: Option<String>,
    remote_addr: Option<String>,
    is_complete: i32,
}

impl From<CapturedRequestRow> for StoredCapturedRequest {
    fn from(row: CapturedRequestRow) -> Self {
        Self {
            id: row.id,
            timestamp: row.timestamp,
            method: row.method,
            url: row.url,
            host: row.host,
            path: row.path,
            protocol: row.protocol,
            request_headers: row
                .request_headers
                .and_then(|s| serde_json::from_str(&s).ok()),
            request_size: row.request_size as u64,
            response_status: row.response_status.map(|s| s as u16),
            response_headers: row
                .response_headers
                .and_then(|s| serde_json::from_str(&s).ok()),
            response_size: row.response_size.map(|s| s as u64),
            content_type: row.content_type,
            duration_ms: row.duration_ms.map(|d| d as u64),
            matched_rules: row
                .matched_rules
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            error: row.error,
            tls_version: row.tls_version,
            remote_addr: row.remote_addr,
            is_complete: row.is_complete != 0,
        }
    }
}
