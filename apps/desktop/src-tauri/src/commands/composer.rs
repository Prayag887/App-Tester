//! Composer commands: send manually composed requests from the desktop shell.

use androidqa_core::composer::{
    model::{ManualRequest, SendOptions, SendResult},
    send_manual,
};

use crate::state::InspectorState;

/// Sends a composed request, recording it as a transaction in the current
/// session (same storage, same events as captured traffic).
#[tauri::command]
pub async fn send_request(
    state: tauri::State<'_, InspectorState>,
    request: ManualRequest,
    options: Option<SendOptions>,
) -> Result<SendResult, String> {
    let session_id = state.session.id_or_new();
    send_manual(
        state.database.clone(),
        state.proxy.events(),
        session_id,
        request,
        options.unwrap_or_default(),
    )
    .await
    .map_err(|error| error.to_string())
}
