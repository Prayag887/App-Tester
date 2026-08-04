//! Android CA trust-store and proxy certificate commands.

use androidqa_core::{
    android,
    android::{AndroidCaState, AndroidCaStatus, inspect_android_ca, manage_ca_usage},
    proxy::CertificateInfo,
};
use tauri::State;

use crate::{adb::adb_blocking, state::InspectorState};

#[tauri::command]
pub async fn prepare_android_certificate_install(
    state: State<'_, InspectorState>,
    serial: String,
) -> Result<android::AndroidCertificateInstall, String> {
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    if !certificate_path.exists() {
        androidqa_core::proxy::generate_ca(&state.ca_directory)
            .map_err(|error| error.to_string())?;
    }
    adb_blocking(move |adb| android::prepare_certificate_install(adb, &serial, &certificate_path))
        .await
}

#[tauri::command]
pub async fn get_android_ca_status(
    state: State<'_, InspectorState>,
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
    adb_blocking(move |adb| {
        if connection_type == "emulator" {
            let _ = adb.run(&["-s", &serial, "root"]);
            let _ = adb.run(&["-s", &serial, "wait-for-device"]);
        }
        Ok::<_, String>(inspect_android_ca(adb, &serial, &certificate_path))
    })
    .await
}

#[tauri::command]
pub async fn set_android_ca_usage(
    state: State<'_, InspectorState>,
    serial: String,
    connection_type: String,
    use_ca: bool,
) -> Result<android::AndroidCaChange, String> {
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    if !certificate_path.exists() {
        androidqa_core::proxy::generate_ca(&state.ca_directory)
            .map_err(|error| error.to_string())?;
    }
    let certificate_path = state.proxy.configuration().ca_certificate_path.clone();
    adb_blocking(move |adb| {
        manage_ca_usage(adb, &serial, &certificate_path, &connection_type, use_ca)
    })
    .await
}

#[tauri::command]
pub fn get_proxy_status(state: State<'_, InspectorState>) -> androidqa_core::proxy::ProxyStatus {
    state.proxy.status()
}

#[tauri::command]
pub fn get_proxy_configuration(
    state: State<'_, InspectorState>,
) -> androidqa_core::proxy::ProxyConfiguration {
    state.proxy.configuration()
}

#[tauri::command]
pub fn generate_ca_certificate(
    state: State<'_, InspectorState>,
) -> Result<CertificateInfo, String> {
    androidqa_core::proxy::generate_ca(&state.ca_directory).map_err(|error| error.to_string())
}
