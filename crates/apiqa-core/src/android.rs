use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionType {
    Usb,
    Wireless,
    Emulator,
}

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
    pub connection_type: ConnectionType,
    pub authorization_status: AuthorizationStatus,
    pub model: Option<String>,
    pub android_version: Option<String>,
    pub api_level: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndroidApp {
    pub package_name: String,
    pub version_name: Option<String>,
    pub debuggable: bool,
}

#[derive(Debug, Clone)]
pub struct Adb {
    path: PathBuf,
}

impl Adb {
    pub fn discover() -> CoreResult<Self> {
        discover_adb_path().map(|path| Self { path }).ok_or_else(|| CoreError::Android("Android Platform Tools not found; install them or set ANDROID_HOME/ANDROID_SDK_ROOT".into()))
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn run(&self, args: &[&str]) -> CoreResult<String> {
        let output = Command::new(&self.path).args(args).output()?;
        if !output.status.success() {
            return Err(CoreError::Android(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|_| CoreError::Android("ADB returned non-UTF-8 output".into()))
    }
    pub fn devices(&self) -> CoreResult<Vec<AndroidDevice>> {
        let mut devices = parse_devices(&self.run(&["devices", "-l"])?);
        for device in devices
            .iter_mut()
            .filter(|d| d.authorization_status == AuthorizationStatus::Authorized)
        {
            device.android_version = self.property(&device.serial, "ro.build.version.release");
            device.api_level = self
                .property(&device.serial, "ro.build.version.sdk")
                .and_then(|v| v.parse().ok());
        }
        Ok(devices)
    }
    pub fn debuggable_apps(&self, serial: &str) -> CoreResult<Vec<AndroidApp>> {
        let mut apps = Vec::new();
        for package in
            parse_packages(&self.run(&["-s", serial, "shell", "pm", "list", "packages", "-3"])?)
        {
            let dump = self
                .run(&["-s", serial, "shell", "dumpsys", "package", &package])
                .unwrap_or_default();
            if is_debuggable(&dump) {
                apps.push(AndroidApp {
                    version_name: parse_version(&dump),
                    package_name: package,
                    debuggable: true,
                });
            }
        }
        apps.sort_by(|a, b| a.package_name.cmp(&b.package_name));
        Ok(apps)
    }
    pub fn configure_proxy(&self, serial: &str, host: &str, port: u16) -> CoreResult<()> {
        validate_host(host)?;
        self.run(&[
            "-s",
            serial,
            "shell",
            "settings",
            "put",
            "global",
            "http_proxy",
            &format!("{host}:{port}"),
        ])
        .map(|_| ())
    }
    pub fn clear_proxy(&self, serial: &str) -> CoreResult<()> {
        self.run(&[
            "-s",
            serial,
            "shell",
            "settings",
            "put",
            "global",
            "http_proxy",
            ":0",
        ])
        .map(|_| ())
    }
    pub fn wifi_ip(&self, serial: &str) -> CoreResult<IpAddr> {
        parse_wifi_ip(&self.run(&["-s", serial, "shell", "ip", "route"])?).ok_or_else(|| {
            CoreError::Android("could not determine selected device Wi-Fi IP".into())
        })
    }
    pub fn enable_usb_wifi(&self, serial: &str) -> CoreResult<String> {
        // This is intentionally a two-step ADB operation: TCP mode can only be
        // enabled while the USB transport is still available, then the known
        // Wi-Fi address is connected explicitly without a shell.
        self.run(&["-s", serial, "tcpip", "5555"])?;
        let endpoint = format!("{}:5555", self.wifi_ip(serial)?);
        self.run(&["connect", &endpoint])?;
        Ok(endpoint)
    }
    pub fn prepare_certificate_install(&self, serial: &str, certificate: &Path) -> CoreResult<()> {
        if !certificate.is_file() {
            return Err(CoreError::Android(
                "local CA certificate was not found".into(),
            ));
        }
        let output = Command::new(&self.path)
            .args(["-s", serial, "push"])
            .arg(certificate)
            .arg("/sdcard/Download/APIQA-HTTPS-CA.pem")
            .output()?;
        if !output.status.success() {
            return Err(CoreError::Android(
                String::from_utf8_lossy(&output.stderr).trim().into(),
            ));
        }
        self.run(&[
            "-s",
            serial,
            "shell",
            "am",
            "start",
            "-a",
            "android.credentials.INSTALL",
        ])
        .map(|_| ())
    }
    pub fn app_uid(&self, serial: &str, package: &str) -> CoreResult<u32> {
        validate_package(package)?;
        let output = self.run(&[
            "-s", serial, "shell", "cmd", "package", "list", "packages", "-U", package,
        ])?;
        output
            .lines()
            .find_map(|line| {
                line.split_once(" uid:").and_then(|(name, uid)| {
                    (name.trim_start_matches("package:") == package)
                        .then(|| uid.trim().parse().ok())
                        .flatten()
                })
            })
            .ok_or_else(|| CoreError::Android(format!("could not determine UID for {package}")))
    }
    fn property(&self, serial: &str, name: &str) -> Option<String> {
        self.run(&["-s", serial, "shell", "getprop", name])
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    }
}

fn discover_adb_path() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    std::env::var_os("APIQA_ADB")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| {
            ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
                .into_iter()
                .filter_map(std::env::var_os)
                .map(PathBuf::from)
                .map(|p| p.join("platform-tools").join(exe))
                .find(|p| p.is_file())
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join("Library/Android/sdk/platform-tools").join(exe))
                .filter(|p| p.is_file())
        })
        .or_else(|| {
            std::env::var_os("PATH").and_then(|p| {
                std::env::split_paths(&p)
                    .map(|p| p.join(exe))
                    .find(|p| p.is_file())
            })
        })
}

pub fn parse_devices(output: &str) -> Vec<AndroidDevice> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("List of") && !line.starts_with('*'))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?.to_owned();
            let state = fields.next().unwrap_or("unknown");
            let metadata: HashMap<_, _> = fields.filter_map(|f| f.split_once(':')).collect();
            Some(AndroidDevice {
                connection_type: if serial.starts_with("emulator-") {
                    ConnectionType::Emulator
                } else if serial.contains(':') {
                    ConnectionType::Wireless
                } else {
                    ConnectionType::Usb
                },
                authorization_status: match state {
                    "device" => AuthorizationStatus::Authorized,
                    "unauthorized" => AuthorizationStatus::Unauthorized,
                    "offline" => AuthorizationStatus::Offline,
                    _ => AuthorizationStatus::Unknown,
                },
                model: metadata.get("model").map(|v| v.replace('_', " ")),
                serial,
                android_version: None,
                api_level: None,
            })
        })
        .collect()
}
fn parse_packages(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .map(str::to_owned)
        .collect()
}
pub fn parse_wifi_ip(output: &str) -> Option<IpAddr> {
    output.lines().find_map(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let source = fields
            .windows(2)
            .find_map(|pair| (pair[0] == "src").then_some(pair[1]))?;
        let ip = source.parse::<IpAddr>().ok()?;
        let is_wifi_route = fields.first().is_some_and(|field| field.contains('/'))
            && fields
                .windows(2)
                .any(|pair| pair[0] == "dev" && pair[1].starts_with("wlan"));
        (is_wifi_route && !ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
    })
}
fn parse_version(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|l| l.trim().strip_prefix("versionName="))
        .filter(|v| *v != "null")
        .map(str::to_owned)
}
fn is_debuggable(output: &str) -> bool {
    output.lines().any(|l| {
        l.to_ascii_uppercase()
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|v| v == "DEBUGGABLE")
    })
}
fn validate_package(value: &str) -> CoreResult<()> {
    if value.contains('.')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_'))
    {
        Ok(())
    } else {
        Err(CoreError::InvalidInput(
            "invalid Android package name".into(),
        ))
    }
}
fn validate_host(value: &str) -> CoreResult<()> {
    if value.parse::<std::net::IpAddr>().is_ok() || (value == "localhost") {
        Ok(())
    } else {
        Err(CoreError::InvalidInput(
            "proxy host must be an IP address or localhost".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_authorization_and_connection() {
        let d = parse_devices(
            "List of devices attached\nemulator-5554 device model:Pixel_8\nR58 unauthorized\n",
        );
        assert_eq!(d[0].connection_type, ConnectionType::Emulator);
        assert_eq!(d[1].authorization_status, AuthorizationStatus::Unauthorized);
    }
    #[test]
    fn rejects_interpolated_values() {
        assert!(validate_host("127.0.0.1;reboot").is_err());
        assert!(validate_package("com.app;reboot").is_err());
    }
    #[test]
    fn parses_only_wifi_route_source_ip() {
        let output = "default via 192.168.1.1 dev wlan0 proto dhcp src 192.168.1.99 metric 303\n192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.42\n10.0.0.0/8 dev rmnet0 proto kernel scope link src 10.2.3.4\n";
        assert_eq!(parse_wifi_ip(output), Some("192.168.1.42".parse().unwrap()));
        assert_eq!(parse_wifi_ip("default dev wlan0 src 192.168.1.42\n"), None);
        assert_eq!(
            parse_wifi_ip("192.168.1.0/24 dev wlan0 src not-an-ip\n"),
            None
        );
    }
}
