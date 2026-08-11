//! Collections and saved-request commands for the composer library.

use androidqa_core::{
    composer::model::ManualRequest,
    persistence::collections::{CollectionSummary, SavedRequest, SavedRequestSummary},
};
use uuid::Uuid;

use crate::state::InspectorState;

fn parse_id(value: String) -> Result<Uuid, String> {
    Uuid::parse_str(&value).map_err(|error| format!("invalid id: {error}"))
}

#[tauri::command]
pub async fn create_collection(
    state: tauri::State<'_, InspectorState>,
    name: String,
    description: Option<String>,
) -> Result<CollectionSummary, String> {
    state
        .database
        .create_collection_async(&name, description.as_deref().unwrap_or(""), "")
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn rename_collection(
    state: tauri::State<'_, InspectorState>,
    id: String,
    name: String,
) -> Result<(), String> {
    state
        .database
        .rename_collection_async(parse_id(id)?, &name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_collection(
    state: tauri::State<'_, InspectorState>,
    id: String,
) -> Result<(), String> {
    state
        .database
        .delete_collection_async(parse_id(id)?)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_collections(
    state: tauri::State<'_, InspectorState>,
) -> Result<Vec<CollectionSummary>, String> {
    state
        .database
        .list_collections_async()
        .await
        .map_err(|error| error.to_string())
}

/// Saves the composer's current request. `id` carries the request currently
/// loaded in the composer (update in place) or `None` for a fresh save.
#[tauri::command]
pub async fn save_request(
    state: tauri::State<'_, InspectorState>,
    id: Option<String>,
    collection_id: String,
    name: String,
    request: ManualRequest,
) -> Result<SavedRequest, String> {
    let id = match id {
        Some(value) => Some(parse_id(value)?),
        None => None,
    };
    state
        .database
        .save_request_async(id, parse_id(collection_id)?, &name, &request)
        .await
        .map_err(|error| error.to_string())
}

/// Lightweight rows for the sidebar; the full payload is loaded on demand
/// via `get_request` when a saved request is opened.
#[tauri::command]
pub async fn list_requests(
    state: tauri::State<'_, InspectorState>,
    collection_id: String,
) -> Result<Vec<SavedRequestSummary>, String> {
    state
        .database
        .list_requests_async(parse_id(collection_id)?)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_request(
    state: tauri::State<'_, InspectorState>,
    id: String,
) -> Result<SavedRequest, String> {
    state
        .database
        .get_request_async(parse_id(id)?)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_request(
    state: tauri::State<'_, InspectorState>,
    id: String,
) -> Result<(), String> {
    state
        .database
        .delete_request_async(parse_id(id)?)
        .await
        .map_err(|error| error.to_string())
}
