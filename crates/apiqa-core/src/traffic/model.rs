//! HTTP traffic models shared by the proxy, replay, and persistence layers.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    RequestStarted,
    RequestComplete,
    ResponseStarted,
    ResponseComplete,
    Failed,
    Cancelled,
    WebSocket,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "storage")]
pub enum BodyStorage {
    Empty,
    Inline {
        bytes: Vec<u8>,
    },
    Artifact {
        artifact_id: Uuid,
        preview: Vec<u8>,
        original_size: u64,
    },
    Truncated {
        preview: Vec<u8>,
        original_size: Option<u64>,
    },
    Unavailable {
        reason: String,
    },
}

impl BodyStorage {
    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Inline { bytes } => Some(bytes),
            Self::Artifact { preview, .. } | Self::Truncated { preview, .. } => Some(preview),
            Self::Empty => Some(&[]),
            Self::Unavailable { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryParameter {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedRequest {
    pub method: String,
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Vec<QueryParameter>,
    pub headers: Vec<HeaderEntry>,
    pub body: BodyStorage,
    pub content_type: Option<String>,
    pub http_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedResponse {
    pub status: u16,
    pub reason: Option<String>,
    pub headers: Vec<HeaderEntry>,
    pub body: BodyStorage,
    pub content_type: Option<String>,
    pub decoded_size: u64,
    pub encoded_size: u64,
    pub http_version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionTiming {
    pub request_started_ms: i64,
    pub request_complete_ms: Option<i64>,
    pub response_started_ms: Option<i64>,
    pub response_complete_ms: Option<i64>,
}

impl TransactionTiming {
    pub fn duration_ms(&self) -> Option<i64> {
        self.response_complete_ms
            .map(|end| end - self.request_started_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointIdentity {
    pub method: String,
    pub host: String,
    pub path_template: String,
    pub content_type: Option<String>,
    pub request_shape: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCurl {
    pub compact: String,
    pub multiline: String,
    pub redacted: bool,
}

/// Compact evidence that an endpoint changed during the snapshot's UTC day.
/// Individual duplicate responses are replaced; this summary survives them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyChangeSummary {
    pub count: u32,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_changed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureQuality {
    Complete,
    PreviewOnly,
    MetadataOnly,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpTransaction {
    pub id: Uuid,
    pub session_id: Uuid,
    pub connection_id: Uuid,
    pub request: CapturedRequest,
    pub response: Option<CapturedResponse>,
    pub state: TransactionState,
    pub timing: TransactionTiming,
    pub endpoint_identity: Option<EndpointIdentity>,
    pub curl: Option<GeneratedCurl>,
    pub capture_quality: CaptureQuality,
    pub comparison: Option<crate::comparison::ResponseComparison>,
    pub correlated_incidents: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_changes: Option<DailyChangeSummary>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_storage_exposes_bytes_only_when_available() {
        assert_eq!(BodyStorage::Empty.bytes(), Some(b"".as_slice()));
        assert_eq!(
            BodyStorage::Inline { bytes: vec![1, 2] }.bytes(),
            Some(&[1u8, 2u8][..])
        );
        assert_eq!(
            BodyStorage::Artifact {
                artifact_id: Uuid::new_v4(),
                preview: vec![3, 4],
                original_size: 2
            }
            .bytes(),
            Some(&[3u8, 4u8][..])
        );
        assert_eq!(
            BodyStorage::Truncated {
                preview: vec![5, 6],
                original_size: None
            }
            .bytes(),
            Some(&[5u8, 6u8][..])
        );
        assert_eq!(
            BodyStorage::Unavailable {
                reason: "encrypted".into()
            }
            .bytes(),
            None
        );
    }

    #[test]
    fn timing_reports_duration_only_after_completion() {
        let timing = TransactionTiming {
            request_started_ms: 100,
            ..Default::default()
        };
        assert_eq!(timing.duration_ms(), None);
        let timing = TransactionTiming {
            request_started_ms: 100,
            response_complete_ms: Some(430),
            ..Default::default()
        };
        assert_eq!(timing.duration_ms(), Some(330));
    }

    #[test]
    fn transaction_round_trips_through_json_with_every_body_kind() {
        let json = serde_json::json!({
            "id": Uuid::new_v4(), "session_id": Uuid::new_v4(), "connection_id": Uuid::new_v4(),
            "request": {"method":"POST","scheme":"https","host":"api.test","port":8443,"path":"/v1/items","query":[{"name":"a","value":"1"}],"headers":[{"name":"x-token","value":"abc"}],"body":{"storage":"inline","bytes":[104,105]},"content_type":"application/json","http_version":"HTTP_1_1"},
            "response": {"status":201,"reason":"Created","headers":[],"body":{"storage":"truncated","preview":[1,2,3],"original_size":999},"content_type":"application/json","decoded_size":999,"encoded_size":1001,"http_version":"HTTP_1_1"},
            "state": "response_complete", "timing": {"request_started_ms":1,"request_complete_ms":2,"response_started_ms":3,"response_complete_ms":4},
            "endpoint_identity": {"method":"POST","host":"api.test","path_template":"/v1/{id}","content_type":"application/json","request_shape":"abc"},
            "curl": {"compact":"curl x","multiline":"curl \\\n  x","redacted":true},
            "capture_quality": "complete", "correlated_incidents": [], "comparison": null,
            "created_at": "2026-07-24T00:00:00Z", "updated_at": "2026-07-24T00:00:00Z"
        });
        let transaction: HttpTransaction = serde_json::from_value(json.clone()).unwrap();
        let encoded = serde_json::to_value(&transaction).unwrap();
        assert_eq!(encoded, json);
        assert_eq!(transaction.timing.duration_ms(), Some(3));
        assert_eq!(
            transaction.response.unwrap().body.bytes(),
            Some(&[1u8, 2u8, 3u8][..])
        );
    }

    #[test]
    fn body_storage_serde_tag_round_trips() {
        for value in [
            serde_json::json!({"storage": "empty"}),
            serde_json::json!({"storage": "inline", "bytes": [9]}),
            serde_json::json!({"storage": "unavailable", "reason": "encrypted"}),
        ] {
            let decoded: BodyStorage = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
    }
}
