//! Proxy configuration and status models.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyStatus {
    Stopped,
    Starting,
    Running,
    CertificateRequired,
    DeviceNotConfigured,
    PartiallyAvailable,
    BlockedByPinning,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionApp {
    pub package_name: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfiguration {
    pub bind_address: String,
    pub port: u16,
    pub ca_certificate_path: PathBuf,
    pub ca_fingerprint_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    pub certificate_path: PathBuf,
    pub fingerprint_sha256: String,
}
