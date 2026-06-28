use super::PostGateApi;

impl PostGateApi {
    pub async fn debug_status(&self) -> std::result::Result<crate::debug::DebugStatus, String> {
        self.state.get_debug_status().await
    }

    pub fn debug_sessions(&self) -> Vec<crate::debug::DebugSession> {
        self.state.get_debug_sessions()
    }

    pub fn console_logs(
        &self,
        session_id: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Vec<crate::debug::ConsoleLog> {
        self.state.get_console_logs(session_id, limit, offset)
    }

    pub fn page_errors(&self, session_id: &str) -> Vec<crate::debug::PageError> {
        self.state.get_page_errors(session_id)
    }

    pub fn network_requests(&self, session_id: &str) -> Vec<crate::debug::PageNetworkRequest> {
        self.state.get_network_requests(session_id)
    }
}
