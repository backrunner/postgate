//! Plugin storage implementation
//!
//! Provides persistent key-value storage for plugins using SQLite.
//! Each plugin has isolated storage that persists across restarts.

use crate::error::{PostGateError, Result};
use crate::plugin::types::PluginState;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;

/// Plugin storage interface for persistent key-value storage
#[derive(Clone)]
pub struct PluginStorage {
    pool: SqlitePool,
    plugin_id: String,
}

impl PluginStorage {
    /// Create a new plugin storage instance
    pub fn new(pool: SqlitePool, plugin_id: String) -> Self {
        Self { pool, plugin_id }
    }

    /// Initialize the plugin storage table
    pub async fn init_table(pool: &SqlitePool) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS plugin_storage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                plugin_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
                UNIQUE(plugin_id, key)
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| {
            PostGateError::Storage(format!("Failed to create plugin_storage table: {}", e))
        })?;

        // Create index for faster lookups
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_plugin_storage_plugin_key ON plugin_storage(plugin_id, key)"
        )
        .execute(pool)
        .await
        .ok(); // Index creation failure is not critical

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS plugin_states (
                plugin_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 0,
                config TEXT NOT NULL DEFAULT '{}',
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| {
            PostGateError::Storage(format!("Failed to create plugin_states table: {}", e))
        })?;

        Ok(())
    }

    /// Get a value from storage
    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM plugin_storage WHERE plugin_id = ? AND key = ?")
                .bind(&self.plugin_id)
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    PostGateError::Storage(format!("Failed to get storage value: {}", e))
                })?;

        match row {
            Some((value_str,)) => {
                let value: serde_json::Value = serde_json::from_str(&value_str).map_err(|e| {
                    PostGateError::Storage(format!("Failed to parse stored value: {}", e))
                })?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Set a value in storage
    pub async fn set(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let value_str = serde_json::to_string(value)
            .map_err(|e| PostGateError::Storage(format!("Failed to serialize value: {}", e)))?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO plugin_storage (plugin_id, key, value, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(plugin_id, key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&self.plugin_id)
        .bind(key)
        .bind(&value_str)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to set storage value: {}", e)))?;

        Ok(())
    }

    /// Delete a value from storage
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM plugin_storage WHERE plugin_id = ? AND key = ?")
            .bind(&self.plugin_id)
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                PostGateError::Storage(format!("Failed to delete storage value: {}", e))
            })?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if a key exists in storage
    pub async fn has(&self, key: &str) -> Result<bool> {
        let row: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM plugin_storage WHERE plugin_id = ? AND key = ?")
                .bind(&self.plugin_id)
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    PostGateError::Storage(format!("Failed to check storage key: {}", e))
                })?;

        Ok(row.is_some())
    }

    /// Get all keys in storage for this plugin
    pub async fn keys(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT key FROM plugin_storage WHERE plugin_id = ? ORDER BY key")
                .bind(&self.plugin_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    PostGateError::Storage(format!("Failed to list storage keys: {}", e))
                })?;

        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    /// Clear all storage for this plugin
    pub async fn clear(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM plugin_storage WHERE plugin_id = ?")
            .bind(&self.plugin_id)
            .execute(&self.pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to clear storage: {}", e)))?;

        Ok(result.rows_affected())
    }

    /// Clear all storage for a specific plugin (static method for cleanup)
    pub async fn clear_plugin_storage(pool: &SqlitePool, plugin_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM plugin_storage WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(pool)
            .await
            .map_err(|e| {
                PostGateError::Storage(format!("Failed to clear plugin storage: {}", e))
            })?;

        Ok(result.rows_affected())
    }

    pub async fn load_plugin_states(pool: &SqlitePool) -> Result<HashMap<String, PluginState>> {
        let rows: Vec<(String, i64, String)> = sqlx::query_as(
            "SELECT plugin_id, enabled, config FROM plugin_states ORDER BY plugin_id",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to load plugin states: {}", e)))?;

        rows.into_iter()
            .map(|(id, enabled, config)| {
                let config = serde_json::from_str(&config).map_err(|e| {
                    PostGateError::Storage(format!(
                        "Failed to parse saved config for plugin {id}: {e}"
                    ))
                })?;
                Ok((
                    id.clone(),
                    PluginState {
                        id,
                        enabled: enabled != 0,
                        config,
                    },
                ))
            })
            .collect()
    }

    pub async fn get_plugin_state(
        pool: &SqlitePool,
        plugin_id: &str,
    ) -> Result<Option<PluginState>> {
        let row: Option<(i64, String)> =
            sqlx::query_as("SELECT enabled, config FROM plugin_states WHERE plugin_id = ?")
                .bind(plugin_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| {
                    PostGateError::Storage(format!("Failed to load plugin state: {}", e))
                })?;

        row.map(|(enabled, config)| {
            let config = serde_json::from_str(&config).map_err(|e| {
                PostGateError::Storage(format!(
                    "Failed to parse saved config for plugin {plugin_id}: {e}"
                ))
            })?;
            Ok(PluginState {
                id: plugin_id.to_string(),
                enabled: enabled != 0,
                config,
            })
        })
        .transpose()
    }

    pub async fn save_plugin_state(pool: &SqlitePool, state: &PluginState) -> Result<()> {
        let config = serde_json::to_string(&state.config).map_err(|e| {
            PostGateError::Storage(format!("Failed to serialize plugin config: {}", e))
        })?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO plugin_states (plugin_id, enabled, config, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(plugin_id) DO UPDATE SET
                enabled = excluded.enabled,
                config = excluded.config,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&state.id)
        .bind(i64::from(state.enabled))
        .bind(config)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save plugin state: {}", e)))?;

        Ok(())
    }

    pub async fn delete_plugin_state(pool: &SqlitePool, plugin_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM plugin_states WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(pool)
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to delete plugin state: {}", e)))?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_storage_operations() {
        let pool = create_test_pool().await;
        PluginStorage::init_table(&pool).await.unwrap();

        let storage = PluginStorage::new(pool, "test-plugin".to_string());

        // Test set and get
        let value = serde_json::json!({"foo": "bar", "count": 42});
        storage.set("test-key", &value).await.unwrap();

        let retrieved = storage.get("test-key").await.unwrap();
        assert_eq!(retrieved, Some(value));

        // Test has
        assert!(storage.has("test-key").await.unwrap());
        assert!(!storage.has("nonexistent").await.unwrap());

        // Test keys
        storage
            .set("another-key", &serde_json::json!("value"))
            .await
            .unwrap();
        let keys = storage.keys().await.unwrap();
        assert_eq!(keys.len(), 2);

        // Test delete
        assert!(storage.delete("test-key").await.unwrap());
        assert!(!storage.has("test-key").await.unwrap());

        // Test clear
        storage.set("key1", &serde_json::json!(1)).await.unwrap();
        storage.set("key2", &serde_json::json!(2)).await.unwrap();
        let cleared = storage.clear().await.unwrap();
        assert!(cleared >= 2);
    }

    #[tokio::test]
    async fn plugin_state_round_trips() {
        let pool = create_test_pool().await;
        PluginStorage::init_table(&pool).await.unwrap();
        let state = PluginState {
            id: "fixture".to_string(),
            enabled: true,
            config: HashMap::from([("mode".to_string(), "mock".to_string())]),
        };

        PluginStorage::save_plugin_state(&pool, &state)
            .await
            .unwrap();
        assert_eq!(
            PluginStorage::get_plugin_state(&pool, "fixture")
                .await
                .unwrap()
                .unwrap()
                .config["mode"],
            "mock"
        );
        assert!(PluginStorage::load_plugin_states(&pool).await.unwrap()["fixture"].enabled);

        PluginStorage::delete_plugin_state(&pool, "fixture")
            .await
            .unwrap();
        assert!(PluginStorage::get_plugin_state(&pool, "fixture")
            .await
            .unwrap()
            .is_none());
    }
}
