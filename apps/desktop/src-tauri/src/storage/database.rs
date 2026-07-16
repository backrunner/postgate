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

    /// Get the pool reference
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Save a rule group
    pub async fn save_rule_group(&self, group: &RuleGroup) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO rule_groups (id, name, folder, enabled, priority, raw_content, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&group.id)
        .bind(&group.name)
        .bind(&group.folder)
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
            SELECT id, name, folder, enabled, priority, raw_content, created_at, updated_at
            FROM rule_groups
            ORDER BY priority ASC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch rule groups: {}", e)))?;

        let groups = rows
            .into_iter()
            .map(|row| {
                let (rules, inline_values) =
                    crate::rules::parse_rules_with_external_includes(&row.raw_content, None)
                        .unwrap_or_default();
                RuleGroup {
                    id: row.id,
                    name: row.name,
                    folder: row.folder,
                    enabled: row.enabled != 0,
                    priority: row.priority,
                    rules,
                    raw_content: row.raw_content,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    inline_values,
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

    /// Remove every rule group. Used by full-profile restores.
    pub async fn clear_rule_groups(&self) -> Result<()> {
        sqlx::query("DELETE FROM rule_groups")
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to clear rule groups: {}", e)))?;

        Ok(())
    }

    // ==================== Values Store Methods ====================

    /// List all stored values (ordered by name).
    pub async fn list_values(&self) -> Result<Vec<crate::values::ValueEntry>> {
        let rows = sqlx::query_as::<_, ValueRow>(
            "SELECT name, content, created_at, updated_at FROM values_store ORDER BY name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch values: {}", e)))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Fetch a single value by name.
    pub async fn get_value(&self, name: &str) -> Result<Option<crate::values::ValueEntry>> {
        let row = sqlx::query_as::<_, ValueRow>(
            "SELECT name, content, created_at, updated_at FROM values_store WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch value: {}", e)))?;

        Ok(row.map(Into::into))
    }

    /// Insert or replace a value; returns the persisted row.
    pub async fn upsert_value(
        &self,
        name: &str,
        content: &str,
    ) -> Result<crate::values::ValueEntry> {
        let now = chrono::Utc::now().timestamp_millis();

        // Preserve created_at if the row already exists.
        let created_at = match self.get_value(name).await? {
            Some(existing) => existing.created_at,
            None => now,
        };

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO values_store (name, content, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(name)
        .bind(content)
        .bind(created_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save value: {}", e)))?;

        Ok(crate::values::ValueEntry {
            name: name.to_string(),
            content: content.to_string(),
            created_at,
            updated_at: now,
        })
    }

    /// Delete a value by name; returns true if a row was removed.
    pub async fn delete_value(&self, name: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM values_store WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to delete value: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    /// Remove every stored value. Used by full-profile restores.
    pub async fn clear_values(&self) -> Result<()> {
        sqlx::query("DELETE FROM values_store")
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to clear values: {}", e)))?;

        Ok(())
    }

    /// Rename a value. Fails if `new_name` already exists.
    pub async fn rename_value(
        &self,
        old_name: &str,
        new_name: &str,
    ) -> Result<crate::values::ValueEntry> {
        if old_name == new_name {
            return self
                .get_value(old_name)
                .await?
                .ok_or_else(|| PostGateError::Storage(format!("Value '{}' not found", old_name)));
        }

        if self.get_value(new_name).await?.is_some() {
            return Err(PostGateError::Storage(format!(
                "Value '{}' already exists",
                new_name
            )));
        }

        let existing = self
            .get_value(old_name)
            .await?
            .ok_or_else(|| PostGateError::Storage(format!("Value '{}' not found", old_name)))?;

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE values_store SET name = ?, updated_at = ? WHERE name = ?")
            .bind(new_name)
            .bind(now)
            .bind(old_name)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to rename value: {}", e)))?;

        Ok(crate::values::ValueEntry {
            name: new_name.to_string(),
            content: existing.content,
            created_at: existing.created_at,
            updated_at: now,
        })
    }

    // ==================== Collection Methods ====================

    /// Get all collections
    pub async fn get_collections(&self) -> Result<Vec<Collection>> {
        let rows = sqlx::query_as::<_, CollectionRow>(
            "SELECT id, name, parent_id, created_at, updated_at FROM collections ORDER BY name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch collections: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| Collection {
                id: r.id,
                name: r.name,
                parent_id: r.parent_id,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    /// Get a single collection
    pub async fn get_collection(&self, id: &str) -> Result<Option<Collection>> {
        let row = sqlx::query_as::<_, CollectionRow>(
            "SELECT id, name, parent_id, created_at, updated_at FROM collections WHERE id = ?",
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

    /// Remove every collection and saved request. Used by full-profile restores.
    pub async fn clear_replay_data(&self) -> Result<()> {
        sqlx::query("DELETE FROM request_history")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                PostGateError::Storage(format!("Failed to clear request history: {}", e))
            })?;

        sqlx::query("DELETE FROM saved_requests")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                PostGateError::Storage(format!("Failed to clear saved requests: {}", e))
            })?;

        sqlx::query("DELETE FROM collections")
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to clear collections: {}", e)))?;

        Ok(())
    }

    /// Delete collection and all children recursively
    pub fn delete_collection_recursive<'a>(
        &'a self,
        id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Get all child collections
            let children =
                sqlx::query_scalar::<_, String>("SELECT id FROM collections WHERE parent_id = ?")
                    .bind(id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| {
                        PostGateError::Storage(format!("Failed to fetch child collections: {}", e))
                    })?;

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

        Ok(rows.into_iter().filter_map(parse_saved_request).collect())
    }

    /// Get requests in a specific collection
    pub async fn get_requests_in_collection(
        &self,
        collection_id: &str,
    ) -> Result<Vec<SavedRequest>> {
        let rows = sqlx::query_as::<_, SavedRequestRow>(
            "SELECT id, name, collection_id, method, url, headers, query_params, body_type, body_content, created_at, updated_at FROM saved_requests WHERE collection_id = ? ORDER BY name ASC"
        )
        .bind(collection_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch requests: {}", e)))?;

        Ok(rows.into_iter().filter_map(parse_saved_request).collect())
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

        Ok(row.and_then(parse_saved_request))
    }

    /// Save a request
    pub async fn save_request(&self, request: &SavedRequest) -> Result<()> {
        let headers_json = serde_json::to_string(&request.headers)
            .map_err(|e| PostGateError::Storage(format!("Failed to serialize headers: {}", e)))?;
        let query_params_json = serde_json::to_string(&request.query_params).map_err(|e| {
            PostGateError::Storage(format!("Failed to serialize query params: {}", e))
        })?;
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
        let response_json = history
            .response
            .as_ref()
            .map(serde_json::to_string)
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

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let request = serde_json::from_str(&r.request_json).ok()?;
                let response = r
                    .response_json
                    .as_ref()
                    .and_then(|j| serde_json::from_str(j).ok());

                Some(RequestHistory {
                    id: r.id,
                    saved_request_id: r.saved_request_id,
                    request,
                    response,
                    error: r.error,
                    executed_at: r.executed_at,
                })
            })
            .collect())
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
    folder: Option<String>,
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
    #[sqlx(rename = "body_type")]
    _body_type: String,
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
    let body = row
        .body_content
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

#[derive(sqlx::FromRow)]
struct ValueRow {
    name: String,
    content: String,
    created_at: i64,
    updated_at: i64,
}

impl From<ValueRow> for crate::values::ValueEntry {
    fn from(row: ValueRow) -> Self {
        crate::values::ValueEntry {
            name: row.name,
            content: row.content,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
