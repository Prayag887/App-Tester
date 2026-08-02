use androidqa_core::{
    AdbRunner, AndroidApp, AndroidDevice, AuthorizationStatus, ConnectionType, ProcessAdb, android,
    android::AndroidCertificateInstall,
    events::{EventBroadcaster, InspectorEvent},
    launch_app, list_devices, list_third_party_apps,
    persistence::Database,
    proxy::{CertificateInfo, ProxyConfiguration, ProxyService, ProxyStatus, generate_ca},
    replay::ReplaySummary,
    traffic::HttpTransaction,
};
use serde::Serialize;
use std::{
    collections::VecDeque,
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

const COMPANION_PACKAGE: &str = "dev.prayag.apptester.companion";
const MINIMUM_COMPANION_VERSION_CODE: u64 = 9;
const PROXY_PORT: u16 = 8080;

#[derive(Debug, Serialize)]
struct CompanionStatus {
    installed: bool,
    package_name: &'static str,
}

struct InspectorState {
    proxy: Arc<ProxyService>,
    database: Arc<Database>,
    session_id: Mutex<Option<Uuid>>,
    ca_directory: std::path::PathBuf,
    logcat_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    configured_device: Mutex<Option<String>>,
    configured_device_path: std::path::PathBuf,
}

#[tauri::command]
async fn discover_devices() -> Result<Vec<AndroidDevice>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        list_devices(&adb)
            .map(|devices| {
                devices
                    .into_iter()
                    .filter(|device| device.connection_type == ConnectionType::Usb)
                    .collect()
            })
            .map_err(|error| error.to_string())
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

fn authorized_usb(adb: &ProcessAdb, serial: &str) -> Result<(), String> {
    let connected = list_devices(adb)
        .map_err(|error| error.to_string())?
        .into_iter()
        .any(|device| {
            device.serial == serial
                && device.connection_type == ConnectionType::Usb
                && device.authorization_status == AuthorizationStatus::Authorized
        });
    connected
        .then_some(())
        .ok_or_else(|| "connect and authorize this phone over USB".into())
}

#[tauri::command]
async fn get_companion_status(serial: String) -> Result<CompanionStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        authorized_usb(&adb, &serial)?;
        let installed = android::package_installed(&adb, &serial, COMPANION_PACKAGE)
            .map_err(|error| error.to_string())?
            && android::package_version_code(&adb, &serial, COMPANION_PACKAGE)
                .map_err(|error| error.to_string())?
                .is_some_and(|version| version >= MINIMUM_COMPANION_VERSION_CODE);
        Ok(CompanionStatus {
            installed,
            package_name: COMPANION_PACKAGE,
        })
    })
    .await
    .map_err(|error| format!("companion status task failed: {error}"))?
}

#[tauri::command]
async fn install_companion(
    app: tauri::AppHandle,
    serial: String,
) -> Result<CompanionStatus, String> {
    let apk_path = companion_apk_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        authorized_usb(&adb, &serial)?;
        let existing_version = android::package_version_code(&adb, &serial, COMPANION_PACKAGE)
            .map_err(|error| error.to_string())?;
        if existing_version.is_some_and(|version| version >= MINIMUM_COMPANION_VERSION_CODE) {
            return Ok(CompanionStatus {
                installed: true,
                package_name: COMPANION_PACKAGE,
            });
        }
        let apk = apk_path
            .to_str()
            .ok_or_else(|| "companion APK path contains unsupported characters".to_string())?;
        let mut install_args = vec!["-s", &serial, "install"];
        if existing_version.is_some() {
            install_args.push("-r");
        }
        install_args.push(apk);
        adb.run(&install_args).map_err(|error| error.to_string())?;
        let installed = android::package_installed(&adb, &serial, COMPANION_PACKAGE)
            .map_err(|error| error.to_string())?;
        installed
            .then_some(CompanionStatus {
                installed,
                package_name: COMPANION_PACKAGE,
            })
            .ok_or_else(|| "companion install finished but package was not found".into())
    })
    .await
    .map_err(|error| format!("companion install task failed: {error}"))?
}

#[tauri::command]
async fn open_companion(
    state: tauri::State<'_, InspectorState>,
    serial: String,
    package_name: Option<String>,
) -> Result<(), String> {
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    if !certificate_path.exists() {
        generate_ca(&state.ca_directory).map_err(|error| error.to_string())?;
    }
    let ca_pem = std::fs::read_to_string(&certificate_path)
        .map_err(|error| format!("could not read App Tester CA: {error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        authorized_usb(&adb, &serial)?;
        if !android::package_installed(&adb, &serial, COMPANION_PACKAGE)
            .map_err(|error| error.to_string())?
        {
            return Err("install App Tester Companion before opening it".into());
        }
        adb.push(
            &serial,
            &certificate_path,
            "/sdcard/Download/AppTester-HTTPS-CA.pem",
        )
        .map_err(|error| error.to_string())?;
        let package_name = package_name.filter(|package| !package.trim().is_empty());
        if package_name.is_some() {
            android::configure_usb_relay(&adb, &serial, PROXY_PORT)
                .map_err(|error| error.to_string())?;
        }
        android::launch_usb_companion(
            &adb,
            &serial,
            COMPANION_PACKAGE,
            package_name.as_deref(),
            PROXY_PORT,
            &ca_pem,
        )
        .map_err(|error| error.to_string())?;
        if package_name.is_some() {
            for _ in 0..120 {
                if android::companion_vpn_active(&adb, &serial, COMPANION_PACKAGE)
                    .map_err(|error| error.to_string())?
                {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            return Err("Companion VPN did not start within 30 seconds. Approve the VPN prompt on the phone, then start capture again.".into());
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("companion launch task failed: {error}"))?
}

#[tauri::command]
async fn remove_usb_relay(serial: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let adb = ProcessAdb::discover().map_err(|error| error.to_string())?;
        android::remove_usb_relay(&adb, &serial, PROXY_PORT).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("USB relay cleanup task failed: {error}"))?
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
    development
        .is_file()
        .then_some(development)
        .ok_or_else(|| "The signed App Tester Companion release APK is missing.".into())
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
    state.proxy.configuration().clone()
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
        let mut child = match Command::new(&adb_path)
            .args([
                "-s",
                &serial,
                "logcat",
                "-T",
                "1",
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
            Err(_) => return,
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
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
                        if pending.len() >= 200 {
                            emit_logcat_incident(
                                &events,
                                session_id,
                                &package_name,
                                &adb_path,
                                &serial,
                                std::mem::take(&mut pending),
                            )
                            .await;
                            context.clear();
                        }
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
                    context.clear();
                }
                Err(_) => {}
            }
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
            get_companion_status,
            install_companion,
            open_companion,
            remove_usb_relay,
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
            test_yesterdays_apis,
            get_transaction,
            approve_baseline
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
