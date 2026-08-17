//! Android device discovery and process-based ADB execution.
//!
//! All interaction with ADB is kept behind [`AdbRunner`] so parsing and failure
//! behaviour can be tested without a connected phone.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationStatus {
    Authorized,
    Unauthorized,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndroidDevice {
    pub serial: String,
    pub authorization_status: AuthorizationStatus,
    pub model: Option<String>,
    pub android_version: Option<String>,
    pub api_level: Option<u32>,
    pub resolution: Option<String>,
    pub density: Option<u32>,
    pub architecture: Option<String>,
    pub product: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndroidApp {
    pub package_name: String,
    pub version_name: Option<String>,
    pub version_code: Option<u64>,
    pub debuggable: bool,
}

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error(
        "Android Platform Tools were not found. Install them or set ANDROID_HOME/ANDROID_SDK_ROOT."
    )]
    AdbNotFound,
    #[error("failed to start ADB at {path}: {message}")]
    Start { path: PathBuf, message: String },
    #[error("ADB failed: {0}")]
    Adb(String),
    #[error("ADB returned non-UTF-8 output")]
    InvalidOutput,
}

pub trait AdbRunner: Send + Sync {
    fn run(&self, args: &[&str]) -> Result<String, DeviceError>;
    fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<String, DeviceError>;
}

#[derive(Debug, Clone)]
pub struct ProcessAdb {
    path: PathBuf,
}

impl ProcessAdb {
    pub fn discover() -> Result<Self, DeviceError> {
        discover_adb_path()
            .map(|path| Self { path })
            .ok_or(DeviceError::AdbNotFound)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AdbRunner for ProcessAdb {
    fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
        let output = Command::new(&self.path)
            .args(args)
            .output()
            .map_err(|error| DeviceError::Start {
                path: self.path.clone(),
                message: error.to_string(),
            })?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(DeviceError::Adb(if message.is_empty() {
                format!("command exited with {}", output.status)
            } else {
                message
            }));
        }
        String::from_utf8(output.stdout).map_err(|_| DeviceError::InvalidOutput)
    }

    fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<String, DeviceError> {
        let output = Command::new(&self.path)
            .args(["-s", serial, "push"])
            .arg(local)
            .arg(remote)
            .output()
            .map_err(|error| DeviceError::Start {
                path: self.path.clone(),
                message: error.to_string(),
            })?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(DeviceError::Adb(if message.is_empty() {
                format!("command exited with {}", output.status)
            } else {
                message
            }));
        }
        String::from_utf8(output.stdout).map_err(|_| DeviceError::InvalidOutput)
    }
}

pub fn discover_adb_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("APP_TESTER_ADB") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let executable = if cfg!(windows) { "adb.exe" } else { "adb" };
    for variable in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = std::env::var_os(variable) {
            let candidate = PathBuf::from(root).join("platform-tools").join(executable);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    if let Some(home) = std::env::var_os("HOME") {
        let root = PathBuf::from(home);
        for candidate in [
            root.join("Library/Android/sdk/platform-tools")
                .join(executable),
            root.join("Android/Sdk/platform-tools").join(executable),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(executable))
            .find(|candidate| candidate.is_file())
    })
}

pub fn list_devices(runner: &dyn AdbRunner) -> Result<Vec<AndroidDevice>, DeviceError> {
    let output = runner.run(&["devices", "-l"])?;
    // `adb devices -l` already includes the model needed by the picker.  Do
    // not run six extra shell commands per device here: this command is polled
    // frequently so that a USB connection appears immediately in the UI.
    Ok(parse_device_list(&output))
}

pub fn list_third_party_apps(
    runner: &dyn AdbRunner,
    serial: &str,
) -> Result<Vec<AndroidApp>, DeviceError> {
    let output = runner.run(&["-s", serial, "shell", "pm", "list", "packages", "-3"])?;
    let package_names = parse_package_list(&output);
    if package_names.is_empty() {
        return Ok(Vec::new());
    }

    // One package-manager dump is vastly faster than spawning a new `dumpsys`
    // process for every third-party app on the phone.
    let package_dump = runner
        .run(&["-s", serial, "shell", "dumpsys", "package"])
        .unwrap_or_default();
    let mut apps = package_names
        .into_iter()
        .filter_map(|package_name| {
            package_details(&package_dump, &package_name).map(|details| {
                let debuggable = package_is_debuggable(details);
                let (version_name, version_code) = parse_package_version(details);
                AndroidApp {
                    package_name,
                    version_name,
                    version_code,
                    debuggable,
                }
            })
        })
        .collect::<Vec<_>>();
    // Some OEM package managers omit or truncate flag data in their complete
    // dump. Keep the package picker usable in that case: selecting a package
    // is still validated by Android before capture begins.
    if apps.is_empty() {
        apps = parse_package_list(&output)
            .into_iter()
            .map(|package_name| AndroidApp {
                package_name,
                version_name: None,
                version_code: None,
                debuggable: false,
            })
            .collect();
    }
    apps.sort_by(|left, right| left.package_name.cmp(&right.package_name));
    Ok(apps)
}

fn package_details<'a>(dump: &'a str, package_name: &str) -> Option<&'a str> {
    let marker = format!("Package [{}]", package_name);
    let start = dump.find(&marker)?;
    let details = &dump[start..];
    let end = details[marker.len()..]
        .find("\n  Package [")
        .map(|offset| marker.len() + offset)
        .unwrap_or(details.len());
    Some(&details[..end])
}

fn package_is_debuggable(package_dump: &str) -> bool {
    package_dump.lines().any(|line| {
        let normalized = line.trim().to_ascii_uppercase();
        normalized == "DEBUGGABLE=TRUE"
            || normalized
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .any(|token| token == "DEBUGGABLE")
    })
}

pub fn launch_app(
    runner: &dyn AdbRunner,
    serial: &str,
    package_name: &str,
) -> Result<(), DeviceError> {
    validate_package_name(package_name)?;
    let activities = runner.run(&[
        "-s",
        serial,
        "shell",
        "cmd",
        "package",
        "query-activities",
        "--brief",
        "-a",
        "android.intent.action.MAIN",
        "-c",
        "android.intent.category.LAUNCHER",
        package_name,
    ])?;
    let component = parse_launcher_activity(&activities, package_name).ok_or_else(|| {
        DeviceError::Adb(format!(
            "{package_name} does not expose a launchable activity"
        ))
    })?;
    let output = runner.run(&["-s", serial, "shell", "am", "start", "-W", "-n", &component])?;
    if output.lines().any(|line| line.trim().starts_with("Error:")) {
        return Err(DeviceError::Adb(
            output
                .lines()
                .find(|line| line.trim().starts_with("Error:"))
                .unwrap_or("failed to launch application")
                .trim()
                .to_owned(),
        ));
    }
    Ok(())
}

pub fn parse_launcher_activity(output: &str, package_name: &str) -> Option<String> {
    let mut candidates = output
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(package_name) && line.contains('/'))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|component| {
        let lowercase = component.to_ascii_lowercase();
        lowercase.contains("leakcanary")
            || lowercase.contains("debug")
            || lowercase.contains("test")
    });
    candidates.first().map(|component| (*component).to_owned())
}

fn validate_package_name(package_name: &str) -> Result<(), DeviceError> {
    let valid = !package_name.is_empty()
        && package_name.contains('.')
        && package_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(DeviceError::Adb("invalid Android package name".to_owned()))
    }
}

pub fn parse_package_list(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .map(str::trim)
        .filter(|package| !package.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn parse_package_version(output: &str) -> (Option<String>, Option<u64>) {
    let version_name = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("versionName="))
        .map(str::trim)
        .filter(|version| !version.is_empty() && *version != "null")
        .map(str::to_owned);
    let version_code = output.lines().find_map(|line| {
        let value = line.trim().strip_prefix("versionCode=")?;
        value.split_whitespace().next()?.parse().ok()
    });
    (version_name, version_code)
}

pub fn parse_device_list(output: &str) -> Vec<AndroidDevice> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("List of devices") && !line.starts_with('*')
        })
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?.to_owned();
            // App Tester supports physical USB transports only. ADB exposes
            // emulators as `emulator-*` and network transports as host:port.
            if serial.starts_with("emulator-") || serial.contains(':') {
                return None;
            }
            let state = fields.next().unwrap_or("unknown");
            let metadata = fields
                .filter_map(|field| field.split_once(':'))
                .collect::<std::collections::HashMap<_, _>>();
            Some(AndroidDevice {
                authorization_status: match state {
                    "device" => AuthorizationStatus::Authorized,
                    "unauthorized" => AuthorizationStatus::Unauthorized,
                    "offline" => AuthorizationStatus::Offline,
                    _ => AuthorizationStatus::Unknown,
                },
                model: metadata.get("model").map(|value| value.replace('_', " ")),
                product: metadata.get("product").map(|value| (*value).to_owned()),
                serial,
                android_version: None,
                api_level: None,
                resolution: None,
                density: None,
                architecture: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted runner so list_installed_apps can be tested without adb.
    struct FakeRunner {
        responses: std::collections::HashMap<String, String>,
    }

    impl AdbRunner for FakeRunner {
        fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
            let key = args.join(" ");
            self.responses
                .get(&key)
                .cloned()
                .ok_or(DeviceError::AdbNotFound)
        }

        fn push(&self, _serial: &str, _local: &Path, _remote: &str) -> Result<String, DeviceError> {
            Ok(String::new())
        }
    }

    #[test]
    fn lists_both_debug_and_release_apps_with_flags() {
        let runner = FakeRunner {
            responses: [
                (
                    "-s serial shell pm list packages -3".to_owned(),
                    "package:com.example.debug\npackage:com.example.release\n".to_owned(),
                ),
                (
                    "-s serial shell dumpsys package".to_owned(),
                    "  Package [com.example.debug] (111):\n\
                     \x20   versionCode=1 minSdk=24\n\
                     \x20   versionName=1.0\n\
                     \x20   flags=[ HAS_CODE DEBUGGABLE ]\n\
                       Package [com.example.release] (222):\n\
                     \x20   versionCode=2 minSdk=24\n\
                     \x20   versionName=2.0\n\
                     \x20   flags=[ HAS_CODE ]\n"
                        .to_owned(),
                ),
            ]
            .into(),
        };

        let apps = list_third_party_apps(&runner, "serial").unwrap();
        assert_eq!(apps.len(), 2);
        let debug = apps
            .iter()
            .find(|app| app.package_name == "com.example.debug")
            .unwrap();
        let release = apps
            .iter()
            .find(|app| app.package_name == "com.example.release")
            .unwrap();
        assert!(debug.debuggable);
        assert_eq!(debug.version_name.as_deref(), Some("1.0"));
        assert!(
            !release.debuggable,
            "release builds must appear in the picker"
        );
        assert_eq!(release.version_name.as_deref(), Some("2.0"));
    }

    #[test]
    fn falls_back_to_names_when_the_package_dump_is_missing() {
        let runner = FakeRunner {
            responses: [(
                "-s serial shell pm list packages -3".to_owned(),
                "package:com.example.release\n".to_owned(),
            )]
            .into(),
        };

        let apps = list_third_party_apps(&runner, "serial").unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].package_name, "com.example.release");
        assert!(!apps[0].debuggable);
    }

    #[test]
    fn lists_only_physical_usb_devices() {
        let output = "List of devices attached\n\
emulator-5554 device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 transport_id:1\n\
192.168.1.7:37099 device product:oriole model:Pixel_6 transport_id:2\n\
R58M123 unauthorized usb:1-1 product:x model:Galaxy_S22\n\
deadbeef offline\n";
        let devices = parse_device_list(output);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "R58M123");
        assert_eq!(devices[0].model.as_deref(), Some("Galaxy S22"));
        assert_eq!(
            devices[0].authorization_status,
            AuthorizationStatus::Unauthorized
        );
        assert_eq!(
            devices[1].authorization_status,
            AuthorizationStatus::Offline
        );
    }

    #[test]
    fn ignores_daemon_noise_and_headers() {
        let output = "* daemon started successfully\nList of devices attached\n\n";
        assert!(parse_device_list(output).is_empty());
    }

    #[test]
    fn parses_package_names_and_versions() {
        assert_eq!(
            parse_package_list("package:com.example.beta\npackage:com.example.alpha\n"),
            ["com.example.beta", "com.example.alpha"]
        );
        assert_eq!(
            parse_package_version(
                "Packages:\n  versionCode=42 minSdk=24 targetSdk=35\n  versionName=2.4.1\n"
            ),
            (Some("2.4.1".to_owned()), Some(42))
        );
        assert!(package_is_debuggable(
            "flags=[ HAS_CODE ALLOW_CLEAR_USER_DATA DEBUGGABLE ]"
        ));
        assert!(package_is_debuggable("debuggable=true"));
        assert!(!package_is_debuggable(
            "flags=[ HAS_CODE ALLOW_CLEAR_USER_DATA ]"
        ));

        let dump = "  Package [com.example.app] (123):\n    versionCode=42 minSdk=24\n    versionName=2.4.1\n    flags=[ HAS_CODE DEBUGGABLE ]\n  Package [com.example.other] (456):\n";
        assert!(package_is_debuggable(
            package_details(dump, "com.example.app").unwrap()
        ));
        assert!(package_details(dump, "missing").is_none());
    }

    #[test]
    fn rejects_unsafe_package_names_before_launch() {
        assert!(validate_package_name("com.example.app").is_ok());
        assert!(validate_package_name("com.example;reboot").is_err());
        assert!(validate_package_name("").is_err());
    }

    #[test]
    fn prefers_product_launcher_over_debug_tooling_activity() {
        let output = "2 activities found:\n\
  Activity #0:\n\
    com.example.app/com.example.MainActivity\n\
  Activity #1:\n\
    com.example.app/leakcanary.internal.activity.LeakLauncherActivity\n";
        assert_eq!(
            parse_launcher_activity(output, "com.example.app").as_deref(),
            Some("com.example.app/com.example.MainActivity")
        );
    }
}
