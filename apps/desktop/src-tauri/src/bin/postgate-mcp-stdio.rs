use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, ClientInfo, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ServerCapabilities,
        ServerInfo,
    },
    service::RequestContext,
    transport::{
        io::stdio, streamable_http_client::StreamableHttpClientTransportConfig,
        StreamableHttpClientTransport,
    },
    ErrorData, RoleServer, ServerHandler, ServiceError, ServiceExt,
};

#[derive(Clone)]
struct Bridge {
    peer: rmcp::Peer<rmcp::RoleClient>,
}

impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions("Bridge stdio MCP requests to the local PostGate MCP server.")
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.peer.list_tools(request).await.map_err(to_error_data)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.peer.call_tool(request).await.map_err(to_error_data)
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        self.peer
            .list_resources(request)
            .await
            .map_err(to_error_data)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        self.peer
            .read_resource(request)
            .await
            .map_err(to_error_data)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = std::env::var("POSTGATE_MCP_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:18999/mcp".to_string());
    let token = std::env::var("POSTGATE_MCP_TOKEN")
        .map_err(|_| anyhow::anyhow!("POSTGATE_MCP_TOKEN is required"))?;

    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(endpoint).auth_header(token),
    );
    let remote = ClientInfo::default().serve(transport).await?;
    let bridge = Bridge {
        peer: remote.peer().clone(),
    };
    let stdio_server = bridge.serve(stdio()).await?;
    stdio_server.waiting().await?;
    let _ = remote.cancel().await;
    Ok(())
}

fn to_error_data(error: ServiceError) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}
