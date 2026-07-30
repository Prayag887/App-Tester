use androidqa_core::{
    AdbRunner, AndroidApp, AndroidDevice, ProcessAdb, android,
    android::{AndroidCertificateInstall, QrPairingChallenge, QrPairingResult, QrPairingSecret},
    comparison::ComparisonRules,
    events::{EventBroadcaster, InspectorEvent},
    launch_app, list_devices, list_third_party_apps,
    persistence::Database,
    proxy::{
        CertificateInfo, CompanionApp, ProxyConfiguration, ProxyService, ProxyStatus, generate_ca,
    },
    replay::ReplaySummary,
    traffic::HttpTransaction,
};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    net::UdpSocket,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager};
use time::{OffsetDateTime, PrimitiveDateTime, Time};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum AndroidCaState {
    Installed,
    NotInstalled,
    Unknown,
}

#[derive(Debug, Serialize)]
struct AndroidCaStatus {
    state: AndroidCaState,
    can_manage_automatically: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct AndroidCaChange {
    status: AndroidCaStatus,
    requires_user_confirmation: bool,
    rebooting: bool,
}

#[derive(Debug, Serialize)]
struct CompanionInstall {
    install_url: String,
    qr_svg: String,
}

#[derive(serde::Serialize)]
struct CompanionConnection {
    payload: String,
    qr_svg: String,
    token: String,
}

#[derive(serde::Serialize)]
struct CompanionConnectionPayload<'a> {
    protocol: &'static str,
    version: u8,
    host: &'a str,
    port: u16,
    token: &'a str,
    minimum_companion_version: &'static str,
}

struct InspectorState {
    proxy: Arc<ProxyService>,
    database: Arc<Database>,
    session_id: Mutex<Option<Uuid>>,
    ca_directory: std::path::PathBuf,
    qr_pairings: Mutex<HashMap<Uuid, QrPairingSecret>>,
    logcat_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    configured_device: Mutex<Option<String>>,
    configured_device_path: std::path::PathBuf,
}

#[tauri::command]
async fn discover_devices() -> Result<Vec<AndroidDevice>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        list_devices(&adb).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("device discovery task failed: {error}"))?
}

#[tauri::command]
async fn list_installed_apps(serial: String) -> Result<Vec<AndroidApp>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        list_third_party_apps(&adb, &serial).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("application discovery task failed: {error}"))?
}

#[tauri::command]
async fn launch_installed_app(serial: String, package_name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        launch_app(&adb, &serial, &package_name).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("application launch task failed: {error}"))?
}

#[tauri::command]
fn begin_qr_pairing(state: tauri::State<'_, InspectorState>) -> Result<QrPairingChallenge, String> {
    let (challenge, secret) = android::create_qr_pairing().map_err(|error| error.to_string())?;
    state
        .qr_pairings
        .lock()
        .map_err(|_| "QR pairing lock poisoned")?
        .insert(challenge.id, secret);
    Ok(challenge)
}

#[tauri::command]
fn prepare_companion_install(app: tauri::AppHandle) -> Result<CompanionInstall, String> {
    companion_apk_path(&app)?;
    let install_url = "https://github.com/Prayag887/postman-like/releases/download/v0.1.1/app-tester-companion-0.2.2.apk".to_string();
    let qr_svg = qrcode::QrCode::new(install_url.as_bytes())
        .map_err(|error| format!("could not create the companion install QR code: {error}"))?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(320, 320)
        .dark_color(qrcode::render::svg::Color("#08110f"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    Ok(CompanionInstall {
        install_url,
        qr_svg,
    })
}

#[tauri::command]
fn prepare_companion_connection(
    state: tauri::State<'_, InspectorState>,
    host: String,
) -> Result<CompanionConnection, String> {
    android::validate_companion_connection(&host).map_err(|error| error.to_string())?;
    let token = Uuid::new_v4().simple().to_string();
    let payload = serde_json::to_string(&CompanionConnectionPayload {
        protocol: "app-tester-companion",
        version: 2,
        host: &host,
        port: state.proxy.configuration().port,
        token: &token,
        minimum_companion_version: "0.2.2",
    })
    .map_err(|error| format!("could not encode companion connection: {error}"))?;
    let qr_svg = qrcode::QrCode::new(payload.as_bytes())
        .map_err(|error| format!("could not create companion connection QR code: {error}"))?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(320, 320)
        .dark_color(qrcode::render::svg::Color("#08110f"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    Ok(CompanionConnection {
        payload,
        qr_svg,
        token,
    })
}

#[tauri::command]
fn list_companion_apps(
    state: tauri::State<'_, InspectorState>,
    token: String,
) -> Vec<CompanionApp> {
    state.proxy.companion_apps(&token)
}

#[tauri::command]
fn select_companion_package(
    state: tauri::State<'_, InspectorState>,
    token: String,
    package_name: String,
) -> Result<(), String> {
    state
        .proxy
        .select_companion_package(&token, &package_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn install_companion(app: tauri::AppHandle, serial: String) -> Result<String, String> {
    let apk_path = companion_apk_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        let apk = apk_path
            .to_str()
            .ok_or_else(|| "companion APK path contains unsupported characters".to_string())?;
        adb.run(&["-s", &serial, "install", "-r", apk])
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("companion install task failed: {error}"))?
}

fn companion_apk_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?
        .join("_up_/_up_/companion/releases/app-tester-companion.apk");
    if bundled.is_file() {
        return Ok(bundled);
    }
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../companion/releases/app-tester-companion.apk");
    development.is_file().then_some(development).ok_or_else(|| {
        "App Tester Companion has not been built yet. Build apps/companion first.".into()
    })
}

#[tauri::command]
async fn finish_qr_pairing(
    state: tauri::State<'_, InspectorState>,
    pairing_id: Uuid,
) -> Result<QrPairingResult, String> {
    let secret = state
        .qr_pairings
        .lock()
        .map_err(|_| "QR pairing lock poisoned")?
        .remove(&pairing_id)
        .ok_or_else(|| "QR pairing request was not found or already used".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        loop {
            match android::finish_qr_pairing(&adb, &secret).map_err(|error| error.to_string())? {
                Some(result) => return Ok(result),
                None => std::thread::sleep(Duration::from_millis(500)),
            }
        }
    })
    .await
    .map_err(|error| format!("QR pairing task failed: {error}"))?
}

#[tauri::command]
async fn pair_with_code(
    host: String,
    port: u16,
    pairing_code: String,
) -> Result<QrPairingResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        android::pair_with_code(&adb, &host, port, &pairing_code).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("pairing-code task failed: {error}"))?
}

#[tauri::command]
async fn enable_usb_wifi(serial: String, port: Option<u16>) -> Result<QrPairingResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        let endpoint = android::prepare_usb_wifi(&adb, &serial, port.unwrap_or(5555))
            .map_err(|error| error.to_string())?;
        android::verify_adb_wifi_endpoint(&endpoint, Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        android::connect_usb_wifi(&adb, &endpoint).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("USB Wi-Fi task failed: {error}"))?
}

#[tauri::command]
async fn prepare_android_certificate_install(
    state: tauri::State<'_, InspectorState>,
    serial: String,
) -> Result<AndroidCertificateInstall, String> {
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    if !certificate_path.exists() {
        generate_ca(&state.ca_directory).map_err(|error| error.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        android::prepare_certificate_install(&adb, &serial, &certificate_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("certificate setup task failed: {error}"))?
}

fn certificate_hash(certificate_path: &std::path::Path) -> Result<String, String> {
    let output = std::process::Command::new("openssl")
        .args(["x509", "-subject_hash_old", "-noout", "-in"])
        .arg(certificate_path)
        .output()
        .map_err(|error| format!("could not inspect the local CA: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|_| "OpenSSL returned an invalid certificate hash".to_owned())
}

fn root_ca_path(certificate_path: &std::path::Path) -> Result<String, String> {
    Ok(format!(
        "/data/misc/user/0/cacerts-added/{}.0",
        certificate_hash(certificate_path)?
    ))
}

fn inspect_android_ca(
    adb: &ProcessAdb,
    serial: &str,
    certificate_path: &std::path::Path,
) -> AndroidCaStatus {
    let Ok(path) = root_ca_path(certificate_path) else {
        return AndroidCaStatus {
            state: AndroidCaState::Unknown,
            can_manage_automatically: false,
            detail: "The local CA has not been generated yet.".into(),
        };
    };
    let command = format!("test -f {path} && echo installed || echo missing");
    match androidqa_core::AdbRunner::run(
        adb,
        &["-s", serial, "shell", "su", "0", "sh", "-c", &command],
    ) {
        Ok(output) => match parse_root_ca_probe(&output) {
            Some(true) => AndroidCaStatus {
                state: AndroidCaState::Installed,
                can_manage_automatically: true,
                detail: "App Tester CA is installed in Android's user trust store.".into(),
            },
            Some(false) => AndroidCaStatus {
                state: AndroidCaState::NotInstalled,
                can_manage_automatically: true,
                detail: "App Tester CA is not installed on this rooted device.".into(),
            },
            None => protected_ca_status(),
        },
        Err(_) => protected_ca_status(),
    }
}

fn parse_root_ca_probe(output: &str) -> Option<bool> {
    match output.trim() {
        "installed" => Some(true),
        "missing" => Some(false),
        _ => None,
    }
}

fn protected_ca_status() -> AndroidCaStatus {
    AndroidCaStatus {
            state: AndroidCaState::Unknown,
            can_manage_automatically: false,
            detail: "Android protects the user CA store on this device. Installation status requires on-device confirmation.".into(),
    }
}

#[tauri::command]
async fn get_android_ca_status(
    state: tauri::State<'_, InspectorState>,
    serial: String,
    connection_type: String,
) -> Result<AndroidCaStatus, String> {
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    if !certificate_path.exists() {
        return Ok(AndroidCaStatus {
            state: AndroidCaState::NotInstalled,
            can_manage_automatically: connection_type == "emulator",
            detail: "The App Tester CA has not been generated yet.".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        if connection_type == "emulator" {
            let _ = androidqa_core::AdbRunner::run(&adb, &["-s", &serial, "root"]);
            let _ = androidqa_core::AdbRunner::run(&adb, &["-s", &serial, "wait-for-device"]);
        }
        Ok(inspect_android_ca(&adb, &serial, &certificate_path))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn set_android_ca_usage(
    state: tauri::State<'_, InspectorState>,
    serial: String,
    connection_type: String,
    use_ca: bool,
) -> Result<AndroidCaChange, String> {
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    if !certificate_path.exists() {
        generate_ca(&state.ca_directory).map_err(|error| error.to_string())?;
    }
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        if connection_type == "emulator" {
            androidqa_core::AdbRunner::run(&adb, &["-s", &serial, "root"])
                .map_err(|error| error.to_string())?;
            androidqa_core::AdbRunner::run(&adb, &["-s", &serial, "wait-for-device"])
                .map_err(|error| error.to_string())?;
        }
        let current = inspect_android_ca(&adb, &serial, &certificate_path);
        if current.can_manage_automatically {
            let path = root_ca_path(&certificate_path)?;
            if use_ca {
                let temporary = "/data/local/tmp/app-tester-ca.pem";
                androidqa_core::AdbRunner::push(&adb, &serial, &certificate_path, temporary)
                    .map_err(|error| error.to_string())?;
                let command = format!(
                    "cp {temporary} {path} && chmod 644 {path} && chown system:system {path}"
                );
                androidqa_core::AdbRunner::run(
                    &adb,
                    &["-s", &serial, "shell", "su", "0", "sh", "-c", &command],
                )
                .map_err(|error| error.to_string())?;
            } else {
                let command = format!("rm -f {path}");
                androidqa_core::AdbRunner::run(
                    &adb,
                    &["-s", &serial, "shell", "su", "0", "sh", "-c", &command],
                )
                .map_err(|error| error.to_string())?;
            }
            androidqa_core::AdbRunner::run(&adb, &["-s", &serial, "reboot"])
                .map_err(|error| error.to_string())?;
            return Ok(AndroidCaChange {
                status: AndroidCaStatus {
                    state: if use_ca {
                        AndroidCaState::Installed
                    } else {
                        AndroidCaState::NotInstalled
                    },
                    can_manage_automatically: true,
                    detail: if use_ca {
                        "CA installed. Android is rebooting to activate it."
                    } else {
                        "CA removed. Android is rebooting to apply the change."
                    }
                    .into(),
                },
                requires_user_confirmation: false,
                rebooting: true,
            });
        }
        if use_ca {
            android::prepare_certificate_install(&adb, &serial, &certificate_path)
                .map_err(|error| error.to_string())?;
        } else {
            let _ = android::clear_proxy(&adb, &serial);
            androidqa_core::AdbRunner::run(
                &adb,
                &[
                    "-s",
                    &serial,
                    "shell",
                    "am",
                    "start",
                    "-a",
                    "android.settings.TRUSTED_CREDENTIALS_USER",
                ],
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(AndroidCaChange {
            status: current,
            requires_user_confirmation: true,
            rebooting: false,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn get_proxy_status(state: tauri::State<'_, InspectorState>) -> ProxyStatus {
    state.proxy.status()
}
#[tauri::command]
fn get_proxy_configuration(state: tauri::State<'_, InspectorState>) -> ProxyConfiguration {
    state.proxy.configuration()
}
#[tauri::command]
fn generate_ca_certificate(
    state: tauri::State<'_, InspectorState>,
) -> Result<CertificateInfo, String> {
    generate_ca(&state.ca_directory).map_err(|error| error.to_string())
}
#[tauri::command]
async fn start_proxy(state: tauri::State<'_, InspectorState>) -> Result<String, String> {
    let session_id = state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")?
        .unwrap_or_else(Uuid::new_v4);
    *state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")? = Some(session_id);
    state
        .proxy
        .start(session_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(session_id.to_string())
}

#[tauri::command]
async fn start_logcat_capture(
    state: tauri::State<'_, InspectorState>,
    serial: String,
    package_name: String,
) -> Result<(), String> {
    if package_name.trim().is_empty() {
        return Ok(());
    }
    let session_id = (*state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")?)
    .ok_or_else(|| "start the proxy before starting log capture".to_owned())?;
    let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
    let uid = android::app_uid(&adb, &serial, &package_name).map_err(|error| error.to_string())?;
    let mut previous = state
        .logcat_task
        .lock()
        .map_err(|_| "logcat lock poisoned")?;
    if let Some(task) = previous.take() {
        task.abort();
    }
    let adb_path = adb.path().to_path_buf();
    let events = state.proxy.events();
    let task = tauri::async_runtime::spawn(async move {
        let mut reconnect_delay = Duration::from_secs(1);
        loop {
            let mut child = match Command::new(&adb_path)
                .args([
                    "-s",
                    &serial,
                    "logcat",
                    &format!("--uid={uid}"),
                    "-v",
                    "epoch",
                ])
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => child,
                Err(_) => {
                    tokio::time::sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(15));
                    continue;
                }
            };
            let Some(stdout) = child.stdout.take() else {
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(15));
                continue;
            };
            reconnect_delay = Duration::from_secs(1);
            let mut lines = BufReader::new(stdout).lines();
            let mut context = VecDeque::with_capacity(50);
            let mut pending = Vec::new();
            loop {
                match tokio::time::timeout(Duration::from_millis(700), lines.next_line()).await {
                    Ok(Ok(Some(line))) => {
                        let Some(log_line) =
                            androidqa_core::diagnostics::parse_logcat_epoch_line(&line)
                        else {
                            continue;
                        };
                        let actionable = androidqa_core::diagnostics::classify(&log_line.message)
                            .is_some()
                            || matches!(log_line.level.as_str(), "W" | "E" | "F" | "A");
                        if pending.is_empty() && actionable {
                            pending.extend(context.iter().cloned());
                        }
                        if !pending.is_empty() {
                            pending.push(log_line.clone());
                        }
                        context.push_back(log_line);
                        if context.len() > 50 {
                            context.pop_front();
                        }
                    }
                    Ok(Ok(None)) | Ok(Err(_)) => {
                        emit_logcat_incident(
                            &events,
                            session_id,
                            &package_name,
                            &adb_path,
                            &serial,
                            pending,
                        )
                        .await;
                        break;
                    }
                    Err(_) if !pending.is_empty() => {
                        emit_logcat_incident(
                            &events,
                            session_id,
                            &package_name,
                            &adb_path,
                            &serial,
                            std::mem::take(&mut pending),
                        )
                        .await;
                    }
                    Err(_) => {}
                }
            }
            let _ = child.wait().await;
            tokio::time::sleep(reconnect_delay).await;
            reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(15));
        }
    });
    *previous = Some(task);
    Ok(())
}

async fn emit_logcat_incident(
    events: &EventBroadcaster,
    session_id: Uuid,
    package_name: &str,
    adb_path: &std::path::Path,
    serial: &str,
    lines: Vec<androidqa_core::diagnostics::FocusedLogLine>,
) {
    if lines.is_empty() {
        return;
    }
    let foreground_activity = Command::new(adb_path)
        .args(["-s", serial, "shell", "dumpsys", "window", "windows"])
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|output| android::parse_foreground_activity(&output, package_name));
    if let Some(incident) = androidqa_core::diagnostics::parse_incident(
        session_id,
        package_name,
        lines,
        foreground_activity,
    ) {
        events.send(InspectorEvent::IncidentCreated(incident));
    }
}
#[tauri::command]
async fn stop_proxy(state: tauri::State<'_, InspectorState>) -> Result<(), String> {
    if let Some(task) = state
        .logcat_task
        .lock()
        .map_err(|_| "logcat lock poisoned")?
        .take()
    {
        task.abort();
    }
    state.proxy.stop().await;
    Ok(())
}
#[tauri::command]
async fn restart_proxy(state: tauri::State<'_, InspectorState>) -> Result<String, String> {
    state.proxy.stop().await;
    start_proxy(state).await
}
#[tauri::command]
async fn configure_android_proxy(
    state: tauri::State<'_, InspectorState>,
    serial: String,
    host: String,
    port: u16,
) -> Result<(), String> {
    let configured_serial = serial.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|e| e.to_string())?;
        android::configure_proxy(&adb, &serial, &host, port).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Err(error) = std::fs::write(&state.configured_device_path, &configured_serial) {
        if let Ok(adb) = ProcessAdb::discover() {
            let _ = android::clear_proxy(&adb, &configured_serial);
        }
        return Err(format!(
            "could not persist Android proxy ownership: {error}"
        ));
    }
    *state
        .configured_device
        .lock()
        .map_err(|_| "device proxy lock poisoned")? = Some(configured_serial);
    Ok(())
}
#[tauri::command]
async fn clear_android_proxy(
    state: tauri::State<'_, InspectorState>,
    serial: String,
) -> Result<(), String> {
    let cleared_serial = serial.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|e| e.to_string())?;
        android::clear_proxy(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut configured = state
        .configured_device
        .lock()
        .map_err(|_| "device proxy lock poisoned")?;
    if configured.as_deref() == Some(cleared_serial.as_str()) {
        *configured = None;
        let _ = std::fs::remove_file(&state.configured_device_path);
    }
    Ok(())
}
#[tauri::command]
fn get_proxy_host(connection_type: String) -> Result<String, String> {
    if connection_type == "emulator" {
        return Ok("10.0.2.2".into());
    }
    lan_ipv4()
}

fn lan_ipv4() -> Result<String, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|error| format!("could not inspect the Mac network: {error}"))?;
    socket
        .connect("8.8.8.8:80")
        .map_err(|error| format!("could not determine the Mac Wi-Fi address: {error}"))?;
    match socket.local_addr().map_err(|error| error.to_string())?.ip() {
        std::net::IpAddr::V4(address) if !address.is_loopback() => Ok(address.to_string()),
        _ => Err("could not determine an IPv4 address reachable from the Android device".into()),
    }
}
#[tauri::command]
async fn verify_android_proxy(serial: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|e| e.to_string())?;
        android::verify_proxy(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn list_transactions(
    state: tauri::State<'_, InspectorState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<HttpTransaction>, String> {
    let Some(session_id) = *state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")?
    else {
        return Ok(vec![]);
    };
    state
        .database
        .list_transactions(session_id, limit.unwrap_or(250), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_all_transactions(state: tauri::State<'_, InspectorState>) -> Result<(), String> {
    state
        .database
        .delete_all_transactions()
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn export_capture(state: tauri::State<'_, InspectorState>) -> Result<String, String> {
    let session_id = (*state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")?)
    .ok_or_else(|| "capture something before exporting".to_owned())?;
    let transactions = state
        .database
        .all_session_transactions(session_id)
        .map_err(|error| error.to_string())?;
    androidqa_core::persistence::portable::encode_capture(
        &androidqa_core::persistence::portable::export_capture(
            &transactions,
            OffsetDateTime::now_utc(),
        ),
    )
    .map_err(|error| error.to_string())
}
#[tauri::command]
fn export_capture_to_file(state: tauri::State<'_, InspectorState>) -> Result<String, String> {
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
fn import_capture(
    state: tauri::State<'_, InspectorState>,
    payload: String,
) -> Result<usize, String> {
    let session_id = Uuid::new_v4();
    let transactions = androidqa_core::persistence::portable::import_capture(
        &payload,
        session_id,
        OffsetDateTime::now_utc(),
    )
    .map_err(|error| error.to_string())?;
    for transaction in &transactions {
        state
            .database
            .upsert_transaction(transaction)
            .map_err(|error| error.to_string())?;
    }
    *state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")? = Some(session_id);
    Ok(transactions.len())
}
#[tauri::command]
async fn test_yesterdays_apis(
    state: tauri::State<'_, InspectorState>,
) -> Result<ReplaySummary, String> {
    let today = OffsetDateTime::now_utc().date();
    let yesterday = today
        .previous_day()
        .ok_or_else(|| "could not calculate yesterday".to_owned())?;
    let start = PrimitiveDateTime::new(yesterday, Time::MIDNIGHT).assume_utc();
    let end = PrimitiveDateTime::new(today, Time::MIDNIGHT).assume_utc();
    let baselines = state
        .database
        .transactions_between(start, end)
        .map_err(|error| error.to_string())?;
    let session_id = (*state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")?)
    .unwrap_or_else(Uuid::new_v4);
    *state
        .session_id
        .lock()
        .map_err(|_| "session lock poisoned")? = Some(session_id);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let mut summary = ReplaySummary::default();
    for baseline in baselines {
        if androidqa_core::replay::replay_blocker(&baseline).is_some() {
            summary.skipped += 1;
            continue;
        }
        summary.attempted += 1;
        let result = androidqa_core::replay::replay(&client, &baseline, session_id).await;
        if result.state == androidqa_core::traffic::TransactionState::Failed {
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
            .upsert_transaction(&result)
            .map_err(|error| error.to_string())?;
        state
            .proxy
            .events()
            .send(InspectorEvent::TransactionCompleted(result));
    }
    Ok(summary)
}
#[tauri::command]
fn get_transaction(
    state: tauri::State<'_, InspectorState>,
    id: Uuid,
) -> Result<Option<HttpTransaction>, String> {
    state
        .database
        .get_transaction(id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn approve_baseline(
    state: tauri::State<'_, InspectorState>,
    endpoint_id: String,
    transaction_id: Uuid,
) -> Result<(), String> {
    state
        .database
        .approve_baseline(&endpoint_id, transaction_id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_baseline(
    state: tauri::State<'_, InspectorState>,
    endpoint_id: String,
) -> Result<bool, String> {
    state
        .database
        .delete_baseline(&endpoint_id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn get_comparison_rules(
    state: tauri::State<'_, InspectorState>,
    endpoint_id: String,
) -> Result<ComparisonRules, String> {
    state
        .database
        .comparison_rules(&endpoint_id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn save_comparison_rules(
    state: tauri::State<'_, InspectorState>,
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
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database = Arc::new(Database::open(data_dir.join("inspector.sqlite"))?);
            let events = androidqa_core::events::EventBroadcaster::default();
            let ca_directory = data_dir.join("certificate-authority");
            let configured_device_path = data_dir.join("configured-android-proxy");
            if let Ok(serial) = std::fs::read_to_string(&configured_device_path) {
                let serial = serial.trim();
                if !serial.is_empty()
                    && let Ok(adb) = ProcessAdb::discover()
                {
                    let _ = android::clear_proxy(&adb, serial);
                }
                let _ = std::fs::remove_file(&configured_device_path);
            }
            let proxy = Arc::new(ProxyService::new(
                ProxyConfiguration {
                    bind_address: "0.0.0.0".into(),
                    port: 8080,
                    ca_certificate_path: ca_directory.join("app-tester-ca.pem"),
                    ca_fingerprint_sha256: None,
                },
                database.clone(),
                events.clone(),
            ));
            let mut receiver = events.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = receiver.recv().await {
                    let name = match &event {
                        InspectorEvent::ProxyStatusChanged(_) => "proxy-status-changed",
                        InspectorEvent::SessionStatusChanged(_) => "session-status-changed",
                        InspectorEvent::TransactionCreated(_) => "transaction-created",
                        InspectorEvent::TransactionUpdated(_) => "transaction-updated",
                        InspectorEvent::TransactionCompleted(_) => "transaction-completed",
                        InspectorEvent::ComparisonCompleted { .. } => "comparison-completed",
                        InspectorEvent::IncidentCreated(_) => "incident-created",
                        InspectorEvent::IssueCreated(_) => "issue-created",
                        InspectorEvent::DeviceStatusChanged(_) => "device-status-changed",
                    };
                    let _ = handle.emit(name, event);
                }
            });
            app.manage(InspectorState {
                proxy,
                database,
                session_id: Mutex::new(None),
                ca_directory,
                qr_pairings: Mutex::new(HashMap::new()),
                logcat_task: Mutex::new(None),
                configured_device: Mutex::new(None),
                configured_device_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            discover_devices,
            list_installed_apps,
            launch_installed_app,
            begin_qr_pairing,
            prepare_companion_install,
            prepare_companion_connection,
            list_companion_apps,
            select_companion_package,
            install_companion,
            finish_qr_pairing,
            pair_with_code,
            enable_usb_wifi,
            prepare_android_certificate_install,
            get_android_ca_status,
            set_android_ca_usage,
            start_proxy,
            start_logcat_capture,
            stop_proxy,
            restart_proxy,
            get_proxy_status,
            get_proxy_configuration,
            generate_ca_certificate,
            configure_android_proxy,
            clear_android_proxy,
            get_proxy_host,
            verify_android_proxy,
            list_transactions,
            delete_all_transactions,
            export_capture,
            export_capture_to_file,
            import_capture,
            test_yesterdays_apis,
            get_transaction,
            approve_baseline,
            delete_baseline,
            get_comparison_rules,
            save_comparison_rules
        ])
        .build(tauri::generate_context!())
        .expect("failed to build App Tester")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let configured_device = app_handle
                    .state::<InspectorState>()
                    .configured_device
                    .lock()
                    .ok()
                    .and_then(|device| device.clone());
                if let Some(serial) = configured_device
                    && let Ok(adb) = ProcessAdb::discover()
                {
                    let _ = android::clear_proxy(&adb, &serial);
                    let _ = std::fs::remove_file(
                        &app_handle.state::<InspectorState>().configured_device_path,
                    );
                }
            }
        });
}

#[cfg(test)]
mod desktop_tests {
    use super::parse_root_ca_probe;

    #[test]
    fn rejects_shell_errors_as_proof_of_root_access() {
        assert_eq!(parse_root_ca_probe("installed\n"), Some(true));
        assert_eq!(parse_root_ca_probe("missing\n"), Some(false));
        assert_eq!(
            parse_root_ca_probe("/system/bin/sh: su: inaccessible or not found\n"),
            None
        );
    }
}
