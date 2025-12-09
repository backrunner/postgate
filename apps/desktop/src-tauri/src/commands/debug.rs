// Tauri IPC commands for debug functionality

use crate::debug::{ConsoleLog, DebugSession, DebugStatus, PageError};
use crate::state::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn start_debug_server(
    state: State<'_, Arc<AppState>>,
    port: Option<u16>,
) -> Result<(), String> {
    let port = port.unwrap_or(9229);
    state.start_debug_server(port).await
}

#[tauri::command]
pub async fn stop_debug_server(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.stop_debug_server().await;
    Ok(())
}

#[tauri::command]
pub async fn get_debug_status(
    state: State<'_, Arc<AppState>>,
) -> Result<DebugStatus, String> {
    state.get_debug_status().await
}

#[tauri::command]
pub async fn get_debug_sessions(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DebugSession>, String> {
    Ok(state.get_debug_sessions())
}

#[tauri::command]
pub async fn get_console_logs(
    state: State<'_, Arc<AppState>>,
    session_id: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<ConsoleLog>, String> {
    Ok(state.get_console_logs(session_id.as_deref(), limit, offset))
}

#[tauri::command]
pub async fn clear_console_logs(
    state: State<'_, Arc<AppState>>,
    session_id: Option<String>,
) -> Result<(), String> {
    state.clear_console_logs(session_id.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn get_page_errors(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<Vec<PageError>, String> {
    Ok(state.get_page_errors(&session_id))
}

#[tauri::command]
pub async fn clear_all_debug_data(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.clear_all_debug_data();
    Ok(())
}

#[tauri::command]
pub async fn remove_debug_session(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    state.remove_debug_session(&session_id);
    Ok(())
}
