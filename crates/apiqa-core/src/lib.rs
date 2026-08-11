//! Core Android traffic inspection, device control, diagnostics, and replay.
//!
//! Production code must not panic: unwrap/expect/panic are denied outside
//! `#[cfg(test)]` so every fallible path is handled explicitly.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod adb;
pub mod android;
pub mod application;
pub mod comparison;
pub mod composer;
pub mod correlation;
pub mod diagnostics;
pub mod domain;
pub mod events;
pub mod infrastructure;
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

#[cfg(test)]
mod tests {
    /// Verify that domain modules do not import infrastructure crates or modules.
    /// Run after the `domain/` directory is created in the architecture refactor.
    /// Until then this test is a no-op guard; it activates once domain source files
    /// exist so no one accidentally couples domain to rusqlite, reqwest, tauri,
    /// hudsucker, or persistence.
    #[test]
    fn domain_does_not_import_infrastructure() {
        let domain_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/domain");
        if !domain_dir.exists() {
            // domain/ does not exist yet — the guard is dormant.
            return;
        }
        let forbidden: &[&str] = &[
            "use crate::persistence",
            "use crate::proxy",
            "use crate::adb",
            "use rusqlite",
            "use reqwest",
            "use hudsucker",
            "use tauri",
            "std::process::Command",
        ];
        for entry in walkdir::WalkDir::new(&domain_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "rs")
            })
        {
            let contents = std::fs::read_to_string(entry.path()).unwrap_or_else(|error| {
                panic!("could not read {}: {error}", entry.path().display())
            });
            for pattern in forbidden {
                assert!(
                    !contents.contains(pattern),
                    "domain/{} imports forbidden pattern: {pattern}",
                    entry.path().display()
                );
            }
        }
    }

    use std::path::PathBuf;
}
