use crate::traffic::{BodyStorage, CaptureQuality, HttpTransaction, is_secret, redact_headers};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

pub const PORTABLE_CAPTURE_VERSION: u32 = 1;
pub const MAX_PORTABLE_CAPTURE_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_PORTABLE_TRANSACTIONS: usize = 10_000;

#[derive(Debug, Error)]
pub enum PortableCaptureError {
    #[error("portable capture exceeds the 25 MiB safety limit")]
    TooLarge,
    #[error("portable capture is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("portable capture format version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("portable capture contains more than {MAX_PORTABLE_TRANSACTIONS} transactions")]
    TooManyTransactions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableCapture {
    pub format_version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub exported_at: OffsetDateTime,
    pub transactions: Vec<HttpTransaction>,
}

pub fn export_capture(transactions: &[HttpTransaction], now: OffsetDateTime) -> PortableCapture {
    PortableCapture {
        format_version: PORTABLE_CAPTURE_VERSION,
        exported_at: now,
        transactions: transactions.iter().map(sanitize_transaction).collect(),
    }
}

pub fn encode_capture(capture: &PortableCapture) -> Result<String, PortableCaptureError> {
    let encoded = serde_json::to_string_pretty(capture)?;
    if encoded.len() > MAX_PORTABLE_CAPTURE_BYTES {
        return Err(PortableCaptureError::TooLarge);
    }
    Ok(encoded)
}

pub fn import_capture(
    input: &str,
    session_id: Uuid,
    now: OffsetDateTime,
) -> Result<Vec<HttpTransaction>, PortableCaptureError> {
    if input.len() > MAX_PORTABLE_CAPTURE_BYTES {
        return Err(PortableCaptureError::TooLarge);
    }
    let capture: PortableCapture = serde_json::from_str(input)?;
    if capture.format_version != PORTABLE_CAPTURE_VERSION {
        return Err(PortableCaptureError::UnsupportedVersion(
            capture.format_version,
        ));
    }
    if capture.transactions.len() > MAX_PORTABLE_TRANSACTIONS {
        return Err(PortableCaptureError::TooManyTransactions);
    }
    Ok(capture
        .transactions
        .iter()
        .map(|transaction| {
            let mut transaction = sanitize_transaction(transaction);
            transaction.id = Uuid::new_v4();
            transaction.session_id = session_id;
            transaction.connection_id = Uuid::new_v4();
            transaction.comparison = None;
            transaction.correlated_incidents.clear();
            transaction.created_at = now;
            transaction.updated_at = now;
            transaction
        })
        .collect())
}

fn sanitize_transaction(transaction: &HttpTransaction) -> HttpTransaction {
    let mut sanitized = transaction.clone();
    sanitized.request.headers = redact_headers(&sanitized.request.headers);
    sanitized.request.query.iter_mut().for_each(|parameter| {
        if is_secret(&parameter.name) {
            parameter.value = "<redacted>".into();
        }
    });
    sanitized.request.body = omitted_body();
    if let Some(response) = &mut sanitized.response {
        response.headers = redact_headers(&response.headers);
        response.body = omitted_body();
    }
    sanitized.curl = None;
    sanitized.comparison = None;
    sanitized.correlated_incidents.clear();
    sanitized.capture_quality = CaptureQuality::MetadataOnly;
    sanitized
}

fn omitted_body() -> BodyStorage {
    BodyStorage::Unavailable {
        reason: "Body omitted from portable export to protect sensitive data.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::{
        CapturedRequest, CapturedResponse, GeneratedCurl, HeaderEntry, QueryParameter,
        TransactionState, TransactionTiming,
    };

    fn transaction() -> HttpTransaction {
        let now = OffsetDateTime::now_utc();
        HttpTransaction {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            connection_id: Uuid::new_v4(),
            request: CapturedRequest {
                method: "POST".into(),
                scheme: "https".into(),
                host: "api.example.test".into(),
                port: None,
                path: "/payments".into(),
                query: vec![QueryParameter {
                    name: "token".into(),
                    value: "secret-query".into(),
                }],
                headers: vec![HeaderEntry {
                    name: "Authorization".into(),
                    value: "Bearer secret-header".into(),
                }],
                body: BodyStorage::Inline {
                    bytes: b"secret-body".to_vec(),
                },
                content_type: Some("application/json".into()),
                http_version: "HTTP/2".into(),
            },
            response: Some(CapturedResponse {
                status: 200,
                reason: None,
                headers: vec![HeaderEntry {
                    name: "Set-Cookie".into(),
                    value: "secret-cookie".into(),
                }],
                body: BodyStorage::Inline {
                    bytes: b"secret-response".to_vec(),
                },
                content_type: Some("application/json".into()),
                decoded_size: 15,
                encoded_size: 15,
                http_version: "HTTP/2".into(),
            }),
            state: TransactionState::ResponseComplete,
            timing: TransactionTiming::default(),
            endpoint_identity: None,
            curl: Some(GeneratedCurl {
                compact: "secret-curl".into(),
                multiline: "secret-curl".into(),
                redacted: false,
            }),
            capture_quality: CaptureQuality::Complete,
            comparison: None,
            correlated_incidents: vec![Uuid::new_v4()],
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn export_redacts_metadata_and_omits_bodies() {
        let encoded =
            encode_capture(&export_capture(&[transaction()], OffsetDateTime::now_utc())).unwrap();
        for secret in [
            "secret-query",
            "secret-header",
            "secret-body",
            "secret-cookie",
            "secret-response",
            "secret-curl",
        ] {
            assert!(!encoded.contains(secret));
        }
        assert!(encoded.contains("<redacted>"));
        assert!(encoded.contains("Body omitted from portable export"));
    }

    #[test]
    fn import_resanitizes_and_assigns_a_new_local_session() {
        let original = transaction();
        let exported =
            encode_capture(&export_capture(&[original], OffsetDateTime::now_utc())).unwrap();
        let session_id = Uuid::new_v4();
        let imported = import_capture(&exported, session_id, OffsetDateTime::now_utc()).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].session_id, session_id);
        assert!(matches!(
            imported[0].request.body,
            BodyStorage::Unavailable { .. }
        ));
        assert!(imported[0].curl.is_none());
    }
}
