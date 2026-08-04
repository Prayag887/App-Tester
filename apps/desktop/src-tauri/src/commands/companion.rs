//! Companion pairing, install, and capture commands.

use std::{path::PathBuf, time::Duration};

use androidqa_core::{
    android::{
        QrPairingChallenge, QrPairingResult, create_qr_pairing, validate_companion_connection,
    },
    proxy::CompanionApp,
};
use serde::Serialize;
use tauri::{Manager, State};
use uuid::Uuid;

use crate::{adb::adb_blocking, state::InspectorState};

const MINIMUM_COMPANION_VERSION: &str = "0.2.2";
const PROTOCOL_VERSION: u8 = 2;
const COMPANION_DOWNLOAD_URL: &str = "https://github.com/Prayag887/postman-like/releases/download/v0.1.1/app-tester-companion-0.2.2.apk";
/// The GitHub release that publishes the current Companion APK; auto-install
/// falls back to this when the app does not bundle a copy.
const COMPANION_RELEASE_DOWNLOAD_URL: &str =
    "https://github.com/Prayag887/App-Tester/releases/download/v0.2.3/app-tester-companion.apk";
const COMPANION_REGISTRATION_DEADLINE: Duration = Duration::from_secs(12);

#[derive(Debug, Serialize)]
pub struct CompanionInstall {
    install_url: String,
    qr_svg: String,
}

#[derive(serde::Serialize)]
pub struct CompanionConnection {
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

#[derive(serde::Serialize)]
pub struct UsbCompanionConnection {
    session_id: String,
    port: u16,
}

fn qr_svg(payload: &str) -> Result<String, String> {
    Ok(qrcode::QrCode::new(payload.as_bytes())
        .map_err(|error| format!("could not create a QR code: {error}"))?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(320, 320)
        .dark_color(qrcode::render::svg::Color("#08110f"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

#[tauri::command]
pub fn begin_qr_pairing(state: State<'_, InspectorState>) -> Result<QrPairingChallenge, String> {
    let (challenge, secret) = create_qr_pairing().map_err(|error| error.to_string())?;
    state
        .qr_pairings
        .lock()
        .map_err(|_| "QR pairing lock poisoned")?
        .insert(challenge.id, secret);
    Ok(challenge)
}

#[tauri::command]
pub async fn finish_qr_pairing(
    state: State<'_, InspectorState>,
    pairing_id: Uuid,
) -> Result<QrPairingResult, String> {
    let secret = state
        .qr_pairings
        .lock()
        .map_err(|_| "QR pairing lock poisoned")?
        .remove(&pairing_id)
        .ok_or_else(|| "QR pairing request was not found or already used".to_owned())?;
    adb_blocking(move |adb| -> Result<QrPairingResult, String> {
        loop {
            match androidqa_core::android::finish_qr_pairing(adb, &secret)
                .map_err(|error| error.to_string())?
            {
                Some(result) => return Ok(result),
                None => std::thread::sleep(Duration::from_millis(500)),
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn pair_with_code(
    host: String,
    port: u16,
    pairing_code: String,
) -> Result<QrPairingResult, String> {
    adb_blocking(move |adb| {
        androidqa_core::android::pair_with_code(adb, &host, port, &pairing_code)
    })
    .await
}

#[tauri::command]
pub fn prepare_companion_install(app: tauri::AppHandle) -> Result<CompanionInstall, String> {
    companion_apk_path(&app)?;
    Ok(CompanionInstall {
        install_url: COMPANION_DOWNLOAD_URL.into(),
        qr_svg: qr_svg(COMPANION_DOWNLOAD_URL)?,
    })
}

#[tauri::command]
pub fn prepare_companion_connection(
    state: State<'_, InspectorState>,
    host: String,
) -> Result<CompanionConnection, String> {
    validate_companion_connection(&host).map_err(|error| error.to_string())?;
    let token = Uuid::new_v4().simple().to_string();
    let payload = serde_json::to_string(&CompanionConnectionPayload {
        protocol: "app-tester-companion",
        version: PROTOCOL_VERSION,
        host: &host,
        port: state.proxy.configuration().port,
        token: &token,
        minimum_companion_version: MINIMUM_COMPANION_VERSION,
    })
    .map_err(|error| format!("could not encode companion connection: {error}"))?;
    let qr_svg = qr_svg(&payload)?;
    Ok(CompanionConnection {
        payload,
        qr_svg,
        token,
    })
}

#[tauri::command]
pub fn list_companion_apps(state: State<'_, InspectorState>, token: String) -> Vec<CompanionApp> {
    state.proxy.companion_apps(&token)
}

#[tauri::command]
pub fn select_companion_package(
    state: State<'_, InspectorState>,
    token: String,
    package_name: String,
) -> Result<(), String> {
    state
        .proxy
        .select_companion_package(&token, &package_name)
        .map_err(|error| error.to_string())
}

/// Connect a USB device without exposing a pairing code. The device sees the
/// dynamically assigned desktop proxy through `adb reverse` at 127.0.0.1.
/// The Companion registers its installed apps and starts its per-app VPN from
/// the explicit ADB intent. We wait for that registration so the UI only calls
/// the connection established after the desktop has received it.
#[tauri::command]
pub async fn start_usb_companion_capture(
    state: State<'_, InspectorState>,
    serial: String,
    package_name: String,
) -> Result<Vec<CompanionApp>, String> {
    let port = state.proxy.configuration().port;
    let token = Uuid::new_v4().simple().to_string();
    let command_serial = serial.clone();
    let command_package = package_name.clone();
    let command_token = token.clone();
    adb_blocking(move |adb| {
        androidqa_core::android::start_usb_companion_capture(
            adb,
            &command_serial,
            port,
            &command_token,
            &command_package,
        )
    })
    .await?;

    let proxy = state.proxy.clone();
    let deadline = tokio::time::Instant::now() + COMPANION_REGISTRATION_DEADLINE;
    loop {
        let apps = proxy.companion_apps(&token);
        if apps.iter().any(|app| app.package_name == package_name) {
            return Ok(apps);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Companion was opened, but it did not confirm its USB connection. Approve Android's VPN permission if prompted, then try Capture again.".into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tauri::command]
pub async fn open_usb_companion(
    app: tauri::AppHandle,
    state: State<'_, InspectorState>,
    serial: String,
    package_name: String,
) -> Result<UsbCompanionConnection, String> {
    let session_id = state.session.id_or_new();
    state
        .proxy
        .start(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let port = state.proxy.configuration().port;
    let command_serial = serial.clone();
    let command_package = package_name.clone();
    let launch = adb_blocking(move |adb| {
        androidqa_core::android::open_usb_companion(adb, &command_serial, port, &command_package)
    })
    .await;
    // The Companion is not installed on this device yet: install the current
    // release (bundled APK when present, otherwise downloaded from the GitHub
    // release) and retry the launch automatically.
    if let Err(error) = &launch {
        if is_missing_companion_error(error) {
            ensure_companion_installed(&app, &serial).await?;
            let retry_serial = serial.clone();
            let retry_package = package_name.clone();
            adb_blocking(move |adb| {
                androidqa_core::android::open_usb_companion(
                    adb,
                    &retry_serial,
                    port,
                    &retry_package,
                )
            })
            .await?;
        } else {
            return Err(error.clone());
        }
    }
    state.session.set_companion_device(serial);
    Ok(UsbCompanionConnection {
        session_id: session_id.to_string(),
        port,
    })
}

/// True when adb's error text means the Companion activity is not installed,
/// as opposed to an authorization, offline, or intent-argument failure.
fn is_missing_companion_error(message: &str) -> bool {
    message.contains("does not exist") && message.contains("dev.prayag.apptester.companion")
}

/// Makes sure the Companion APK is installed on `serial`: the bundled copy
/// when the app ships with one, otherwise the current release downloaded from
/// GitHub. Returns the install output.
async fn ensure_companion_installed(
    app: &tauri::AppHandle,
    serial: &str,
) -> Result<String, String> {
    let apk = match companion_apk_path(app) {
        Ok(path) => path,
        Err(_) => download_companion_apk(app).await?,
    };
    let apk_argument = apk
        .to_str()
        .ok_or_else(|| "companion APK path contains unsupported characters".to_string())?
        .to_owned();
    let install_serial = serial.to_owned();
    adb_blocking(move |adb| {
        adb.run(&["-s", &install_serial, "install", "-r", &apk_argument])
            .map_err(|error| error.to_string())
    })
    .await
}

/// Downloads the current Companion release APK into the app cache directory.
async fn download_companion_apk(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("could not locate the app cache: {error}"))?;
    std::fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;
    let destination = cache_dir.join("app-tester-companion.apk");
    let bytes = reqwest::get(COMPANION_RELEASE_DOWNLOAD_URL)
        .await
        .map_err(|error| format!("could not download the Companion release: {error}"))?
        .bytes()
        .await
        .map_err(|error| format!("could not read the Companion release: {error}"))?;
    if bytes.is_empty() {
        return Err("the Companion release download returned an empty APK".into());
    }
    std::fs::write(&destination, &bytes).map_err(|error| error.to_string())?;
    Ok(destination)
}

#[tauri::command]
pub async fn stop_usb_companion_capture(serial: String) -> Result<(), String> {
    adb_blocking(move |adb| androidqa_core::android::stop_usb_companion_capture(adb, &serial)).await
}

#[tauri::command]
pub async fn install_companion(app: tauri::AppHandle, serial: String) -> Result<String, String> {
    let apk_path = companion_apk_path(&app)?;
    adb_blocking(move |adb| {
        let apk = apk_path
            .to_str()
            .ok_or_else(|| "companion APK path contains unsupported characters".to_string())?;
        adb.run(&["-s", &serial, "install", "-r", apk])
            .map_err(|error| error.to_string())
    })
    .await
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

#[cfg(test)]
mod tests {
    use super::is_missing_companion_error;

    #[test]
    fn recognizes_a_missing_companion_activity_error() {
        assert!(is_missing_companion_error(
            "ADB failed: Error type 3 Error: Activity class {dev.prayag.apptester.companion/dev.prayag.apptester.companion.MainActivity} does not exist."
        ));
    }

    #[test]
    fn does_not_misclassify_unrelated_adb_errors() {
        assert!(!is_missing_companion_error("device 'R58M123' not found"));
        assert!(!is_missing_companion_error(
            "Error type 3 Error: Activity class {com.example.app/.Main} does not exist."
        ));
        assert!(!is_missing_companion_error(""));
    }
}
