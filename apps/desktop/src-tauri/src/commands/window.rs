use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    pub quic: bool,
    pub cloudkit_sync: bool,
    pub icloud_sync: bool,
}

#[tauri::command]
pub fn get_runtime_capabilities() -> RuntimeCapabilities {
    RuntimeCapabilities {
        quic: cfg!(feature = "quic"),
        cloudkit_sync: cloudkit_sync_available(),
        icloud_sync: cfg!(target_os = "macos"),
    }
}

fn cloudkit_sync_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::cloudkit_sync::is_available()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Show the main window - called by frontend when ready
#[tauri::command]
pub async fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    tracing::info!("Frontend requested to show main window");

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;

    tracing::info!("Main window shown successfully");
    Ok(())
}
