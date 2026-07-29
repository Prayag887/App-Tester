use crate::{AdbRunner, DeviceError};
use qrcode::{QrCode, render::svg};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    time::Duration,
};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct QrPairingSecret {
    pub id: Uuid,
    pub service_name: String,
    pub password: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrPairingChallenge {
    pub id: Uuid,
    pub service_name: String,
    pub qr_payload: String,
    pub qr_svg: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrPairingResult {
    pub endpoint: String,
    pub adb_output: String,
}

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

pub fn pair_with_code(
    runner: &dyn AdbRunner,
    host: &str,
    port: u16,
    pairing_code: &str,
) -> Result<QrPairingResult, DeviceError> {
    validate_host(host)?;
    if port == 0 {
        return Err(DeviceError::Adb(
            "pairing port must be between 1 and 65535".into(),
        ));
    }
    if pairing_code.len() != 6 || !pairing_code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(DeviceError::Adb(
            "pairing code must contain exactly six digits".into(),
        ));
    }
    let endpoint = format!("{host}:{port}");
    let output = runner.run(&["pair", &endpoint, pairing_code])?;
    if !output.to_ascii_lowercase().contains("successfully paired") {
        return Err(DeviceError::Adb(output.trim().to_owned()));
    }
    Ok(QrPairingResult {
        endpoint,
        adb_output: output.trim().to_owned(),
    })
}

pub fn enable_usb_wifi(
    runner: &dyn AdbRunner,
    serial: &str,
    port: u16,
) -> Result<QrPairingResult, DeviceError> {
    let endpoint = prepare_usb_wifi(runner, serial, port)?;
    connect_usb_wifi(runner, &endpoint)
}

/// Switches the selected USB device to ADB-over-TCP and returns its endpoint.
/// Callers can validate local network reachability before starting the ADB
/// connection handshake.
pub fn prepare_usb_wifi(
    runner: &dyn AdbRunner,
    serial: &str,
    port: u16,
) -> Result<String, DeviceError> {
    if port == 0 {
        return Err(DeviceError::Adb(
            "ADB Wi-Fi port must be between 1 and 65535".into(),
        ));
    }
    let routes = runner.run(&["-s", serial, "shell", "ip", "route"])?;
    let host = parse_wifi_ipv4(&routes).ok_or_else(|| {
        DeviceError::Adb(
            "could not determine the device Wi-Fi address; connect manually using its IP".into(),
        )
    })?;
    // `adb tcpip` intentionally tears down the USB transport. Resolve the device address
    // first, while the selected serial is still reachable.
    runner.run(&["-s", serial, "tcpip", &port.to_string()])?;
    Ok(format!("{host}:{port}"))
}

pub fn connect_usb_wifi(
    runner: &dyn AdbRunner,
    endpoint: &str,
) -> Result<QrPairingResult, DeviceError> {
    let output = runner.run(&["connect", &endpoint])?;
    if !output.to_ascii_lowercase().contains("connected") {
        return Err(DeviceError::Adb(output.trim().to_owned()));
    }
    Ok(QrPairingResult {
        endpoint: endpoint.to_owned(),
        adb_output: output.trim().to_owned(),
    })
}

/// Verifies that the desktop can reach the ADB TCP listener before attempting
/// the ADB handshake. Some Wi-Fi access points isolate clients even when both
/// devices receive addresses in the same subnet; without this check `adb
/// connect` can appear to hang and leave the capture handoff ambiguous.
pub fn verify_adb_wifi_endpoint(endpoint: &str, timeout: Duration) -> Result<(), DeviceError> {
    let address = endpoint
        .to_socket_addrs()
        .map_err(|_| DeviceError::Adb("invalid Wi-Fi ADB endpoint".into()))?
        .find(|address| address.is_ipv4())
        .ok_or_else(|| DeviceError::Adb("invalid Wi-Fi ADB endpoint".into()))?;
    TcpStream::connect_timeout(&address, timeout).map_err(|error| {
        DeviceError::Adb(format!(
            "the phone's Wi-Fi ADB port is unreachable ({error}). Check that both devices are on the same non-isolated Wi-Fi network; guest/client-isolated networks block USB-to-Wi-Fi capture"
        ))
    })?;
    Ok(())
}

fn validate_host(host: &str) -> Result<(), DeviceError> {
    let valid = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'));
    if valid {
        Ok(())
    } else {
        Err(DeviceError::Adb("invalid device host or IP address".into()))
    }
}

pub fn validate_companion_connection(host: &str) -> Result<(), DeviceError> {
    validate_host(host)
}

pub fn parse_wifi_ipv4(routes: &str) -> Option<String> {
    let expression =
        Regex::new(r"\bsrc ((?:\d{1,3}\.){3}\d{1,3})\b").expect("valid IP route regex");
    expression
        .captures(routes)
        .and_then(|captures| captures.get(1))
        .map(|address| address.as_str().to_owned())
}

pub fn create_qr_pairing() -> Result<(QrPairingChallenge, QrPairingSecret), DeviceError> {
    let id = Uuid::new_v4();
    let compact = id.simple().to_string();
    let service_name = format!("studio-app-tester-{}", &compact[..10]);
    let password = compact[10..26].to_owned();
    let qr_payload = format!("WIFI:T:ADB;S:{service_name};P:{password};;");
    let code = QrCode::new(qr_payload.as_bytes())
        .map_err(|error| DeviceError::Adb(format!("failed to generate pairing QR: {error}")))?;
    let qr_svg = code
        .render::<svg::Color>()
        .min_dimensions(320, 320)
        .dark_color(svg::Color("#08110f"))
        .light_color(svg::Color("#ffffff"))
        .build();
    let expires_at = OffsetDateTime::now_utc() + time::Duration::minutes(2);
    Ok((
        QrPairingChallenge {
            id,
            service_name: service_name.clone(),
            qr_payload,
            qr_svg,
            expires_at,
        },
        QrPairingSecret {
            id,
            service_name,
            password,
            expires_at,
        },
    ))
}

pub fn parse_mdns_pairing_endpoint(output: &str, service_name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        (fields.first().is_some_and(|name| *name == service_name)
            && fields
                .get(1)
                .is_some_and(|kind| *kind == "_adb-tls-pairing._tcp"))
        .then(|| fields.get(2).map(|endpoint| (*endpoint).to_owned()))
        .flatten()
    })
}

pub fn finish_qr_pairing(
    runner: &dyn AdbRunner,
    secret: &QrPairingSecret,
) -> Result<Option<QrPairingResult>, DeviceError> {
    if OffsetDateTime::now_utc() >= secret.expires_at {
        return Err(DeviceError::Adb("QR pairing request expired".into()));
    }
    let services = runner.run(&["mdns", "services"])?;
    let Some(endpoint) = parse_mdns_pairing_endpoint(&services, &secret.service_name) else {
        return Ok(None);
    };
    let output = runner.run(&["pair", &endpoint, &secret.password])?;
    if !output.to_ascii_lowercase().contains("successfully paired") {
        return Err(DeviceError::Adb(output.trim().to_owned()));
    }
    Ok(Some(QrPairingResult {
        endpoint,
        adb_output: output.trim().to_owned(),
    }))
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
    fn generates_android_adb_qr_payload() {
        let (challenge, secret) = create_qr_pairing().unwrap();
        assert!(
            challenge
                .qr_payload
                .starts_with("WIFI:T:ADB;S:studio-app-tester-")
        );
        assert!(challenge.qr_payload.ends_with(";;"));
        assert_eq!(challenge.id, secret.id);
        assert!(challenge.qr_svg.contains("<svg"));
        assert!(!challenge.qr_svg.contains(&secret.password));
    }

    #[test]
    fn validates_companion_connection_values() {
        assert!(validate_companion_connection("192.168.1.12").is_ok());
        assert!(validate_companion_connection("host/path").is_err());
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
    fn parses_only_matching_pairing_service() {
        let output = "List of discovered mdns services\n\
studio-other _adb-tls-pairing._tcp 192.168.1.2:4000\n\
studio-app-tester-123 _adb-tls-pairing._tcp 192.168.1.4:42891\n";
        assert_eq!(
            parse_mdns_pairing_endpoint(output, "studio-app-tester-123").as_deref(),
            Some("192.168.1.4:42891")
        );
    }

    #[test]
    fn parses_the_usb_device_wifi_address() {
        assert_eq!(
            parse_wifi_ipv4(
                "default via 192.168.1.1 dev wlan0 proto dhcp src 192.168.1.44 metric 600"
            ),
            Some("192.168.1.44".into())
        );
        assert_eq!(parse_wifi_ipv4("unreachable 127.0.0.0/8"), None);
    }

    #[test]
    fn resolves_the_wifi_address_before_switching_adb_off_usb() {
        use std::sync::Mutex;

        struct RecordingRunner {
            commands: Mutex<Vec<Vec<String>>>,
        }
        impl AdbRunner for RecordingRunner {
            fn run(&self, args: &[&str]) -> Result<String, DeviceError> {
                self.commands
                    .lock()
                    .unwrap()
                    .push(args.iter().map(|arg| (*arg).to_owned()).collect());
                if args.ends_with(&["shell", "ip", "route"]) {
                    Ok("10.10.10.0/24 dev wlan0 proto kernel scope link src 10.10.10.19".into())
                } else {
                    Ok("connected to 10.10.10.19:5555".into())
                }
            }
            fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
                unreachable!("USB-to-Wi-Fi does not transfer a file")
            }
        }

        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
        };
        let result = enable_usb_wifi(&runner, "JFR8T8YDFI9955MB", 5555).unwrap();

        assert_eq!(result.endpoint, "10.10.10.19:5555");
        assert_eq!(
            *runner.commands.lock().unwrap(),
            vec![
                vec!["-s", "JFR8T8YDFI9955MB", "shell", "ip", "route"],
                vec!["-s", "JFR8T8YDFI9955MB", "tcpip", "5555"],
                vec!["connect", "10.10.10.19:5555"],
            ]
        );
    }

    #[test]
    fn verifies_that_the_wifi_adb_port_is_reachable() {
        use std::net::TcpListener;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        verify_adb_wifi_endpoint(
            &listener.local_addr().unwrap().to_string(),
            Duration::from_secs(1),
        )
        .unwrap();
    }

    #[test]
    fn rejects_invalid_manual_pairing_values() {
        struct Unused;
        impl AdbRunner for Unused {
            fn run(&self, _: &[&str]) -> Result<String, DeviceError> {
                panic!("validation should run before ADB")
            }
            fn push(&self, _: &str, _: &Path, _: &str) -> Result<String, DeviceError> {
                panic!("validation should run before ADB")
            }
        }
        let runner = Unused;
        assert!(pair_with_code(&runner, "host;bad", 37123, "123456").is_err());
        assert!(pair_with_code(&runner, "192.168.1.5", 0, "123456").is_err());
        assert!(pair_with_code(&runner, "192.168.1.5", 37123, "abcdef").is_err());
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
