use super::database::Database;
use crate::error::{PostGateError, Result};

impl Database {
    /// Run database migrations.
    pub(crate) async fn run_migrations(&self) -> Result<()> {
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
        .execute(self.pool())
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
        .execute(self.pool())
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
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

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
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

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
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed: {}", e)))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_captured_requests_timestamp ON captured_requests(timestamp DESC)",
        )
        .execute(self.pool())
        .await
        .ok();

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
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed (plugin_storage): {}", e)))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_plugin_storage_plugin_key ON plugin_storage(plugin_id, key)",
        )
        .execute(self.pool())
        .await
        .ok();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS values_store (
                name TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed (values_store): {}", e)))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                port INTEGER NOT NULL DEFAULT 18999,
                allowed_origins TEXT NOT NULL DEFAULT '[]',
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed (mcp_settings): {}", e)))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_clients (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                token_salt TEXT NOT NULL,
                token_hash TEXT NOT NULL,
                scopes TEXT NOT NULL,
                revoked INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_seen_at INTEGER
            )
            "#,
        )
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed (mcp_clients): {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_mcp_clients_revoked ON mcp_clients(revoked)")
            .execute(self.pool())
            .await
            .ok();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_audit_log (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                client_id TEXT,
                operation TEXT NOT NULL,
                target TEXT,
                allowed INTEGER NOT NULL,
                detail TEXT
            )
            "#,
        )
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Migration failed (mcp_audit_log): {}", e)))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_mcp_audit_timestamp ON mcp_audit_log(timestamp DESC)",
        )
        .execute(self.pool())
        .await
        .ok();

        Ok(())
    }
}
