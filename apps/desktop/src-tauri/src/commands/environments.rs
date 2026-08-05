//! Environment and variable commands for the composer.

use androidqa_core::persistence::environments::{EnvironmentSummary, VariableRecord};
use uuid::Uuid;

use crate::state::InspectorState;

fn parse_optional_id(value: Option<String>) -> Result<Option<Uuid>, String> {
    match value {
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => Uuid::parse_str(&value)
            .map(Some)
            .map_err(|error| format!("invalid id: {error}")),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn create_environment(
    state: tauri::State<'_, InspectorState>,
    name: String,
) -> Result<EnvironmentSummary, String> {
    state
        .database
        .create_environment_async(&name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn rename_environment(
    state: tauri::State<'_, InspectorState>,
    id: String,
    name: String,
) -> Result<(), String> {
    let id = parse_optional_id(Some(id))?.ok_or("invalid environment id")?;
    state
        .database
        .rename_environment_async(id, &name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_environment(
    state: tauri::State<'_, InspectorState>,
    id: String,
) -> Result<(), String> {
    let id = parse_optional_id(Some(id))?.ok_or("invalid environment id")?;
    state
        .database
        .delete_environment_async(id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_environments(
    state: tauri::State<'_, InspectorState>,
) -> Result<Vec<EnvironmentSummary>, String> {
    state
        .database
        .list_environments_async()
        .await
        .map_err(|error| error.to_string())
}

/// Lists variables of one environment; omit `environment_id` for globals.
#[tauri::command]
pub async fn list_variables(
    state: tauri::State<'_, InspectorState>,
    environment_id: Option<String>,
) -> Result<Vec<VariableRecord>, String> {
    state
        .database
        .list_variables_async(parse_optional_id(environment_id)?)
        .await
        .map_err(|error| error.to_string())
}

/// Saves a variable; `id` carries the edited row (update) or `None` (insert).
#[tauri::command]
pub async fn save_variable(
    state: tauri::State<'_, InspectorState>,
    id: Option<String>,
    environment_id: Option<String>,
    name: String,
    value: String,
    is_secret: bool,
) -> Result<VariableRecord, String> {
    state
        .database
        .save_variable_async(
            parse_optional_id(id)?,
            parse_optional_id(environment_id)?,
            &name,
            &value,
            is_secret,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_variable(
    state: tauri::State<'_, InspectorState>,
    id: String,
) -> Result<(), String> {
    let id = parse_optional_id(Some(id))?.ok_or("invalid variable id")?;
    state
        .database
        .delete_variable_async(id)
        .await
        .map_err(|error| error.to_string())
}
