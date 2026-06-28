mod inputs;
mod resources;
mod tools;
mod util;

use crate::api::PostGateApi;
use rmcp::handler::server::router::tool::ToolRouter;

#[derive(Clone)]
pub struct PostGateMcpServer {
    api: PostGateApi,
    tool_router: ToolRouter<Self>,
}

impl PostGateMcpServer {
    pub fn new(api: PostGateApi) -> Self {
        Self {
            api,
            tool_router: tools::tool_router(),
        }
    }
}
