mod capture;
mod debug;
mod proxy;
mod replay;
mod rules;
pub mod types;
mod values;

use crate::state::AppState;
use std::sync::Arc;

pub use types::{
    CaptureBodyEncoding, CaptureBodyInput, CaptureBodyResult, CaptureBodySide, CaptureBodySource,
    CaptureSearchInput, CaptureSearchResult, NetworkAddress, ProxyStatusView, RuleParseIssue,
    RuleParseResult,
};

#[derive(Clone)]
pub struct PostGateApi {
    pub(crate) state: Arc<AppState>,
}

impl PostGateApi {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub fn state(&self) -> Arc<AppState> {
        Arc::clone(&self.state)
    }
}
