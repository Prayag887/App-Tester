//! Captured-traffic, baseline, and comparison-rules commands.

use androidqa_core::{comparison::ComparisonRules, traffic::HttpTransaction};
use tauri::State;
use uuid::Uuid;

use crate::state::InspectorState;

#[tauri::command]
pub fn list_transactions(
    state: State<'_, InspectorState>,
    session_id: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<HttpTransaction>, String> {
    // The WebView keeps the capture ID returned when it opened the companion.
    // Prefer that explicit ID over the process-local fallback so a UI restore
    // cannot read a different capture session from the one receiving traffic.
    let session_id = match session_id {
        Some(session_id) => {
            Uuid::parse_str(&session_id).map_err(|_| "invalid capture session id")?
        }
        None => match state.session.id() {
            Some(session_id) => session_id,
            None => return Ok(vec![]),
        },
    };
    let mut transactions = state
        .database
        .list_transactions(session_id, limit.unwrap_or(250), offset.unwrap_or(0))
        .map_err(|error| error.to_string())?;
    transactions
        .iter_mut()
        .for_each(crate::ui_payload::cap_transaction_summary);
    Ok(transactions)
}

#[tauri::command]
pub fn delete_all_transactions(state: State<'_, InspectorState>) -> Result<(), String> {
    state
        .database
        .delete_all_transactions()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_transaction(
    state: State<'_, InspectorState>,
    id: Uuid,
) -> Result<Option<HttpTransaction>, String> {
    let mut transaction = state
        .database
        .get_transaction(id)
        .map_err(|error| error.to_string())?;
    if let Some(transaction) = transaction.as_mut() {
        crate::ui_payload::cap_transaction_detail(transaction);
    }
    Ok(transaction)
}

#[tauri::command]
pub fn approve_baseline(
    state: State<'_, InspectorState>,
    endpoint_id: String,
    transaction_id: Uuid,
) -> Result<(), String> {
    state
        .database
        .approve_baseline(&endpoint_id, transaction_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_baseline(
    state: State<'_, InspectorState>,
    endpoint_id: String,
) -> Result<bool, String> {
    state
        .database
        .delete_baseline(&endpoint_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_comparison_rules(
    state: State<'_, InspectorState>,
    endpoint_id: String,
) -> Result<ComparisonRules, String> {
    state
        .database
        .comparison_rules(&endpoint_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_comparison_rules(
    state: State<'_, InspectorState>,
    endpoint_id: String,
    ignored_json_pointers: Vec<String>,
    volatile_keys: Vec<String>,
) -> Result<(), String> {
    let rules = ComparisonRules {
        ignored_json_pointers: ignored_json_pointers.into_iter().collect(),
        volatile_keys: volatile_keys.into_iter().collect(),
    };
    state
        .database
        .save_comparison_rules(&endpoint_id, &rules)
        .map_err(|error| error.to_string())
}
