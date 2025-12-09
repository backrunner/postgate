use crate::error::{PostGateError, Result};
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

        Ok(())
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
