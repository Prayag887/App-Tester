//! Core Android traffic inspection, device control, diagnostics, and replay.

pub mod adb;
pub mod android;
pub mod comparison;
pub mod correlation;
pub mod diagnostics;
pub mod events;
pub mod issues;
pub mod persistence;
pub mod proxy;
pub mod replay;
pub mod session;
pub mod traffic;

pub use adb::{
    AdbRunner, AndroidApp, AndroidDevice, AuthorizationStatus, ConnectionType, DeviceError,
    ProcessAdb, classify_connection, discover_adb_path, launch_app, list_devices,
    list_third_party_apps, parse_device_list, parse_launcher_activity, parse_package_list,
    parse_package_version,
};
