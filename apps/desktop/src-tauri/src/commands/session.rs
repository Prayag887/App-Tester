//! Proxy lifecycle, logcat capture, and capture import/export commands.

use std::time::Duration;

use androidqa_core::{
    android,
    diagnostics::logcat::{LogcatSupervisor, logcat_command},
    events::InspectorEvent,
    persistence::portable,
    replay::{ReplaySummary, replay, replay_blocker},
    traffic::TransactionState,
};
use tauri::State;
use time::{OffsetDateTime, PrimitiveDateTime, Time};
use uuid::Uuid;

use crate::{adb::adb_blocking, state::InspectorState};

#[tauri::command]
pub async fn start_proxy(state: State<'_, InspectorState>) -> Result<String, String> {
    let session_id = state.session.id_or_new();
    state
        .proxy
        .start(session_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(session_id.to_string())
}

#[tauri::command]
pub async fn stop_proxy(state: State<'_, InspectorState>) -> Result<(), String> {
    if let Some(supervisor) = state.session.logcat().take() {
        supervisor.abort();
    }
    if let Some(serial) = state.session.take_companion_device() {
        adb_blocking(move |adb| android::stop_usb_companion_capture(adb, &serial)).await?;
    }
    state.proxy.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn restart_proxy(state: State<'_, InspectorState>) -> Result<String, String> {
    state.proxy.stop().await;
    start_proxy(state).await
}

#[tauri::command]
pub async fn start_logcat_capture(
    state: State<'_, InspectorState>,
    serial: String,
    package_name: String,
) -> Result<(), String> {
    if package_name.trim().is_empty() {
        return Ok(());
    }
    let session_id = state
        .session
        .id()
        .ok_or_else(|| "start the proxy before starting log capture".to_owned())?;
    let command_serial = serial.clone();
    let command_package = package_name.clone();
    let uid =
        adb_blocking(move |adb| android::app_uid(adb, &command_serial, &command_package)).await?;
    let adb_path = crate::adb::adb()?.path().to_path_buf();
    let events = state.proxy.events();
    let args = logcat_command(&serial, uid);
    let mut logcat = state.session.logcat();
    let supervisor = logcat.get_or_insert_with(LogcatSupervisor::new);
    supervisor.spawn(
        adb_path.clone(),
        args,
        adb_path,
        serial,
        events,
        session_id,
        package_name,
    );
    Ok(())
}

#[tauri::command]
pub fn export_capture(state: State<'_, InspectorState>) -> Result<String, String> {
    let session_id = state
        .session
        .id()
        .ok_or_else(|| "capture something before exporting".to_owned())?;
    let transactions = state
        .database
        .all_session_transactions(session_id)
        .map_err(|error| error.to_string())?;
    let capture = portable::export_capture(&transactions, OffsetDateTime::now_utc());
    portable::encode_capture(&capture).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_capture_to_file(state: State<'_, InspectorState>) -> Result<String, String> {
    let payload = export_capture(state)?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Export redacted App Tester capture")
        .add_filter("JSON", &["json"])
        .set_file_name("app-tester-capture.json")
        .save_file()
    else {
        return Err("export canceled".into());
    };
    std::fs::write(&path, payload).map_err(|error| format!("could not write export: {error}"))?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn import_capture(state: State<'_, InspectorState>, payload: String) -> Result<usize, String> {
    let session_id = Uuid::new_v4();
    let transactions = portable::import_capture(&payload, session_id, OffsetDateTime::now_utc())
        .map_err(|error| error.to_string())?;
    for transaction in &transactions {
        state
            .database
            .upsert_transaction(transaction)
            .map_err(|error| error.to_string())?;
    }
    state.session.set_id(session_id);
    Ok(transactions.len())
}

#[tauri::command]
pub async fn test_yesterdays_apis(
    state: State<'_, InspectorState>,
) -> Result<ReplaySummary, String> {
    let today = OffsetDateTime::now_utc().date();
    let yesterday = today
        .previous_day()
        .ok_or_else(|| "could not calculate yesterday".to_owned())?;
    let start = PrimitiveDateTime::new(yesterday, Time::MIDNIGHT).assume_utc();
    let end = PrimitiveDateTime::new(today, Time::MIDNIGHT).assume_utc();
    let baselines = state
        .database
        .transactions_between_async(start, end)
        .await
        .map_err(|error| error.to_string())?;
    let session_id = state.session.id_or_new();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let mut summary = ReplaySummary::default();
    for baseline in baselines {
        if replay_blocker(&baseline).is_some() {
            summary.skipped += 1;
            continue;
        }
        summary.attempted += 1;
        let result = replay(&client, &baseline, session_id).await;
        if result.state == TransactionState::Failed {
            summary.failed += 1;
        } else {
            summary.completed += 1;
            if result
                .comparison
                .as_ref()
                .is_some_and(|comparison| !comparison.differences.is_empty())
            {
                summary.changed += 1;
            }
        }
        state
            .database
            .upsert_async(result.clone())
            .await
            .map_err(|error| error.to_string())?;
        state
            .proxy
            .events()
            .send(InspectorEvent::TransactionCompleted(result));
    }
    Ok(summary)
}
