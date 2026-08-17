//! USB Companion installation and capture commands.

use std::{path::PathBuf, time::Duration};

use androidqa_core::proxy::ProxyService;
use tauri::{Manager, State};
use uuid::Uuid;

use crate::{adb::adb_blocking, state::InspectorState};

const COMPANION_REGISTRATION_DEADLINE: Duration = Duration::from_secs(12);

#[derive(serde::Serialize)]
pub struct UsbCompanionConnection {
    session_id: String,
    port: u16,
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
    let token = Uuid::new_v4().simple().to_string();
    let capture_result = async {
        let command_serial = serial.clone();
        let command_package = package_name.clone();
        let command_token = token.clone();
        let launch = adb_blocking(move |adb| {
            androidqa_core::android::start_usb_companion_capture(
                adb,
                &command_serial,
                port,
                &command_token,
                &command_package,
            )
        })
        .await;
        // Install the exact Companion bundled with this desktop build when it
        // is missing, then perform the one allowed launch retry.
        if let Err(error) = &launch {
            if is_missing_companion_error(error) {
                ensure_companion_installed(&app, &serial).await?;
                let retry_serial = serial.clone();
                let retry_package = package_name.clone();
                let retry_token = token.clone();
                adb_blocking(move |adb| {
                    androidqa_core::android::start_usb_companion_capture(
                        adb,
                        &retry_serial,
                        port,
                        &retry_token,
                        &retry_package,
                    )
                })
                .await?;
            } else {
                return Err(error.clone());
            }
        }
        wait_for_registration(state.proxy.clone(), &token, &package_name).await
    }
    .await;
    if let Err(error) = capture_result {
        state.proxy.stop().await;
        return Err(error);
    }
    state.session.set_companion_device(serial);
    Ok(UsbCompanionConnection {
        session_id: session_id.to_string(),
        port,
    })
}

async fn wait_for_registration(
    proxy: std::sync::Arc<ProxyService>,
    token: &str,
    package_name: &str,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + COMPANION_REGISTRATION_DEADLINE;
    loop {
        if proxy
            .companion_apps(token)
            .iter()
            .any(|app| app.package_name == package_name)
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("The Companion did not confirm its USB connection. Approve Android's VPN permission, keep USB connected, then start capture again.".into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// True when adb's error text means the Companion activity is not installed,
/// as opposed to an authorization, offline, or intent-argument failure.
fn is_missing_companion_error(message: &str) -> bool {
    message.contains("does not exist") && message.contains("dev.prayag.apptester.companion")
}

/// Installs the exact Companion APK bundled with this desktop build.
async fn ensure_companion_installed(
    app: &tauri::AppHandle,
    serial: &str,
) -> Result<String, String> {
    let apk = companion_apk_path(app)?;
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
