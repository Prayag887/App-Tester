use std::{
    net::{IpAddr, Ipv4Addr, UdpSocket},
    sync::{Arc, Mutex},
};

use apiqa_core::{
    ApiQaEngine, CleanupResult, Collection, Environment, RetentionPolicy, Run, RunOptions, Store,
    android::{Adb, AndroidApp, AndroidDevice, ConnectionType},
    capture::{
        CaptureEvent, CaptureService, CertificateInfo, HttpTransaction, LogcatEvent, LogcatService,
        ProxyStatus,
    },
    diagnostics::{Diagnostic, LogLine},
    export_workspace, import_postman, import_postman_environment, import_workspace,
};
use tauri::{Emitter, Manager, State};

struct AppState {
    engine: Arc<ApiQaEngine>,
    capture: Arc<CaptureService>,
    logcat: Arc<LogcatService>,
    configured_device: Mutex<Option<String>>,
    capture_lifecycle: tokio::sync::Mutex<()>,
}

#[tauri::command]
fn list_collections(state: State<'_, AppState>) -> Result<Vec<Collection>, String> {
    state.engine.collections().map_err(display_error)
}

#[tauri::command]
fn import_collection(source: String, state: State<'_, AppState>) -> Result<Collection, String> {
    let collection = import_postman(&source).map_err(display_error)?;
    state
        .engine
        .save_collection(&collection)
        .map_err(display_error)?;
    Ok(collection)
}

#[tauri::command]
fn save_collection(collection: Collection, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .save_collection(&collection)
        .map_err(display_error)
}

#[tauri::command]
fn import_environment(source: String, state: State<'_, AppState>) -> Result<Environment, String> {
    let environment = import_postman_environment(&source).map_err(display_error)?;
    state
        .engine
        .save_environment(&environment)
        .map_err(display_error)?;
    Ok(environment)
}

#[tauri::command]
fn list_environments(state: State<'_, AppState>) -> Result<Vec<Environment>, String> {
    state.engine.environments().map_err(display_error)
}

#[tauri::command]
fn save_environment(environment: Environment, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .save_environment(&environment)
        .map_err(display_error)
}

#[tauri::command]
fn export_workspace_bundle(state: State<'_, AppState>) -> Result<String, String> {
    let collections = state.engine.collections().map_err(display_error)?;
    let environments = state.engine.environments().map_err(display_error)?;
    export_workspace(&collections, &environments).map_err(display_error)
}

#[tauri::command]
fn export_workspace_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let source = export_workspace_bundle(state)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(display_error)?
        .as_secs();
    let path = app
        .path()
        .download_dir()
        .map_err(display_error)?
        .join(format!("apiqa-workspace-{timestamp}.apiqa-workspace"));
    std::fs::write(&path, source).map_err(display_error)?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn import_workspace_bundle(
    source: String,
    state: State<'_, AppState>,
) -> Result<Vec<Collection>, String> {
    let bundle = import_workspace(&source).map_err(display_error)?;
    state
        .engine
        .save_workspace(&bundle.collections, &bundle.environments)
        .map_err(display_error)?;
    Ok(bundle.collections)
}

#[tauri::command]
async fn run_collection(
    collection_id: String,
    baseline_run_id: Option<String>,
    environment_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Run, String> {
    state
        .engine
        .run_saved(
            collection_id,
            None,
            environment_id,
            RunOptions {
                baseline_run_id,
                ..Default::default()
            },
        )
        .await
        .map_err(display_error)
}

#[tauri::command]
async fn run_request(
    collection_id: String,
    request_id: String,
    baseline_run_id: Option<String>,
    environment_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Run, String> {
    state
        .engine
        .run_saved(
            collection_id,
            Some(request_id),
            environment_id,
            RunOptions {
                baseline_run_id,
                ..Default::default()
            },
        )
        .await
        .map_err(display_error)
}

#[tauri::command]
fn list_runs(
    collection_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Run>, String> {
    state
        .engine
        .run_summaries(collection_id.as_deref())
        .map_err(display_error)
}

#[tauri::command]
fn get_run(id: String, state: State<'_, AppState>) -> Result<Run, String> {
    state
        .engine
        .run(&id)
        .map_err(display_error)?
        .ok_or_else(|| "Run not found".to_string())
}

#[tauri::command]
fn set_run_pinned(id: String, pinned: bool, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .set_run_pinned(&id, pinned)
        .map_err(display_error)
}

#[tauri::command]
fn retention_policy(state: State<'_, AppState>) -> Result<RetentionPolicy, String> {
    state.engine.retention_policy().map_err(display_error)
}

#[tauri::command]
fn save_retention_policy(
    policy: RetentionPolicy,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .set_retention_policy(&policy)
        .map_err(display_error)
}

#[tauri::command]
fn cleanup_history(state: State<'_, AppState>) -> Result<CleanupResult, String> {
    let policy = state.engine.retention_policy().map_err(display_error)?;
    state.engine.cleanup_history(&policy).map_err(display_error)
}

#[tauri::command]
async fn discover_android_devices() -> Result<Vec<AndroidDevice>, String> {
    tauri::async_runtime::spawn_blocking(|| Adb::discover()?.devices())
        .await
        .map_err(display_error)?
        .map_err(display_error)
}
#[tauri::command]
async fn list_debuggable_apps(serial: String) -> Result<Vec<AndroidApp>, String> {
    tauri::async_runtime::spawn_blocking(move || Adb::discover()?.debuggable_apps(&serial))
        .await
        .map_err(display_error)?
        .map_err(display_error)
}
#[tauri::command]
async fn generate_capture_ca(state: State<'_, AppState>) -> Result<CertificateInfo, String> {
    let capture = state.capture.clone();
    tauri::async_runtime::spawn_blocking(move || capture.ensure_certificate())
        .await
        .map_err(display_error)?
        .map_err(display_error)
}
#[tauri::command]
async fn prepare_android_ca(serial: String, state: State<'_, AppState>) -> Result<(), String> {
    let capture = state.capture.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let certificate = capture.ensure_certificate()?;
        Adb::discover()?.prepare_certificate_install(&serial, &certificate.path)
    })
    .await
    .map_err(display_error)?
    .map_err(display_error)
}
#[tauri::command]
async fn enable_usb_wifi(serial: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || Adb::discover()?.enable_usb_wifi(&serial))
        .await
        .map_err(display_error)?
        .map_err(display_error)
}
#[tauri::command]
fn capture_status(state: State<'_, AppState>) -> ProxyStatus {
    state.capture.status()
}
#[tauri::command]
fn capture_active(state: State<'_, AppState>) -> bool {
    state
        .configured_device
        .lock()
        .is_ok_and(|device| device.is_some())
}
#[tauri::command]
fn capture_transactions(state: State<'_, AppState>) -> Vec<HttpTransaction> {
    state.capture.transactions()
}
#[tauri::command]
fn capture_logs(state: State<'_, AppState>) -> Vec<LogLine> {
    state.logcat.raw()
}
#[tauri::command]
fn capture_diagnostics(state: State<'_, AppState>) -> Vec<Diagnostic> {
    state.logcat.diagnostics()
}
fn proxy_host(connection_type: ConnectionType) -> Result<String, String> {
    if connection_type == ConnectionType::Emulator {
        return Ok("10.0.2.2".into());
    }
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(display_error)?;
    socket.connect("8.8.8.8:80").map_err(display_error)?;
    Ok(socket.local_addr().map_err(display_error)?.ip().to_string())
}
#[tauri::command]
async fn start_capture(
    serial: String,
    connection_type: ConnectionType,
    package_name: String,
    state: State<'_, AppState>,
) -> Result<ProxyStatus, String> {
    let _lifecycle = state.capture_lifecycle.lock().await;
    if state
        .configured_device
        .lock()
        .map_err(|_| "proxy ownership lock poisoned")?
        .is_some()
    {
        return Err("capture session is already active".into());
    }
    let allowed_client_ip = if connection_type == ConnectionType::Emulator {
        None
    } else {
        let selected_serial = serial.clone();
        match tauri::async_runtime::spawn_blocking(move || {
            Adb::discover()?.wifi_ip(&selected_serial)
        })
        .await
        {
            Ok(result) => Some(result.map_err(display_error)?),
            Err(error) => {
                let _ = cleanup_capture(&state).await;
                return Err(display_error(error));
            }
        }
    };
    *state
        .configured_device
        .lock()
        .map_err(|_| "proxy ownership lock poisoned")? = Some(serial.clone());
    let bind = if connection_type == ConnectionType::Emulator {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };
    let address = match state.capture.start(bind, allowed_client_ip).await {
        Ok(address) => address,
        Err(error) => {
            *state
                .configured_device
                .lock()
                .map_err(|_| "proxy ownership lock poisoned")? = None;
            return Err(display_error(error));
        }
    };
    let host = match proxy_host(connection_type) {
        Ok(host) => host,
        Err(error) => {
            let _ = cleanup_capture(&state).await;
            return Err(error);
        }
    };
    let configured = match tauri::async_runtime::spawn_blocking({
        let serial = serial.clone();
        move || Adb::discover()?.configure_proxy(&serial, &host, address.port())
    })
    .await
    {
        Ok(result) => result.map_err(display_error),
        Err(error) => {
            let error = display_error(error);
            let _ = cleanup_capture(&state).await;
            return Err(error);
        }
    };
    if let Err(error) = configured {
        let _ = cleanup_capture(&state).await;
        return Err(error);
    }
    let adb_result = tauri::async_runtime::spawn_blocking(Adb::discover)
        .await
        .map_err(display_error)
        .and_then(|result| result.map_err(display_error));
    let adb = match adb_result {
        Ok(adb) => adb,
        Err(error) => {
            let _ = cleanup_capture(&state).await;
            return Err(error);
        }
    };
    if let Err(error) = state
        .logcat
        .start_for_app(&adb, serial, &package_name)
        .await
    {
        let _ = cleanup_capture(&state).await;
        return Err(display_error(error));
    }
    Ok(state.capture.status())
}

#[tauri::command]
async fn stop_capture(state: State<'_, AppState>) -> Result<(), String> {
    let _lifecycle = state.capture_lifecycle.lock().await;
    cleanup_capture(&state).await
}

async fn cleanup_capture(state: &AppState) -> Result<(), String> {
    let serial = state
        .configured_device
        .lock()
        .map_err(|_| "proxy ownership lock poisoned")?
        .take();
    state.logcat.stop().await;
    let clear_error = if let Some(serial) = serial.clone() {
        match tauri::async_runtime::spawn_blocking(move || Adb::discover()?.clear_proxy(&serial))
            .await
        {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(display_error(error)),
            Err(error) => Some(display_error(error)),
        }
    } else {
        None
    };
    state.capture.stop().await;
    if let Some(error) = clear_error {
        *state
            .configured_device
            .lock()
            .map_err(|_| "proxy ownership lock poisoned")? = serial;
        Err(error)
    } else {
        Ok(())
    }
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = Store::open(data_dir.join("apiqa.db"))?;
            let policy = store.retention_policy()?;
            store.cleanup_history(&policy)?;
            let capture = Arc::new(CaptureService::new(data_dir.join("capture-ca")));
            let logcat = Arc::new(LogcatService::new());
            let mut capture_events = capture.subscribe();
            let capture_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = recv_broadcast(&mut capture_events).await {
                    let name = match event {
                        CaptureEvent::Status(_) => "capture-status",
                        CaptureEvent::Transaction(_) => "capture-transaction",
                    };
                    let _ = capture_handle.emit(name, event);
                }
            });
            let mut log_events = logcat.subscribe();
            let log_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = recv_broadcast(&mut log_events).await {
                    let name = match event {
                        LogcatEvent::Line(_) => "capture-log-line",
                        LogcatEvent::Diagnostic(_) => "capture-diagnostic",
                        LogcatEvent::Stopped => "capture-logcat-stopped",
                    };
                    let _ = log_handle.emit(name, event);
                }
            });
            app.manage(AppState {
                engine: Arc::new(ApiQaEngine::new(store)),
                capture,
                logcat,
                configured_device: Mutex::new(None),
                capture_lifecycle: tokio::sync::Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_collections,
            import_collection,
            save_collection,
            import_environment,
            list_environments,
            save_environment,
            export_workspace_bundle,
            export_workspace_file,
            import_workspace_bundle,
            run_collection,
            run_request,
            list_runs,
            get_run,
            set_run_pinned,
            retention_policy,
            save_retention_policy,
            cleanup_history,
            discover_android_devices,
            list_debuggable_apps,
            generate_capture_ca,
            prepare_android_ca,
            enable_usb_wifi,
            start_capture,
            stop_capture,
            capture_status,
            capture_active,
            capture_transactions,
            capture_logs,
            capture_diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("error while building APIQA")
        .run(|handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = handle.state::<AppState>();
                // Process shutdown drops logcat child and proxy task; clear owned device best-effort.
                let serial = state
                    .configured_device
                    .lock()
                    .ok()
                    .and_then(|value| value.clone());
                if let Some(serial) = serial
                    && let Ok(adb) = Adb::discover()
                {
                    let _ = adb.clear_proxy(&serial);
                }
            }
        });
}

async fn recv_broadcast<T: Clone>(receiver: &mut tokio::sync::broadcast::Receiver<T>) -> Option<T> {
    loop {
        match receiver.recv().await {
            Ok(event) => return Some(event),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recv_broadcast;

    #[tokio::test]
    async fn broadcast_relay_skips_lag_and_stops_only_when_closed() {
        let (sender, mut receiver) = tokio::sync::broadcast::channel(1);
        sender.send(1).unwrap();
        sender.send(2).unwrap();
        assert_eq!(recv_broadcast(&mut receiver).await, Some(2));
        drop(sender);
        assert_eq!(recv_broadcast(&mut receiver).await, None);
    }
}
