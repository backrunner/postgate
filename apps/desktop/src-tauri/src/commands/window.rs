use tauri::Manager;

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
