use crate::state::AppState;
use std::sync::Arc;
use tauri::Emitter;

pub(crate) const RULE_GROUPS_CHANGED_EVENT: &str = "rules:groups_changed";

pub(crate) async fn notify_rule_groups_changed(state: &Arc<AppState>) {
    #[cfg(target_os = "macos")]
    {
        if let Err(error) = crate::app_tray::refresh(&state.app_handle, Arc::clone(state)).await {
            tracing::warn!("Failed to refresh tray rule groups: {}", error);
        }
    }

    if let Err(error) = state.app_handle.emit(RULE_GROUPS_CHANGED_EVENT, ()) {
        tracing::warn!("Failed to emit rule group change event: {}", error);
    }
}
