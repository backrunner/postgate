use super::util::{resource, to_error_data, to_value};
use super::PostGateMcpServer;
use crate::api::CaptureSearchInput;
use rmcp::{
    model::{
        ListResourcesResult, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool_handler, ErrorData, RoleServer, ServerHandler,
};

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PostGateMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "PostGate MCP exposes local proxy control, rules, captures, replay, and debug data.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![
                resource("postgate://proxy/status", "Proxy Status"),
                resource("postgate://rules/groups", "Rule Groups"),
                resource("postgate://captures/recent", "Recent Captures"),
                resource("postgate://debug/sessions", "Debug Sessions"),
                resource("postgate://mcp/audit", "MCP Audit Log"),
            ],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let uri = request.uri;
        let value = match uri.as_str() {
            "postgate://proxy/status" => to_value(self.api.proxy_status().await),
            "postgate://rules/groups" => to_value(self.api.list_rule_groups().await),
            "postgate://captures/recent" => to_value(
                self.api
                    .search_captures(CaptureSearchInput::default())
                    .await,
            ),
            "postgate://debug/sessions" => Ok(serde_json::to_value(self.api.debug_sessions())
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?),
            "postgate://mcp/audit" => {
                let db = self
                    .api
                    .state()
                    .get_database()
                    .await
                    .map_err(to_error_data)?;
                to_value(db.list_mcp_audit_events(100).await)
            }
            _ if uri.starts_with("postgate://captures/") => {
                let id = uri.trim_start_matches("postgate://captures/");
                to_value(self.api.get_capture(id, true).await)
            }
            _ => {
                return Err(ErrorData::resource_not_found(
                    "Unknown PostGate resource",
                    None,
                ))
            }
        }?;

        let text = serde_json::to_string_pretty(&value)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            text, uri,
        )
        .with_mime_type("application/json")]))
    }
}
