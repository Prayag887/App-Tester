//! App Tester desktop shell: Tauri wiring and command registration.
//!
//! All behavior lives in [`state`] (shared state), [`adb`] (cached ADB
//! access), [`bridge`] (event forwarding), and the [`commands`] modules.
//!
//! Production code must not panic: unwrap/expect/panic are denied outside
//! `#[cfg(test)]` so every fallible path is handled explicitly.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod adb;
mod bridge;
mod commands;
mod state;
mod ui_payload;

use std::sync::Arc;

use androidqa_core::{
    android,
    persistence::Database,
    proxy::{ProxyConfiguration, ProxyService},
};
use tauri::Manager;

use commands::{certificate, companion, composer, devices, session as session_commands, traffic};
pub use state::{InspectorState, Session};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
                    && let Ok(adb) = adb::adb()
                {
                    let _ = android::clear_proxy(adb, serial);
                }
                let _ = std::fs::remove_file(&configured_device_path);
            }
            let proxy = Arc::new(ProxyService::new(
                ProxyConfiguration {
                    bind_address: "0.0.0.0".into(),
                    port: 0,
                    ca_certificate_path: ca_directory.join("app-tester-ca.pem"),
                    ca_fingerprint_sha256: None,
                },
                database.clone(),
                events.clone(),
            ));
            let state = InspectorState::new(proxy, database, ca_directory);
            app.manage(state);
            bridge::forward_events(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            devices::discover_devices,
            devices::list_installed_apps,
            devices::launch_installed_app,
            devices::capture_screen,
            companion::open_usb_companion,
            companion::stop_usb_companion_capture,
            companion::install_companion,
            certificate::prepare_android_certificate_install,
            certificate::get_android_ca_status,
            certificate::set_android_ca_usage,
            certificate::generate_ca_certificate,
            certificate::get_proxy_status,
            certificate::get_proxy_configuration,
            session_commands::start_proxy,
            session_commands::stop_proxy,
            session_commands::restart_proxy,
            session_commands::start_logcat_capture,
            session_commands::export_capture,
            session_commands::export_capture_to_file,
            session_commands::import_capture,
            session_commands::test_yesterdays_apis,
            traffic::list_transactions,
            traffic::delete_all_transactions,
            traffic::get_transaction,
            traffic::approve_baseline,
            traffic::delete_baseline,
            traffic::get_comparison_rules,
            traffic::save_comparison_rules,
            composer::parse_curl,
            composer::generate_composer_curl,
            composer::pick_file,
            composer::send_request
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| {
            // The shell cannot run without a valid application: report and
            // exit rather than panicking across the webview boundary.
            eprintln!("failed to build App Tester: {error}");
            std::process::exit(1);
        })
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app_handle.state::<InspectorState>();
                if let Some(serial) = state.session.take_companion_device()
                    && let Ok(adb) = adb::adb()
                {
                    let _ = android::stop_usb_companion_capture(adb, &serial);
                }
            }
        });
}
