//! Device discovery and Android proxy configuration commands.

use androidqa_core::{AndroidApp, AndroidDevice, launch_app, list_devices, list_third_party_apps};
use tauri::State;

use crate::{
    adb::{adb, adb_blocking, lan_ipv4},
    state::InspectorState,
};

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

#[tauri::command]
pub fn get_proxy_host(connection_type: String) -> Result<String, String> {
    if connection_type == "emulator" {
        return Ok("10.0.2.2".into());
    }
    lan_ipv4()
}

#[tauri::command]
pub async fn configure_android_proxy(
    state: State<'_, InspectorState>,
    serial: String,
    host: String,
    port: u16,
) -> Result<(), String> {
    let configured_serial = serial.clone();
    adb_blocking(move |adb| androidqa_core::android::configure_proxy(adb, &serial, &host, port))
        .await?;
    if let Err(error) = std::fs::write(&state.configured_device_path, &configured_serial) {
        if let Ok(adb) = adb() {
            let _ = androidqa_core::android::clear_proxy(adb, &configured_serial);
        }
        return Err(format!(
            "could not persist Android proxy ownership: {error}"
        ));
    }
    state.session.set_configured_device(configured_serial);
    Ok(())
}

#[tauri::command]
pub async fn clear_android_proxy(
    state: State<'_, InspectorState>,
    serial: String,
) -> Result<(), String> {
    let cleared_serial = serial.clone();
    adb_blocking(move |adb| androidqa_core::android::clear_proxy(adb, &serial)).await?;
    state.session.clear_configured_device(&cleared_serial);
    if state.session.configured_device().is_none() {
        let _ = std::fs::remove_file(&state.configured_device_path);
    }
    Ok(())
}

#[tauri::command]
pub async fn verify_android_proxy(serial: String) -> Result<String, String> {
    adb_blocking(move |adb| androidqa_core::android::verify_proxy(adb, &serial)).await
}

#[tauri::command]
pub async fn enable_usb_wifi(
    serial: String,
    port: Option<u16>,
) -> Result<androidqa_core::android::QrPairingResult, String> {
    adb_blocking(move |adb| {
        let endpoint =
            androidqa_core::android::prepare_usb_wifi(adb, &serial, port.unwrap_or(5555))
                .map_err(|error| error.to_string())?;
        androidqa_core::android::verify_adb_wifi_endpoint(
            &endpoint,
            std::time::Duration::from_secs(5),
        )
        .map_err(|error| error.to_string())?;
        androidqa_core::android::connect_usb_wifi(adb, &endpoint).map_err(|error| error.to_string())
    })
    .await
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
