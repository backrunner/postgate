use crate::error::{PostGateError, Result};
use crate::replay::{Collection, RequestHistory, SavedRequest};
use crate::rules::RuleGroup;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;

/// Database wrapper for persistent storage
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub async fn new(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let database_url = format!("sqlite:{}?mode=rwc", path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to connect to database: {}", e)))?;

        let db = Self { pool };
        db.run_migrations().await?;

        Ok(db)
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS rule_groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                enabled INTEGER DEFAULT 1,
                priority INTEGER DEFAULT 0,
                raw_content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS saved_requests (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                collection_id TEXT,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                headers TEXT NOT NULL,
                query_params TEXT NOT NULL,
                body_type TEXT NOT NULL,
                body_content TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                parent_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

        // Request history table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS request_history (
                id TEXT PRIMARY KEY,
                saved_request_id TEXT,
                request_json TEXT NOT NULL,
                response_json TEXT,
                error TEXT,
                executed_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

        // Captured requests table for proxy traffic persistence
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS captured_requests (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                host TEXT NOT NULL,
                path TEXT NOT NULL,
                protocol TEXT NOT NULL,
                request_headers TEXT,
                request_size INTEGER NOT NULL,
                request_body_inline BLOB,
                request_body_path TEXT,
                response_status INTEGER,
                response_headers TEXT,
                response_size INTEGER,
                response_body_inline BLOB,
                response_body_path TEXT,
                content_type TEXT,
                duration_ms INTEGER,
                matched_rules TEXT,
                error TEXT,
                tls_version TEXT,
                remote_addr TEXT,
                is_complete INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

        // Index for timestamp ordering
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_captured_requests_timestamp ON captured_requests(timestamp DESC)"
        )
        .execute(&self.pool)
        .await
        .ok(); // Index creation failure should not block startup

        Ok(())
    }

    /// Get the pool reference
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Save a rule group
    pub async fn save_rule_group(&self, group: &RuleGroup) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO rule_groups (id, name, enabled, priority, raw_content, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&group.id)
        .bind(&group.name)
        .bind(group.enabled as i32)
        .bind(group.priority)
        .bind(&group.raw_content)
        .bind(group.created_at)
        .bind(group.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save rule group: {}", e)))?;

        Ok(())
    }

    /// Get all rule groups
    pub async fn get_rule_groups(&self) -> Result<Vec<RuleGroup>> {
        let rows = sqlx::query_as::<_, RuleGroupRow>(
            r#"
            SELECT id, name, enabled, priority, raw_content, created_at, updated_at
            FROM rule_groups
            ORDER BY priority DESC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch rule groups: {}", e)))?;

        let groups = rows
            .into_iter()
            .map(|row| {
                let rules = crate::rules::parse_rules(&row.raw_content).unwrap_or_default();
                RuleGroup {
                    id: row.id,
                    name: row.name,
                    enabled: row.enabled != 0,
                    priority: row.priority,
                    rules,
                    raw_content: row.raw_content,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                }
            })
            .collect();

        Ok(groups)
    }

    /// Delete a rule group
    pub async fn delete_rule_group(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM rule_groups WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to delete rule group: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    /// Get a single rule group
    pub async fn get_rule_group(&self, id: &str) -> Result<Option<RuleGroup>> {
        let row = sqlx::query_as::<_, RuleGroupRow>(
            r#"
            SELECT id, name, enabled, priority, raw_content, created_at, updated_at
            FROM rule_groups
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch rule group: {}", e)))?;

        Ok(row.map(|row| {
            let rules = crate::rules::parse_rules(&row.raw_content).unwrap_or_default();
            RuleGroup {
                id: row.id,
                name: row.name,
                enabled: row.enabled != 0,
                priority: row.priority,
                rules,
                raw_content: row.raw_content,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        }))
    }

    // ==================== Collection Methods ====================

    /// Get all collections
    pub async fn get_collections(&self) -> Result<Vec<Collection>> {
        let rows = sqlx::query_as::<_, CollectionRow>(
            "SELECT id, name, parent_id, created_at, updated_at FROM collections ORDER BY name ASC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch collections: {}", e)))?;

        Ok(rows.into_iter().map(|r| Collection {
            id: r.id,
            name: r.name,
            parent_id: r.parent_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }).collect())
    }

    /// Get a single collection
    pub async fn get_collection(&self, id: &str) -> Result<Option<Collection>> {
        let row = sqlx::query_as::<_, CollectionRow>(
            "SELECT id, name, parent_id, created_at, updated_at FROM collections WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch collection: {}", e)))?;

        Ok(row.map(|r| Collection {
            id: r.id,
            name: r.name,
            parent_id: r.parent_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    /// Save a collection
    pub async fn save_collection(&self, collection: &Collection) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO collections (id, name, parent_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&collection.id)
        .bind(&collection.name)
        .bind(&collection.parent_id)
        .bind(collection.created_at)
        .bind(collection.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save collection: {}", e)))?;

        Ok(())
    }

    /// Delete a collection
    pub async fn delete_collection(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to delete collection: {}", e)))?;

        Ok(())
    }

    /// Delete collection and all children recursively
    pub fn delete_collection_recursive<'a>(&'a self, id: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Get all child collections
            let children = sqlx::query_scalar::<_, String>(
                "SELECT id FROM collections WHERE parent_id = ?"
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to fetch child collections: {}", e)))?;

            // Recursively delete children
            for child_id in children {
                self.delete_requests_in_collection(&child_id).await?;
                self.delete_collection_recursive(&child_id).await?;
                self.delete_collection(&child_id).await?;
            }

            Ok(())
        })
    }

    /// Delete all requests in a collection
    pub async fn delete_requests_in_collection(&self, collection_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM saved_requests WHERE collection_id = ?")
            .bind(collection_id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to delete requests: {}", e)))?;

        Ok(())
    }

    /// Move collection contents to root (set parent to null)
    pub async fn move_collection_contents_to_root(&self, collection_id: &str) -> Result<()> {
        // Move requests to root
        sqlx::query("UPDATE saved_requests SET collection_id = NULL WHERE collection_id = ?")
            .bind(collection_id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to move requests: {}", e)))?;

        // Move child collections to root
        sqlx::query("UPDATE collections SET parent_id = NULL WHERE parent_id = ?")
            .bind(collection_id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to move collections: {}", e)))?;

        Ok(())
    }

    // ==================== Saved Request Methods ====================

    /// Get all saved requests
    pub async fn get_saved_requests(&self) -> Result<Vec<SavedRequest>> {
        let rows = sqlx::query_as::<_, SavedRequestRow>(
            "SELECT id, name, collection_id, method, url, headers, query_params, body_type, body_content, created_at, updated_at FROM saved_requests ORDER BY name ASC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch requests: {}", e)))?;

        Ok(rows.into_iter().filter_map(|r| parse_saved_request(r)).collect())
    }

    /// Get requests in a specific collection
    pub async fn get_requests_in_collection(&self, collection_id: &str) -> Result<Vec<SavedRequest>> {
        let rows = sqlx::query_as::<_, SavedRequestRow>(
            "SELECT id, name, collection_id, method, url, headers, query_params, body_type, body_content, created_at, updated_at FROM saved_requests WHERE collection_id = ? ORDER BY name ASC"
        )
        .bind(collection_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch requests: {}", e)))?;

        Ok(rows.into_iter().filter_map(|r| parse_saved_request(r)).collect())
    }

    /// Get a single saved request
    pub async fn get_saved_request(&self, id: &str) -> Result<Option<SavedRequest>> {
        let row = sqlx::query_as::<_, SavedRequestRow>(
            "SELECT id, name, collection_id, method, url, headers, query_params, body_type, body_content, created_at, updated_at FROM saved_requests WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch request: {}", e)))?;

        Ok(row.and_then(|r| parse_saved_request(r)))
    }

    /// Save a request
    pub async fn save_request(&self, request: &SavedRequest) -> Result<()> {
        let headers_json = serde_json::to_string(&request.headers)
            .map_err(|e| PostGateError::Storage(format!("Failed to serialize headers: {}", e)))?;
        let query_params_json = serde_json::to_string(&request.query_params)
            .map_err(|e| PostGateError::Storage(format!("Failed to serialize query params: {}", e)))?;
        let body_json = serde_json::to_string(&request.body)
            .map_err(|e| PostGateError::Storage(format!("Failed to serialize body: {}", e)))?;

        sqlx::query(
            "INSERT OR REPLACE INTO saved_requests (id, name, collection_id, method, url, headers, query_params, body_type, body_content, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'json', ?, ?, ?)"
        )
        .bind(&request.id)
        .bind(&request.name)
        .bind(&request.collection_id)
        .bind(&request.method)
        .bind(&request.url)
        .bind(&headers_json)
        .bind(&query_params_json)
        .bind(&body_json)
        .bind(request.created_at)
        .bind(request.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save request: {}", e)))?;

        Ok(())
    }

    /// Delete a request
    pub async fn delete_request(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM saved_requests WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to delete request: {}", e)))?;

        Ok(())
    }

    /// Move a request to a different collection
    pub async fn move_request(&self, request_id: &str, collection_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE saved_requests SET collection_id = ?, updated_at = ? WHERE id = ?")
            .bind(collection_id)
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(request_id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to move request: {}", e)))?;

        Ok(())
    }

    // ==================== History Methods ====================

    /// Save a history entry
    pub async fn save_history(&self, history: &RequestHistory) -> Result<()> {
        let request_json = serde_json::to_string(&history.request)
            .map_err(|e| PostGateError::Storage(format!("Failed to serialize request: {}", e)))?;
        let response_json = history.response.as_ref()
            .map(|r| serde_json::to_string(r))
            .transpose()
            .map_err(|e| PostGateError::Storage(format!("Failed to serialize response: {}", e)))?;

        sqlx::query(
            "INSERT INTO request_history (id, saved_request_id, request_json, response_json, error, executed_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&history.id)
        .bind(&history.saved_request_id)
        .bind(&request_json)
        .bind(&response_json)
        .bind(&history.error)
        .bind(history.executed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save history: {}", e)))?;

        Ok(())
    }

    /// Get request history
    pub async fn get_history(&self, limit: i32) -> Result<Vec<RequestHistory>> {
        let rows = sqlx::query_as::<_, HistoryRow>(
            "SELECT id, saved_request_id, request_json, response_json, error, executed_at FROM request_history ORDER BY executed_at DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch history: {}", e)))?;

        Ok(rows.into_iter().filter_map(|r| {
            let request = serde_json::from_str(&r.request_json).ok()?;
            let response = r.response_json.as_ref()
                .and_then(|j| serde_json::from_str(j).ok());

            Some(RequestHistory {
                id: r.id,
                saved_request_id: r.saved_request_id,
                request,
                response,
                error: r.error,
                executed_at: r.executed_at,
            })
        }).collect())
    }

    /// Clear all history
    pub async fn clear_history(&self) -> Result<()> {
        sqlx::query("DELETE FROM request_history")
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to clear history: {}", e)))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct RuleGroupRow {
    id: String,
    name: String,
    enabled: i32,
    priority: i32,
    raw_content: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct CollectionRow {
    id: String,
    name: String,
    parent_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct SavedRequestRow {
    id: String,
    name: String,
    collection_id: Option<String>,
    method: String,
    url: String,
    headers: String,
    query_params: String,
    body_type: String,
    body_content: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct HistoryRow {
    id: String,
    saved_request_id: Option<String>,
    request_json: String,
    response_json: Option<String>,
    error: Option<String>,
    executed_at: i64,
}

fn parse_saved_request(row: SavedRequestRow) -> Option<SavedRequest> {
    let headers = serde_json::from_str(&row.headers).ok()?;
    let query_params = serde_json::from_str(&row.query_params).ok()?;
    let body = row.body_content
        .as_ref()
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or(crate::replay::RequestBody::None);

    Some(SavedRequest {
        id: row.id,
        name: row.name,
        collection_id: row.collection_id,
        method: row.method,
        url: row.url,
        headers,
        query_params,
        body,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
