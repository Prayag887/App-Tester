use crate::{AdbRunner, DeviceError};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

mod ca;
pub use ca::{
    AndroidCaChange, AndroidCaState, AndroidCaStatus, certificate_hash, inspect_android_ca,
    manage_ca_usage, parse_root_ca_probe, protected_ca_status, root_ca_path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidCertificateInstall {
    pub remote_path: String,
    pub installer_output: String,
}

pub fn prepare_certificate_install(
    runner: &dyn AdbRunner,
    serial: &str,
    certificate_path: &Path,
) -> Result<AndroidCertificateInstall, DeviceError> {
    if !certificate_path.is_file() {
        return Err(DeviceError::Adb(
            "local CA certificate was not found".into(),
        ));
    }
    let remote_path = "/sdcard/Download/AppTester-HTTPS-CA.pem";
    runner.push(serial, certificate_path, remote_path)?;
    let installer_output = runner.run(&[
        "-s",
        serial,
        "shell",
        "am",
        "start",
        "-a",
        "android.credentials.INSTALL",
    ])?;
    Ok(AndroidCertificateInstall {
        remote_path: remote_path.into(),
        installer_output: installer_output.trim().to_owned(),
    })
}

/// Makes the desktop proxy available to a USB-connected device and asks the
/// installed Companion to register and start its per-app VPN capture. Android
/// sees the desktop endpoint as loopback; the ADB reverse tunnel carries that
/// traffic over USB, so no LAN address, QR exchange, or fixed port is needed.
pub fn start_usb_companion_capture(
    runner: &dyn AdbRunner,
    serial: &str,
    port: u16,
    token: &str,
    target_package: &str,
) -> Result<(), DeviceError> {
    if port == 0 {
        return Err(DeviceError::Adb(
            "proxy must be running before starting the USB companion".into(),
        ));
    }
    if token.is_empty() || target_package.trim().is_empty() {
        return Err(DeviceError::Adb(
            "a companion token and target package are required".into(),
        ));
    }
    let port = port.to_string();
    runner.run(&[
        "-s",
        serial,
        "reverse",
        &format!("tcp:{port}"),
        &format!("tcp:{port}"),
    ])?;
    runner.run(&[
        "-s",
        serial,
        "shell",
        "am",
        "start",
        "-n",
        "dev.prayag.apptester.companion/.MainActivity",
        "--es",
        "app_tester_host",
        "127.0.0.1",
        "--ei",
        "app_tester_port",
        &port,
        "--es",
        "app_tester_token",
        token,
        "--es",
        "app_tester_package",
        target_package,
    ])?;
    Ok(())
}

/// Tells the USB-connected Companion to close its capture VPN. This is an
/// explicit lifecycle handoff instead of relying on the Companion's endpoint
/// watchdog, which can otherwise leave the VPN active for up to its next
/// health-check cycle after desktop capture ends.
pub fn stop_usb_companion_capture(runner: &dyn AdbRunner, serial: &str) -> Result<(), DeviceError> {
    runner.run(&[
        "-s",
        serial,
        "shell",
        "am",
        "start",
        "-n",
        "dev.prayag.apptester.companion/.MainActivity",
        "--ez",
        "app_tester_stop_vpn",
        "true",
    ])?;
    Ok(())
}

pub fn clear_proxy_command(serial: &str) -> Vec<String> {
    vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "settings".into(),
        "put".into(),
        "global".into(),
        "http_proxy".into(),
        ":0".into(),
    ]
}
pub fn clear_proxy(runner: &dyn AdbRunner, serial: &str) -> Result<(), DeviceError> {
    let args = clear_proxy_command(serial);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    runner.run(&refs).map(|_| ())
}

/// Resolves the Linux UID assigned to an installed package so logcat can be scoped to
/// the app under test instead of collecting unrelated device noise.
pub fn app_uid(
    runner: &dyn AdbRunner,
    serial: &str,
    package_name: &str,
) -> Result<u32, DeviceError> {
    if let Ok(output) = runner.run(&[
        "-s",
        serial,
        "shell",
        "cmd",
        "package",
        "list",
        "packages",
        "-U",
        package_name,
    ]) && let Some(uid) = parse_package_list_uid(&output, package_name)
    {
        return Ok(uid);
    }
    let output = runner.run(&["-s", serial, "shell", "dumpsys", "package", package_name])?;
    parse_app_uid(&output)
        .ok_or_else(|| DeviceError::Adb(format!("could not determine UID for {package_name}")))
}

/// Extracts the foreground activity component from `dumpsys window` or activity output.
#[allow(clippy::expect_used)] // infallible: regex::escape output is always a valid pattern
pub fn parse_foreground_activity(output: &str, package_name: &str) -> Option<String> {
    let expression = Regex::new(&format!(r"{}\/[^\s}}]+", regex::escape(package_name)))
        .expect("valid foreground activity regex");
    expression
        .find(output)
        .map(|matched| matched.as_str().to_owned())
}

fn parse_package_list_uid(output: &str, package_name: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let (package, uid) = line.trim().strip_prefix("package:")?.split_once(" uid:")?;
        (package == package_name)
            .then(|| uid.trim().parse().ok())
            .flatten()
    })
}

/// The literal package UID regex, compiled once per process.
#[allow(clippy::expect_used)] // infallible: pattern is a source literal
fn package_uid_regex() -> &'static regex::Regex {
    static REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    REGEX.get_or_init(|| {
        regex::Regex::new(r"(?m)^\s*(?:userId|appId)\s*=\s*(\d+)\b")
            .expect("valid package UID regex")
    })
}

fn parse_app_uid(package_dump: &str) -> Option<u32> {
    package_uid_regex()
        .captures(package_dump)
        .and_then(|captures| captures.get(1))
        .and_then(|uid| uid.as_str().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    #[test]
    fn constructs_proxy_cleanup_without_shell_interpolation() {
        assert_eq!(clear_proxy_command("device").last().unwrap(), ":0");
    }

    #[test]
    fn starts_companion_capture_through_a_serial_scoped_reverse_tunnel() {
        use std::sync::Mutex;

        struct RecordingRunner(Mutex<Vec<Vec<String>>>);
        impl AdbRunner for RecordingRunner {
            fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
                self.0
                    .lock()
                    .unwrap()
                    .push(args.iter().map(|arg| (*arg).into()).collect());
                Ok("Starting: Intent".into())
            }
            fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
                unreachable!("USB companion startup does not transfer a file")
            }
        }

        let runner = RecordingRunner(Mutex::new(Vec::new()));
        start_usb_companion_capture(
            &runner,
            "usb-serial",
            49560,
            "secure-token",
            "com.example.app",
        )
        .unwrap();
        assert_eq!(
            *runner.0.lock().unwrap(),
            vec![
                vec!["-s", "usb-serial", "reverse", "tcp:49560", "tcp:49560"],
                vec![
                    "-s",
                    "usb-serial",
                    "shell",
                    "am",
                    "start",
                    "-n",
                    "dev.prayag.apptester.companion/.MainActivity",
                    "--es",
                    "app_tester_host",
                    "127.0.0.1",
                    "--ei",
                    "app_tester_port",
                    "49560",
                    "--es",
                    "app_tester_token",
                    "secure-token",
                    "--es",
                    "app_tester_package",
                    "com.example.app",
                ],
            ]
        );
    }

    #[test]
    fn stops_companion_vpn_through_a_serial_scoped_intent() {
        use std::sync::Mutex;

        struct RecordingRunner(Mutex<Vec<Vec<String>>>);
        impl AdbRunner for RecordingRunner {
            fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
                self.0
                    .lock()
                    .unwrap()
                    .push(args.iter().map(|arg| (*arg).into()).collect());
                Ok("Starting: Intent".into())
            }
            fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
                unreachable!("stopping the companion does not transfer a file")
            }
        }

        let runner = RecordingRunner(Mutex::new(Vec::new()));
        stop_usb_companion_capture(&runner, "usb-serial").unwrap();
        assert_eq!(
            *runner.0.lock().unwrap(),
            vec![vec![
                "-s",
                "usb-serial",
                "shell",
                "am",
                "start",
                "-n",
                "dev.prayag.apptester.companion/.MainActivity",
                "--ez",
                "app_tester_stop_vpn",
                "true",
            ]]
        );
    }

    #[test]
    fn parses_foreground_activity_for_target_package() {
        assert_eq!(
            parse_foreground_activity(
                "mCurrentFocus=Window{abc u0 com.example/.CheckoutActivity}",
                "com.example"
            )
            .as_deref(),
            Some("com.example/.CheckoutActivity")
        );
    }

    #[test]
    fn targets_certificate_transfer_to_the_selected_serial() {
        use std::sync::Mutex;
        struct RecordingRunner {
            pushed_serial: Mutex<Option<String>>,
        }
        impl AdbRunner for RecordingRunner {
            fn run(&self, _: &[&str]) -> Result<String, DeviceError> {
                Ok("started".into())
            }
            fn push(&self, serial: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
                *self.pushed_serial.lock().unwrap() = Some(serial.into());
                Ok("pushed".into())
            }
        }
        let certificate = std::env::temp_dir().join(format!("app-tester-{}.pem", Uuid::new_v4()));
        std::fs::write(&certificate, "test certificate").unwrap();
        let runner = RecordingRunner {
            pushed_serial: Mutex::new(None),
        };
        prepare_certificate_install(&runner, "usb-serial", &certificate).unwrap();
        assert_eq!(
            runner.pushed_serial.lock().unwrap().as_deref(),
            Some("usb-serial")
        );
        std::fs::remove_file(certificate).unwrap();
    }

    #[test]
    fn extracts_app_uid_from_package_dump() {
        let output = "Packages:\n  Package [dev.example.app]:\n    userId=10129\n";
        assert_eq!(parse_app_uid(output), Some(10129));
    }

    #[test]
    fn extracts_app_uid_when_android_appends_package_fields() {
        let output =
            "Packages:\n  Package [com.yajtech.eynorixdev]:\n    userId=10231 gids=[3003]\n";
        assert_eq!(parse_app_uid(output), Some(10231));
    }

    #[test]
    fn extracts_uid_from_modern_package_manager_output() {
        let output = "package:com.yajtech.eynorixdev uid:10228\n";
        assert_eq!(
            parse_package_list_uid(output, "com.yajtech.eynorixdev"),
            Some(10228)
        );
        assert_eq!(parse_app_uid("    appId=10228\n"), Some(10228));
    }
}

#[cfg(test)]
mod proxy_management_tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingRunner {
        commands: Mutex<Vec<Vec<String>>>,
        response: String,
    }
    impl AdbRunner for RecordingRunner {
        fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
            self.commands
                .lock()
                .unwrap()
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            Ok(self.response.clone())
        }
        fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
            unreachable!("proxy management does not transfer files")
        }
    }

    #[test]
    fn clear_proxy_disables_the_global_setting() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            response: String::new(),
        };
        clear_proxy(&runner, "usb-serial").unwrap();
        assert_eq!(
            *runner.commands.lock().unwrap(),
            vec![vec![
                "-s",
                "usb-serial",
                "shell",
                "settings",
                "put",
                "global",
                "http_proxy",
                ":0",
            ]]
        );
    }
}
