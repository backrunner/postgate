use crate::cert::CertStore;
use crate::error::Result;
use crate::state::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::{Manager, State};

#[derive(Debug, Serialize)]
pub struct CertificateInfo {
    pub installed: bool,
    pub pem: String,
}

/// Get the CA certificate
#[tauri::command]
pub async fn get_ca_certificate(state: State<'_, Arc<AppState>>) -> Result<CertificateInfo> {
    let ca = state.get_or_init_ca()?;

    Ok(CertificateInfo {
        installed: false, // TODO: Check if actually installed
        pem: ca.get_ca_pem().to_string(),
    })
}

/// Install the CA certificate to the system
#[tauri::command]
pub async fn install_ca_certificate(state: State<'_, Arc<AppState>>) -> Result<bool> {
    let ca = state.get_or_init_ca()?;
    let pem = ca.get_ca_pem();

    // Get app data directory
    let app_handle = &state.app_handle;
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::PostGateError::Storage(e.to_string()))?;

    let store = CertStore::new(data_dir);

    // Save and install
    store.save_ca_cert(pem)?;
    store.install_to_system(pem)?;

    Ok(true)
}

/// Export the CA certificate to a file
#[tauri::command]
pub async fn export_ca_certificate(path: String, state: State<'_, Arc<AppState>>) -> Result<bool> {
    let ca = state.get_or_init_ca()?;
    let pem = ca.get_ca_pem();

    std::fs::write(&path, pem)?;

    tracing::info!("Exported CA certificate to {}", path);
    Ok(true)
}
