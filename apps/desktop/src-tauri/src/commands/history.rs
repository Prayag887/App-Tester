//! Composer history commands.

use androidqa_core::{composer::model::ManualRequest, persistence::history::HistorySummary};
use uuid::Uuid;

use crate::state::InspectorState;

#[tauri::command]
pub async fn list_history(
    state: tauri::State<'_, InspectorState>,
) -> Result<Vec<HistorySummary>, String> {
    state
        .database
        .list_history_async()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_history_request(
    state: tauri::State<'_, InspectorState>,
    id: String,
) -> Result<ManualRequest, String> {
    let id = Uuid::parse_str(&id).map_err(|error| format!("invalid id: {error}"))?;
    state
        .database
        .get_history_request_async(id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_history(
    state: tauri::State<'_, InspectorState>,
    id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&id).map_err(|error| format!("invalid id: {error}"))?;
    state
        .database
        .delete_history_async(id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_history(state: tauri::State<'_, InspectorState>) -> Result<(), String> {
    state
        .database
        .clear_history_async()
        .await
        .map_err(|error| error.to_string())
}
