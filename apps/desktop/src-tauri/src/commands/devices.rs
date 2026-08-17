//! Device discovery and Android proxy configuration commands.

use crate::adb::adb_blocking;
use androidqa_core::{AndroidApp, AndroidDevice, launch_app, list_devices, list_third_party_apps};

#[tauri::command]
pub async fn discover_devices() -> Result<Vec<AndroidDevice>, String> {
    adb_blocking(list_devices).await
}

#[tauri::command]
pub async fn list_installed_apps(serial: String) -> Result<Vec<AndroidApp>, String> {
    adb_blocking(move |adb| list_third_party_apps(adb, &serial)).await
}

#[tauri::command]
pub async fn launch_installed_app(serial: String, package_name: String) -> Result<(), String> {
    adb_blocking(move |adb| launch_app(adb, &serial, &package_name)).await
}

/// Captures the Android screen as a base64 PNG data URI for the mirror panel.
/// `exec-out` guarantees raw binary output, unlike `shell` which mangles it.
#[tauri::command]
pub async fn capture_screen(serial: String) -> Result<String, String> {
    let bytes =
        adb_blocking(move |adb| adb.run(&["-s", &serial, "exec-out", "screencap", "-p"])).await?;
    if bytes.trim().is_empty() {
        return Err("screen capture returned no image; is the device awake?".into());
    }
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes.as_bytes());
    Ok(format!("data:image/png;base64,{encoded}"))
}
