//! Android user-trust-store CA management for rooted devices.

use serde::Serialize;
use std::path::Path;

use super::super::adb::DeviceError;
use crate::adb::AdbRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidCaState {
    Installed,
    NotInstalled,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct AndroidCaStatus {
    pub state: AndroidCaState,
    pub can_manage_automatically: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AndroidCaChange {
    pub status: AndroidCaStatus,
    pub requires_user_confirmation: bool,
    pub rebooting: bool,
}

pub fn certificate_hash(certificate_path: &Path) -> Result<String, String> {
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

pub fn root_ca_path(certificate_path: &Path) -> Result<String, String> {
    Ok(format!(
        "/data/misc/user/0/cacerts-added/{}.0",
        certificate_hash(certificate_path)?
    ))
}

pub fn parse_root_ca_probe(output: &str) -> Option<bool> {
    match output.trim() {
        "installed" => Some(true),
        "missing" => Some(false),
        _ => None,
    }
}

pub fn protected_ca_status() -> AndroidCaStatus {
    AndroidCaStatus {
        state: AndroidCaState::Unknown,
        can_manage_automatically: false,
        detail: "Android protects the user CA store on this device. Installation status requires on-device confirmation."
            .into(),
    }
}

pub fn inspect_android_ca(
    runner: &dyn AdbRunner,
    serial: &str,
    certificate_path: &Path,
) -> AndroidCaStatus {
    let Ok(path) = root_ca_path(certificate_path) else {
        return AndroidCaStatus {
            state: AndroidCaState::Unknown,
            can_manage_automatically: false,
            detail: "The local CA has not been generated yet.".into(),
        };
    };
    let command = format!("test -f {path} && echo installed || echo missing");
    match runner.run(&["-s", serial, "shell", "su", "0", "sh", "-c", &command]) {
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

/// Installs or removes the CA in the rooted user store, or falls back to the
/// manual on-device installer when the device does not expose root management.
///
/// Returns an [`AndroidCaChange`] describing whether the device is rebooting
/// or whether the user must confirm installation on screen.
pub fn manage_ca_usage(
    runner: &dyn AdbRunner,
    serial: &str,
    certificate_path: &Path,
    connection_type: &str,
    use_ca: bool,
) -> Result<AndroidCaChange, DeviceError> {
    if connection_type == "emulator" {
        runner.run(&["-s", serial, "root"])?;
        runner.run(&["-s", serial, "wait-for-device"])?;
    }
    let current = inspect_android_ca(runner, serial, certificate_path);
    if current.can_manage_automatically {
        let path = root_ca_path(certificate_path).map_err(DeviceError::Adb)?;
        if use_ca {
            let temporary = "/data/local/tmp/app-tester-ca.pem";
            runner.push(serial, certificate_path, temporary)?;
            let command =
                format!("cp {temporary} {path} && chmod 644 {path} && chown system:system {path}");
            runner.run(&["-s", serial, "shell", "su", "0", "sh", "-c", &command])?;
        } else {
            let command = format!("rm -f {path}");
            runner.run(&["-s", serial, "shell", "su", "0", "sh", "-c", &command])?;
        }
        runner.run(&["-s", serial, "reboot"])?;
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
        super::prepare_certificate_install(runner, serial, certificate_path)?;
    } else {
        let _ = super::clear_proxy(runner, serial);
        runner.run(&[
            "-s",
            serial,
            "shell",
            "am",
            "start",
            "-a",
            "android.settings.TRUSTED_CREDENTIALS_USER",
        ])?;
    }
    Ok(AndroidCaChange {
        status: current,
        requires_user_confirmation: true,
        rebooting: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct ScriptedRunner {
        responses: Arc<Mutex<std::collections::VecDeque<Result<String, DeviceError>>>>,
        calls: Arc<Mutex<Vec<String>>>,
        pushes: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl ScriptedRunner {
        fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(std::collections::VecDeque::new())),
                calls: Arc::new(Mutex::new(Vec::new())),
                pushes: Arc::new(Mutex::new(Vec::new())),
            }
        }
        fn queue(&self, response: Result<String, DeviceError>) {
            self.responses.lock().unwrap().push_back(response);
        }
    }

    impl AdbRunner for ScriptedRunner {
        fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
            self.calls.lock().unwrap().push(args.join(" "));
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(String::new()))
        }
        fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<String, DeviceError> {
            self.pushes
                .lock()
                .unwrap()
                .push((serial.to_owned(), remote.to_owned()));
            let _ = local;
            Ok(String::new())
        }
    }

    fn certificate() -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("app-tester-ca-test-{}", uuid::Uuid::new_v4()));
        let info = crate::proxy::generate_ca(&root).unwrap();
        std::fs::copy(&info.certificate_path, root.join("ca.pem")).unwrap();
        root.join("ca.pem")
    }

    #[test]
    fn parses_root_ca_probe_results() {
        assert_eq!(parse_root_ca_probe("installed\n"), Some(true));
        assert_eq!(parse_root_ca_probe("missing\n"), Some(false));
        assert_eq!(
            parse_root_ca_probe("/system/bin/sh: su: inaccessible or not found\n"),
            None
        );
    }

    #[test]
    fn root_ca_path_uses_the_openssl_subject_hash() {
        let certificate = certificate();
        let path = root_ca_path(&certificate).unwrap();
        assert!(path.starts_with("/data/misc/user/0/cacerts-added/"));
        assert!(path.ends_with(".0"));
        assert_eq!(path.len(), "/data/misc/user/0/cacerts-added/".len() + 8 + 2);
        let _ = std::fs::remove_file(&certificate);
    }

    #[test]
    fn inspect_reports_installed_when_probe_succeeds() {
        let runner = ScriptedRunner::new();
        runner.queue(Ok("installed\n".into()));
        let status = inspect_android_ca(&runner, "R58M123", &certificate());
        assert_eq!(status.state, AndroidCaState::Installed);
        assert!(status.can_manage_automatically);
    }

    #[test]
    fn inspect_reports_protected_when_su_is_unavailable() {
        let runner = ScriptedRunner::new();
        runner.queue(Err(DeviceError::Adb("su: inaccessible".into())));
        let status = inspect_android_ca(&runner, "R58M123", &certificate());
        assert_eq!(status.state, AndroidCaState::Unknown);
        assert!(!status.can_manage_automatically);
        assert!(status.detail.contains("protects"));
    }

    #[test]
    fn auto_install_pushes_copies_and_reboots() {
        let runner = ScriptedRunner::new();
        runner.queue(Ok(String::new())); // emulator root
        runner.queue(Ok(String::new())); // wait-for-device
        runner.queue(Ok("installed\n".into())); // probe
        let change =
            manage_ca_usage(&runner, "emulator-5554", &certificate(), "emulator", true).unwrap();
        assert!(change.rebooting);
        assert!(!change.requires_user_confirmation);
        assert_eq!(change.status.state, AndroidCaState::Installed);
        let calls = runner.calls.lock().unwrap();
        let joined = calls.join("\n");
        assert!(joined.contains("root"), "emulator root must run first");
        assert!(joined.contains("reboot"));
        assert!(joined.contains("cp /data/local/tmp/app-tester-ca.pem"));
        assert!(runner.pushes.lock().unwrap().len() == 1);
    }

    #[test]
    fn auto_remove_deletes_and_reboots_without_pushing() {
        let runner = ScriptedRunner::new();
        runner.queue(Ok("missing\n".into()));
        let change = manage_ca_usage(&runner, "R58M123", &certificate(), "usb", false).unwrap();
        assert!(change.rebooting);
        assert_eq!(change.status.state, AndroidCaState::NotInstalled);
        let joined = runner.calls.lock().unwrap().join("\n");
        assert!(joined.contains("rm -f /data/misc/user/0/cacerts-added/"));
        assert!(runner.pushes.lock().unwrap().is_empty());
    }

    #[test]
    fn protected_device_falls_back_to_manual_installer() {
        let runner = ScriptedRunner::new();
        runner.queue(Err(DeviceError::Adb("su: inaccessible".into())));
        runner.queue(Ok("installing\n".into()));
        runner.queue(Ok(String::new()));
        let change = manage_ca_usage(&runner, "R58M123", &certificate(), "usb", true).unwrap();
        assert!(!change.rebooting);
        assert!(change.requires_user_confirmation);
        let joined = runner.calls.lock().unwrap().join("\n");
        assert!(joined.contains("android.credentials.INSTALL"));
    }

    #[test]
    fn protected_device_removal_clears_proxy_and_opens_credentials() {
        let runner = ScriptedRunner::new();
        runner.queue(Err(DeviceError::Adb("su: inaccessible".into())));
        let change = manage_ca_usage(&runner, "R58M123", &certificate(), "usb", false).unwrap();
        assert!(!change.rebooting);
        assert!(change.requires_user_confirmation);
        let joined = runner.calls.lock().unwrap().join("\n");
        assert!(joined.contains("android.settings.TRUSTED_CREDENTIALS_USER"));
    }
}
