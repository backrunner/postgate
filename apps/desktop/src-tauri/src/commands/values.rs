use crate::error::Result;
use crate::state::AppState;
use crate::values::ValueEntry;
use std::sync::Arc;
use tauri::State;

/// List all stored values (sorted by name).
#[tauri::command]
pub async fn list_values(state: State<'_, Arc<AppState>>) -> Result<Vec<ValueEntry>> {
    // Ensure the in-memory map is hydrated, then read authoritatively from DB
    // so the frontend always gets fresh timestamps.
    state.ensure_values_loaded().await?;
    let db = state.get_database().await?;
    db.list_values().await
}

/// Insert or update a value. The in-memory store is updated in lock-step.
#[tauri::command]
pub async fn save_value(
    name: String,
    content: String,
    state: State<'_, Arc<AppState>>,
) -> Result<ValueEntry> {
    state.ensure_values_loaded().await?;
    let db = state.get_database().await?;
    let entry = db.upsert_value(&name, &content).await?;
    state
        .values_store
        .insert(entry.name.clone(), entry.content.clone());
    Ok(entry)
}

/// Delete a value; returns whether a row existed.
#[tauri::command]
pub async fn delete_value(name: String, state: State<'_, Arc<AppState>>) -> Result<bool> {
    state.ensure_values_loaded().await?;
    let db = state.get_database().await?;
    let removed = db.delete_value(&name).await?;
    state.values_store.remove(&name);
    Ok(removed)
}

/// Rename a value (useful when the user edits the name in the UI).
#[tauri::command]
pub async fn rename_value(
    old_name: String,
    new_name: String,
    state: State<'_, Arc<AppState>>,
) -> Result<ValueEntry> {
    state.ensure_values_loaded().await?;
    let db = state.get_database().await?;
    let entry = db.rename_value(&old_name, &new_name).await?;
    state.values_store.remove(&old_name);
    state
        .values_store
        .insert(entry.name.clone(), entry.content.clone());
    Ok(entry)
}
