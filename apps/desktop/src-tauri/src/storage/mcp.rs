use super::database::Database;
use crate::error::{PostGateError, Result};
use crate::mcp::{McpAuditEvent, McpClient, McpClientAuthRecord, McpSettings};

impl Database {
    pub async fn get_mcp_settings(&self) -> Result<McpSettings> {
        let row = sqlx::query_as::<_, McpSettingsRow>(
            "SELECT enabled, port, allowed_origins, updated_at FROM mcp_settings WHERE id = 1",
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch MCP settings: {}", e)))?;

        Ok(row.map(Into::into).unwrap_or_default())
    }

    pub async fn save_mcp_settings(&self, settings: &McpSettings) -> Result<McpSettings> {
        let mut settings = settings.clone();
        settings.updated_at = chrono::Utc::now().timestamp_millis();
        let allowed_origins = serde_json::to_string(&settings.allowed_origins)?;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO mcp_settings (id, enabled, port, allowed_origins, updated_at)
            VALUES (1, ?, ?, ?, ?)
            "#,
        )
        .bind(settings.enabled as i32)
        .bind(settings.port as i32)
        .bind(&allowed_origins)
        .bind(settings.updated_at)
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save MCP settings: {}", e)))?;

        Ok(settings)
    }

    pub async fn get_mcp_clients(&self) -> Result<Vec<McpClient>> {
        let rows = sqlx::query_as::<_, McpClientRow>(
            r#"
            SELECT id, name, token_salt, token_hash, scopes, revoked,
                   created_at, updated_at, last_seen_at
            FROM mcp_clients
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch MCP clients: {}", e)))?;

        rows.into_iter().map(|row| row.try_into_client()).collect()
    }

    pub async fn get_mcp_client_auth_records(&self) -> Result<Vec<McpClientAuthRecord>> {
        let rows = sqlx::query_as::<_, McpClientRow>(
            r#"
            SELECT id, name, token_salt, token_hash, scopes, revoked,
                   created_at, updated_at, last_seen_at
            FROM mcp_clients
            WHERE revoked = 0
            "#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch MCP auth records: {}", e)))?;

        rows.into_iter()
            .map(McpClientRow::try_into_auth_record)
            .collect()
    }

    pub async fn get_mcp_client(&self, id: &str) -> Result<Option<McpClient>> {
        let row = sqlx::query_as::<_, McpClientRow>(
            r#"
            SELECT id, name, token_salt, token_hash, scopes, revoked,
                   created_at, updated_at, last_seen_at
            FROM mcp_clients
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch MCP client: {}", e)))?;

        row.map(|row| row.try_into_client()).transpose()
    }

    pub async fn save_mcp_client_auth_record(&self, record: &McpClientAuthRecord) -> Result<()> {
        let scopes = serde_json::to_string(&record.client.scopes)?;
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO mcp_clients
            (id, name, token_salt, token_hash, scopes, revoked, created_at, updated_at, last_seen_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.client.id)
        .bind(&record.client.name)
        .bind(&record.token_salt)
        .bind(&record.token_hash)
        .bind(&scopes)
        .bind(record.client.revoked as i32)
        .bind(record.client.created_at)
        .bind(record.client.updated_at)
        .bind(record.client.last_seen_at)
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save MCP client: {}", e)))?;

        Ok(())
    }

    pub async fn revoke_mcp_client(&self, id: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE mcp_clients SET revoked = 1, updated_at = ? WHERE id = ? AND revoked = 0",
        )
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to revoke MCP client: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn update_mcp_client_scopes(&self, id: &str, scopes: &[String]) -> Result<bool> {
        let scopes = serde_json::to_string(scopes)?;
        let result = sqlx::query("UPDATE mcp_clients SET scopes = ?, updated_at = ? WHERE id = ?")
            .bind(scopes)
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                PostGateError::Storage(format!("Failed to update MCP client scopes: {}", e))
            })?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn rotate_mcp_client_token(
        &self,
        id: &str,
        token_salt: &str,
        token_hash: &str,
    ) -> Result<Option<McpClient>> {
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            "UPDATE mcp_clients SET token_salt = ?, token_hash = ?, updated_at = ? WHERE id = ? AND revoked = 0",
        )
        .bind(token_salt)
        .bind(token_hash)
        .bind(now)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to rotate MCP token: {}", e)))?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get_mcp_client(id).await
    }

    pub async fn touch_mcp_client(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE mcp_clients SET last_seen_at = ? WHERE id = ?")
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| PostGateError::Storage(format!("Failed to touch MCP client: {}", e)))?;
        Ok(())
    }

    pub async fn insert_mcp_audit_event(&self, event: &McpAuditEvent) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO mcp_audit_log (id, timestamp, client_id, operation, target, allowed, detail)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&event.id)
        .bind(event.timestamp)
        .bind(&event.client_id)
        .bind(&event.operation)
        .bind(&event.target)
        .bind(event.allowed as i32)
        .bind(&event.detail)
        .execute(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to save MCP audit event: {}", e)))?;

        Ok(())
    }

    pub async fn list_mcp_audit_events(&self, limit: i32) -> Result<Vec<McpAuditEvent>> {
        let rows = sqlx::query_as::<_, McpAuditEventRow>(
            r#"
            SELECT id, timestamp, client_id, operation, target, allowed, detail
            FROM mcp_audit_log
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(self.pool())
        .await
        .map_err(|e| PostGateError::Storage(format!("Failed to fetch MCP audit log: {}", e)))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[derive(sqlx::FromRow)]
struct McpSettingsRow {
    enabled: i32,
    port: i32,
    allowed_origins: String,
    updated_at: i64,
}

impl From<McpSettingsRow> for McpSettings {
    fn from(row: McpSettingsRow) -> Self {
        Self {
            enabled: row.enabled != 0,
            port: row.port as u16,
            allowed_origins: serde_json::from_str(&row.allowed_origins).unwrap_or_default(),
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct McpClientRow {
    id: String,
    name: String,
    token_salt: String,
    token_hash: String,
    scopes: String,
    revoked: i32,
    created_at: i64,
    updated_at: i64,
    last_seen_at: Option<i64>,
}

impl McpClientRow {
    fn try_into_client(&self) -> Result<McpClient> {
        Ok(McpClient {
            id: self.id.clone(),
            name: self.name.clone(),
            scopes: serde_json::from_str(&self.scopes)?,
            revoked: self.revoked != 0,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_seen_at: self.last_seen_at,
        })
    }

    fn try_into_auth_record(self) -> Result<McpClientAuthRecord> {
        Ok(McpClientAuthRecord {
            client: self.try_into_client()?,
            token_salt: self.token_salt,
            token_hash: self.token_hash,
        })
    }
}

#[derive(sqlx::FromRow)]
struct McpAuditEventRow {
    id: String,
    timestamp: i64,
    client_id: Option<String>,
    operation: String,
    target: Option<String>,
    allowed: i32,
    detail: Option<String>,
}

impl From<McpAuditEventRow> for McpAuditEvent {
    fn from(row: McpAuditEventRow) -> Self {
        Self {
            id: row.id,
            timestamp: row.timestamp,
            client_id: row.client_id,
            operation: row.operation,
            target: row.target,
            allowed: row.allowed != 0,
            detail: row.detail,
        }
    }
}
