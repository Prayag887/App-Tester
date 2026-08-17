//! Composer commands: send manually composed requests from the desktop shell.

use androidqa_core::composer::{
    curl::{
        CurlImport, generate_curl_command as generate_curl_command_core,
        parse_curl as parse_curl_core,
    },
    model::{ManualRequest, SendOptions, SendResult},
    send_manual,
};

use crate::state::InspectorState;

/// Parses a pasted curl command into a request plus transport options.
#[tauri::command]
pub fn parse_curl(input: String) -> Result<CurlImport, String> {
    parse_curl_core(&input).map_err(|error| error.to_string())
}

/// Exports the current Composer request as an executable cURL command.
#[tauri::command]
pub fn generate_composer_curl(
    request: ManualRequest,
    options: Option<SendOptions>,
) -> Result<String, String> {
    generate_curl_command_core(&request, &options.unwrap_or_default())
        .map_err(|error| error.to_string())
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
) -> Result<SendResult, String> {
    let session_id = state.session.id_or_new();
    let mut result = send_manual(
        state.database.clone(),
        state.proxy.events(),
        session_id,
        request,
        options.unwrap_or_default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    crate::ui_payload::cap_send_result(&mut result);
    Ok(result)
}
