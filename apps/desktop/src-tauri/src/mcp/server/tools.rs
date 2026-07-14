use super::inputs::{
    AppendRuleLinesInput, CaptureBodyToolInput, CaptureGetInput, CaptureSearchToolInput,
    ConsoleLogsInput, DeleteValueInput, IdInput, ImportCaptureReplayInput, RenameValueInput,
    ReplayExecuteInput, SaveValueInput, SessionInput, SetPersistenceInput, StartProxyInput,
    ToggleRuleGroupInput, UpsertRuleGroupInput, ValidateRulesInput,
};
use super::util::to_json;
use super::PostGateMcpServer;
use crate::proxy::ProxyConfig;
use crate::rules::RuleGroup;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_router,
};
use serde_json::json;

pub(super) fn tool_router() -> ToolRouter<PostGateMcpServer> {
    PostGateMcpServer::tool_router()
}

#[tool_router]
impl PostGateMcpServer {
    #[tool(
        name = "postgate.proxy.status",
        description = "Get PostGate proxy status"
    )]
    async fn proxy_status(&self) -> Result<String, String> {
        to_json(self.api.proxy_status().await?)
    }

    #[tool(
        name = "postgate.proxy.start",
        description = "Start the local PostGate proxy"
    )]
    async fn proxy_start(
        &self,
        Parameters(input): Parameters<StartProxyInput>,
    ) -> Result<String, String> {
        let defaults = ProxyConfig::default();
        let config = ProxyConfig {
            port: input.port.unwrap_or(defaults.port),
            enable_http2: input.enable_http2,
            enable_quic: input.enable_quic,
            quic_port: input.quic_port,
            debug_port: defaults.debug_port,
            max_connections_per_host: input
                .max_connections_per_host
                .unwrap_or(defaults.max_connections_per_host),
            connection_idle_timeout: input
                .connection_idle_timeout
                .unwrap_or(defaults.connection_idle_timeout),
        };
        to_json(self.api.start_proxy(config).await?)
    }

    #[tool(
        name = "postgate.proxy.stop",
        description = "Stop the local PostGate proxy"
    )]
    async fn proxy_stop(&self) -> Result<String, String> {
        to_json(self.api.stop_proxy().await?)
    }

    #[tool(
        name = "postgate.proxy.set_persistence",
        description = "Enable or disable captured request persistence"
    )]
    async fn proxy_set_persistence(
        &self,
        Parameters(input): Parameters<SetPersistenceInput>,
    ) -> Result<String, String> {
        self.api.set_persistence_enabled(input.enabled);
        to_json(json!({ "enabled": self.api.get_persistence_enabled() }))
    }

    #[tool(
        name = "postgate.proxy.get_local_ips",
        description = "List local network addresses useful for proxy setup"
    )]
    async fn proxy_get_local_ips(&self) -> Result<String, String> {
        to_json(self.api.get_local_ips())
    }

    #[tool(
        name = "postgate.rules.list_groups",
        description = "List proxy rule groups"
    )]
    async fn rules_list_groups(&self) -> Result<String, String> {
        to_json(self.api.list_rule_groups().await?)
    }

    #[tool(
        name = "postgate.rules.get_group",
        description = "Get one proxy rule group"
    )]
    async fn rules_get_group(
        &self,
        Parameters(input): Parameters<IdInput>,
    ) -> Result<String, String> {
        to_json(self.api.get_rule_group(&input.id).await?)
    }

    #[tool(
        name = "postgate.rules.validate",
        description = "Validate whistle-compatible rules"
    )]
    async fn rules_validate(
        &self,
        Parameters(input): Parameters<ValidateRulesInput>,
    ) -> Result<String, String> {
        to_json(self.api.validate_rules(&input.content))
    }

    #[tool(
        name = "postgate.rules.upsert_group",
        description = "Create or replace a proxy rule group"
    )]
    async fn rules_upsert_group(
        &self,
        Parameters(input): Parameters<UpsertRuleGroupInput>,
    ) -> Result<String, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let existing = match &input.id {
            Some(id) => self.api.get_rule_group(id).await?,
            None => None,
        };
        let group = RuleGroup {
            id: input.id.unwrap_or_default(),
            name: input.name,
            enabled: input.enabled,
            priority: input.priority,
            rules: vec![],
            raw_content: input.raw_content,
            created_at: existing.map(|group| group.created_at).unwrap_or(now),
            updated_at: now,
            inline_values: Default::default(),
        };
        to_json(self.api.save_rule_group(group).await?)
    }

    #[tool(
        name = "postgate.rules.append_lines",
        description = "Append raw rule lines to a rule group"
    )]
    async fn rules_append_lines(
        &self,
        Parameters(input): Parameters<AppendRuleLinesInput>,
    ) -> Result<String, String> {
        to_json(self.api.append_rule_lines(&input.id, &input.lines).await?)
    }

    #[tool(
        name = "postgate.rules.toggle_group",
        description = "Enable or disable a rule group"
    )]
    async fn rules_toggle_group(
        &self,
        Parameters(input): Parameters<ToggleRuleGroupInput>,
    ) -> Result<String, String> {
        to_json(json!({
            "updated": self.api.toggle_rule_group(&input.id, input.enabled).await?
        }))
    }

    #[tool(
        name = "postgate.rules.delete_group",
        description = "Delete a rule group"
    )]
    async fn rules_delete_group(
        &self,
        Parameters(input): Parameters<IdInput>,
    ) -> Result<String, String> {
        to_json(json!({
            "deleted": self.api.delete_rule_group(&input.id).await?
        }))
    }

    #[tool(name = "postgate.values.list", description = "List whistle values")]
    async fn values_list(&self) -> Result<String, String> {
        to_json(self.api.list_values().await?)
    }

    #[tool(
        name = "postgate.values.save",
        description = "Create or update a whistle value"
    )]
    async fn values_save(
        &self,
        Parameters(input): Parameters<SaveValueInput>,
    ) -> Result<String, String> {
        to_json(self.api.save_value(&input.name, &input.content).await?)
    }

    #[tool(
        name = "postgate.values.rename",
        description = "Rename a whistle value"
    )]
    async fn values_rename(
        &self,
        Parameters(input): Parameters<RenameValueInput>,
    ) -> Result<String, String> {
        to_json(
            self.api
                .rename_value(&input.old_name, &input.new_name)
                .await?,
        )
    }

    #[tool(
        name = "postgate.values.delete",
        description = "Delete a whistle value"
    )]
    async fn values_delete(
        &self,
        Parameters(input): Parameters<DeleteValueInput>,
    ) -> Result<String, String> {
        to_json(json!({ "deleted": self.api.delete_value(&input.name).await? }))
    }

    #[tool(
        name = "postgate.capture.search",
        description = "Search recent captured proxy requests"
    )]
    async fn capture_search(
        &self,
        Parameters(input): Parameters<CaptureSearchToolInput>,
    ) -> Result<String, String> {
        to_json(self.api.search_captures(input.into()).await?)
    }

    #[tool(
        name = "postgate.capture.get",
        description = "Get a captured proxy request by id"
    )]
    async fn capture_get(
        &self,
        Parameters(input): Parameters<CaptureGetInput>,
    ) -> Result<String, String> {
        to_json(self.api.get_capture(&input.id, input.redact).await?)
    }

    #[tool(
        name = "postgate.capture.get_body",
        description = "Read a captured request or response body"
    )]
    async fn capture_get_body(
        &self,
        Parameters(input): Parameters<CaptureBodyToolInput>,
    ) -> Result<String, String> {
        to_json(self.api.get_capture_body(input.try_into()?).await?)
    }

    #[tool(
        name = "postgate.capture.clear_history",
        description = "Clear captured metadata and bodies"
    )]
    async fn capture_clear_history(&self) -> Result<String, String> {
        self.api.clear_capture_history().await?;
        to_json(json!({ "cleared": true }))
    }

    #[tool(
        name = "postgate.replay.execute",
        description = "Execute a saved request payload"
    )]
    async fn replay_execute(
        &self,
        Parameters(input): Parameters<ReplayExecuteInput>,
    ) -> Result<String, String> {
        let request = serde_json::from_value(input.request).map_err(|e| e.to_string())?;
        to_json(self.api.execute_replay(request).await?)
    }

    #[tool(
        name = "postgate.replay.import_capture",
        description = "Import a captured request into Replay"
    )]
    async fn replay_import_capture(
        &self,
        Parameters(input): Parameters<ImportCaptureReplayInput>,
    ) -> Result<String, String> {
        to_json(
            self.api
                .import_capture_to_replay(&input.id, input.collection_id)
                .await?,
        )
    }

    #[tool(
        name = "postgate.debug.status",
        description = "Get debug server status"
    )]
    async fn debug_status(&self) -> Result<String, String> {
        to_json(self.api.debug_status().await.map_err(|e| e.to_string())?)
    }

    #[tool(name = "postgate.debug.sessions", description = "List debug sessions")]
    async fn debug_sessions(&self) -> Result<String, String> {
        to_json(self.api.debug_sessions())
    }

    #[tool(
        name = "postgate.debug.console_logs",
        description = "List debug console logs"
    )]
    async fn debug_console_logs(
        &self,
        Parameters(input): Parameters<ConsoleLogsInput>,
    ) -> Result<String, String> {
        to_json(
            self.api
                .console_logs(input.session_id.as_deref(), input.limit, input.offset),
        )
    }

    #[tool(
        name = "postgate.debug.page_errors",
        description = "List page errors for a debug session"
    )]
    async fn debug_page_errors(
        &self,
        Parameters(input): Parameters<SessionInput>,
    ) -> Result<String, String> {
        to_json(self.api.page_errors(&input.session_id))
    }

    #[tool(
        name = "postgate.debug.network_requests",
        description = "List page network requests for a debug session"
    )]
    async fn debug_network_requests(
        &self,
        Parameters(input): Parameters<SessionInput>,
    ) -> Result<String, String> {
        to_json(self.api.network_requests(&input.session_id))
    }
}
