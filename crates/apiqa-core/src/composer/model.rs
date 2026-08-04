//! Composer models: what a user builds in the composer UI and what a send
//! returns. Pure data types — no logic, no I/O.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::traffic::{BodyStorage, HeaderEntry, QueryParameter, TransactionState};

/// How the request body is carried on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManualBody {
    None,
    /// `application/x-www-form-urlencoded`
    Form {
        fields: Vec<(String, String)>,
    },
    /// `multipart/form-data`; file fields stream from disk, never buffered
    Multipart {
        fields: Vec<MultipartField>,
    },
    /// Raw text with an optional media type (JSON, XML, plain text, ...)
    Raw {
        media_type: Option<String>,
        text: String,
    },
    /// Raw bytes, e.g. loaded from a file (bounded by the caller)
    Binary {
        bytes: Vec<u8>,
    },
}

/// One `multipart/form-data` field: either text (`value`) or a file path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartField {
    pub name: String,
    pub value: Option<String>,
    pub file: Option<String>,
    pub media_type: Option<String>,
}

/// How credentials are attached to a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    None,
    /// `Authorization: Bearer <token>`
    Bearer {
        token: String,
    },
    /// `Authorization: Basic base64(user:pass)`
    Basic {
        username: String,
        password: String,
    },
    /// A custom key/value placed in a header or appended to the query
    ApiKey {
        key: String,
        value: String,
        in_query: bool,
    },
}

/// A request composed in the UI, ready to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualRequest {
    pub method: String,
    /// Full URL; extra query rows are appended by the engine
    pub url: String,
    pub query: Vec<QueryParameter>,
    pub headers: Vec<HeaderEntry>,
    pub body: ManualBody,
    pub auth: AuthSpec,
}

impl Default for ManualRequest {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            url: String::new(),
            query: vec![],
            headers: vec![],
            body: ManualBody::None,
            auth: AuthSpec::None,
        }
    }
}

/// Per-send behaviour knobs. Also the key for the pooled client cache, so it
/// must be fully value-comparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendOptions {
    pub timeout_ms: u64,
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub verify_tls: bool,
    /// Route through a proxy (e.g. the capture proxy); `None` = direct
    pub proxy_url: Option<String>,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            follow_redirects: true,
            max_redirects: 5,
            verify_tls: true,
            proxy_url: None,
        }
    }
}

/// What the composer UI needs to render a completed send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    pub transaction_id: Uuid,
    pub state: TransactionState,
    pub status: u16,
    pub reason: Option<String>,
    pub elapsed_ms: u64,
    pub total_bytes: u64,
    /// Redacted preview of the response body
    pub body: BodyStorage,
    pub content_type: Option<String>,
    pub headers: Vec<HeaderEntry>,
    pub http_version: String,
}

/// User-facing send failures. Every variant renders directly in the UI.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("unsupported method: {0}")]
    InvalidMethod(String),
    #[error("request failed: {0}")]
    Request(String),
    #[error("could not read multipart file {path}: {source}")]
    MultipartFile {
        path: String,
        source: std::io::Error,
    },
    #[error("could not store transaction: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_body_kind_round_trips_through_json() {
        let bodies = [
            ManualBody::None,
            ManualBody::Form {
                fields: vec![("a".into(), "1".into())],
            },
            ManualBody::Multipart {
                fields: vec![MultipartField {
                    name: "file".into(),
                    value: None,
                    file: Some("/tmp/x.bin".into()),
                    media_type: Some("application/octet-stream".into()),
                }],
            },
            ManualBody::Raw {
                media_type: Some("application/json".into()),
                text: "{\"a\":1}".into(),
            },
            ManualBody::Binary {
                bytes: vec![1, 2, 3],
            },
        ];
        for body in bodies {
            let encoded = serde_json::to_value(&body).unwrap();
            assert_eq!(serde_json::from_value::<ManualBody>(encoded).unwrap(), body);
        }
    }

    #[test]
    fn every_auth_kind_round_trips_through_json() {
        for auth in [
            AuthSpec::None,
            AuthSpec::Bearer { token: "t".into() },
            AuthSpec::Basic {
                username: "u".into(),
                password: "p".into(),
            },
            AuthSpec::ApiKey {
                key: "x-key".into(),
                value: "v".into(),
                in_query: true,
            },
        ] {
            let encoded = serde_json::to_value(&auth).unwrap();
            assert_eq!(serde_json::from_value::<AuthSpec>(encoded).unwrap(), auth);
        }
    }

    #[test]
    fn defaults_are_safe_for_public_networks() {
        let options = SendOptions::default();
        assert_eq!(options.timeout_ms, 30_000);
        assert!(options.follow_redirects);
        assert!(options.verify_tls);
        assert_eq!(options.proxy_url, None);
    }

    #[test]
    fn default_request_is_a_plain_get() {
        let request = ManualRequest::default();
        assert_eq!(request.method, "GET");
        assert_eq!(request.body, ManualBody::None);
        assert_eq!(request.auth, AuthSpec::None);
    }
}
