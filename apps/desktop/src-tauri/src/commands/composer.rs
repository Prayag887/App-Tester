//! Composer commands: send manually composed requests from the desktop shell.

use androidqa_core::composer::{
    curl::{CurlImport, parse_curl as parse_curl_core},
    model::{ManualRequest, SendOptions, SendResult},
    send_manual,
    variables::{Variable, resolve_request},
};

use crate::state::InspectorState;

/// Parses a pasted curl command into a request plus transport options.
#[tauri::command]
pub fn parse_curl(input: String) -> Result<CurlImport, String> {
    parse_curl_core(&input).map_err(|error| error.to_string())
}

/// Opens a native file picker for multipart file fields; `None` when the
/// user cancels.
#[tauri::command]
pub fn pick_file() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Choose a file to upload")
        .pick_file()
        .map(|path| path.display().to_string()))
}

/// Sends a composed request, recording it as a transaction in the current
/// session (same storage, same events as captured traffic).
#[tauri::command]
pub async fn send_request(
    state: tauri::State<'_, InspectorState>,
    request: ManualRequest,
    options: Option<SendOptions>,
    variables: Option<Vec<Variable>>,
) -> Result<SendResult, String> {
    let session_id = state.session.id_or_new();
    let variables = variables.as_deref().unwrap_or(&[]);
    // Resolve once up front so history records exactly what went on the
    // wire; the engine's own resolution is then a no-op.
    let resolved = resolve_request(&request, variables);
    let outcome = send_manual(
        state.database.clone(),
        state.proxy.events(),
        session_id,
        resolved.clone(),
        options.unwrap_or_default(),
        variables,
    )
    .await;
    let status = outcome.as_ref().map(|result| result.status).ok();
    let _ = state.database.record_history_async(&resolved, status).await;
    outcome.map_err(|error| error.to_string())
}
