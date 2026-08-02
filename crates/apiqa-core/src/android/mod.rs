use crate::{AdbRunner, DeviceError};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

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

pub fn configure_proxy_command(serial: &str, host: &str, port: u16) -> Vec<String> {
    vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "settings".into(),
        "put".into(),
        "global".into(),
        "http_proxy".into(),
        format!("{host}:{port}"),
    ]
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
pub fn configure_proxy(
    runner: &dyn AdbRunner,
    serial: &str,
    host: &str,
    port: u16,
) -> Result<(), DeviceError> {
    let args = configure_proxy_command(serial, host, port);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    runner.run(&refs).map(|_| ())
}
pub fn clear_proxy(runner: &dyn AdbRunner, serial: &str) -> Result<(), DeviceError> {
    let args = clear_proxy_command(serial);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    runner.run(&refs).map(|_| ())
}

pub fn package_installed(
    runner: &dyn AdbRunner,
    serial: &str,
    package_name: &str,
) -> Result<bool, DeviceError> {
    validate_package_name(package_name)?;
    let output = runner.run(&["-s", serial, "shell", "pm", "path", package_name])?;
    Ok(output
        .lines()
        .any(|line| line.trim().starts_with("package:")))
}

pub fn package_version_code(
    runner: &dyn AdbRunner,
    serial: &str,
    package_name: &str,
) -> Result<Option<u64>, DeviceError> {
    validate_package_name(package_name)?;
    let output = runner.run(&["-s", serial, "shell", "dumpsys", "package", package_name])?;
    Ok(crate::parse_package_version(&output).1)
}

pub fn configure_usb_relay(
    runner: &dyn AdbRunner,
    serial: &str,
    port: u16,
) -> Result<(), DeviceError> {
    runner
        .run(&[
            "-s",
            serial,
            "reverse",
            &format!("tcp:{port}"),
            &format!("tcp:{port}"),
        ])
        .map(|_| ())
}

pub fn remove_usb_relay(
    runner: &dyn AdbRunner,
    serial: &str,
    port: u16,
) -> Result<(), DeviceError> {
    runner
        .run(&["-s", serial, "reverse", "--remove", &format!("tcp:{port}")])
        .map(|_| ())
}

pub fn launch_usb_companion(
    runner: &dyn AdbRunner,
    serial: &str,
    companion_package: &str,
    target_package: Option<&str>,
    port: u16,
    ca_pem: &str,
) -> Result<(), DeviceError> {
    validate_package_name(companion_package)?;
    if let Some(target_package) = target_package {
        validate_package_name(target_package)?;
    }
    let component = format!("{companion_package}/.UsbCaptureActivity");
    let port = port.to_string();
    let ca_base64 = STANDARD.encode(ca_pem);
    let mut args = vec![
        "-s",
        serial,
        "shell",
        "am",
        "start",
        "-W",
        "-n",
        &component,
        "--es",
        "app_tester_ca_base64",
        &ca_base64,
    ];
    if let Some(target_package) = target_package {
        args.extend([
            "--es",
            "app_tester_host",
            "127.0.0.1",
            "--ei",
            "app_tester_port",
            &port,
            "--es",
            "app_tester_package",
            target_package,
            "--ez",
            "app_tester_start_capture",
            "true",
        ]);
    }
    let output = runner.run(&args)?;
    if output.lines().any(|line| line.trim().starts_with("Error:")) {
        return Err(DeviceError::Adb(output.trim().to_owned()));
    }
    Ok(())
}

pub fn companion_vpn_active(
    runner: &dyn AdbRunner,
    serial: &str,
    companion_package: &str,
) -> Result<bool, DeviceError> {
    validate_package_name(companion_package)?;
    let output = runner.run(&["-s", serial, "shell", "dumpsys", "connectivity"])?;
    Ok(output.contains(&format!("VPN:{companion_package}")))
}

fn validate_package_name(package_name: &str) -> Result<(), DeviceError> {
    let valid = !package_name.is_empty()
        && package_name.contains('.')
        && package_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_'));
    valid
        .then_some(())
        .ok_or_else(|| DeviceError::Adb("invalid Android package name".into()))
}
pub fn verify_proxy(runner: &dyn AdbRunner, serial: &str) -> Result<String, DeviceError> {
    runner
        .run(&[
            "-s",
            serial,
            "shell",
            "settings",
            "get",
            "global",
            "http_proxy",
        ])
        .map(|value| value.trim().to_owned())
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

fn parse_app_uid(package_dump: &str) -> Option<u32> {
    let expression =
        Regex::new(r"(?m)^\s*(?:userId|appId)\s*=\s*(\d+)\b").expect("valid package UID regex");
    expression
        .captures(package_dump)
        .and_then(|captures| captures.get(1))
        .and_then(|uid| uid.as_str().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingRunner {
        commands: Mutex<Vec<Vec<String>>>,
        package_path: &'static str,
    }

    impl AdbRunner for RecordingRunner {
        fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
            self.commands
                .lock()
                .unwrap()
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            if args.get(3..5) == Some(&["pm", "path"]) {
                Ok(self.package_path.into())
            } else {
                Ok("Starting: Intent".into())
            }
        }

        fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
            unreachable!("USB relay does not transfer files")
        }
    }
    #[test]
    fn constructs_proxy_commands_without_shell_interpolation() {
        assert_eq!(
            configure_proxy_command("device", "10.0.2.2", 8080)
                .last()
                .unwrap(),
            "10.0.2.2:8080"
        );
        assert_eq!(clear_proxy_command("device").last().unwrap(), ":0");
    }

    #[test]
    fn detects_installed_companion_package() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            package_path: "package:/data/app/dev.prayag.apptester.companion/base.apk\n",
        };
        assert!(package_installed(&runner, "phone", "dev.prayag.apptester.companion").unwrap());
    }

    #[test]
    fn reads_companion_version_code() {
        struct VersionRunner;
        impl AdbRunner for VersionRunner {
            fn run(&self, _: &[&str]) -> Result<String, DeviceError> {
                Ok("versionCode=9 minSdk=26 targetSdk=36\nversionName=0.3.2".into())
            }
            fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
                unreachable!()
            }
        }
        assert_eq!(
            package_version_code(&VersionRunner, "phone", "dev.prayag.apptester.companion")
                .unwrap(),
            Some(9)
        );
    }

    #[test]
    fn detects_established_companion_vpn_from_connectivity_dump() {
        struct ConnectivityRunner;
        impl AdbRunner for ConnectivityRunner {
            fn run(&self, _: &[&str]) -> Result<String, DeviceError> {
                Ok("ni{VPN CONNECTED extra: VPN:dev.prayag.apptester.companion} Uids: <{10742-10742}>".into())
            }
            fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
                unreachable!()
            }
        }
        assert!(
            companion_vpn_active(
                &ConnectivityRunner,
                "phone",
                "dev.prayag.apptester.companion"
            )
            .unwrap()
        );
    }

    #[test]
    fn configures_and_removes_usb_reverse_relay() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            package_path: "",
        };
        configure_usb_relay(&runner, "phone", 8080).unwrap();
        remove_usb_relay(&runner, "phone", 8080).unwrap();
        assert_eq!(
            *runner.commands.lock().unwrap(),
            vec![
                vec!["-s", "phone", "reverse", "tcp:8080", "tcp:8080"],
                vec!["-s", "phone", "reverse", "--remove", "tcp:8080"],
            ]
        );
    }

    #[test]
    fn launches_companion_with_fixed_usb_endpoint() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            package_path: "",
        };
        launch_usb_companion(
            &runner,
            "phone",
            "dev.prayag.apptester.companion",
            Some("com.example.debug"),
            8080,
            "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----",
        )
        .unwrap();
        let command = runner.commands.lock().unwrap().pop().unwrap();
        assert!(
            command
                .windows(2)
                .any(|args| args == ["app_tester_host", "127.0.0.1"])
        );
        assert!(
            command
                .windows(2)
                .any(|args| args == ["app_tester_package", "com.example.debug"])
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
        use uuid::Uuid;
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
        prepare_certificate_install(&runner, "emulator-5554", &certificate).unwrap();
        assert_eq!(
            runner.pushed_serial.lock().unwrap().as_deref(),
            Some("emulator-5554")
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
