//! Safe replay of previously captured requests for regression checks.
use crate::{
    comparison::{
        ComparisonCompatibility, Difference, DifferenceKind, DifferenceSeverity, DisplayValue,
        ResponseComparison,
    },
    traffic::{
        BodyStorage, CapturedResponse, HeaderEntry, HttpTransaction, TransactionState,
        redact_headers, redact_json,
    },
};
use reqwest::{Client, Method};
use time::OffsetDateTime;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ReplaySummary {
    pub attempted: usize,
    pub completed: usize,
    pub changed: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub fn replay_blocker(transaction: &HttpTransaction) -> Option<&'static str> {
    if transaction.response.is_none()
        || !matches!(transaction.request.scheme.as_str(), "http" | "https")
    {
        return Some("only completed HTTP requests can be replayed");
    }
    if transaction
        .request
        .headers
        .iter()
        .any(|header| header.value == "<redacted>")
        || transaction
            .request
            .query
            .iter()
            .any(|entry| entry.value == "<redacted>")
        || transaction
            .request
            .body
            .bytes()
            .is_some_and(|body| body.windows(10).any(|part| part == b"<redacted>"))
    {
        return Some("request contains redacted credentials or data");
    }
    if transaction.request.body.bytes().is_none() {
        return Some("request body is unavailable");
    }
    None
}

pub async fn replay(
    client: &Client,
    baseline: &HttpTransaction,
    session_id: uuid::Uuid,
) -> HttpTransaction {
    let started = OffsetDateTime::now_utc();
    let mut transaction = baseline.clone();
    transaction.id = uuid::Uuid::new_v4();
    transaction.session_id = session_id;
    transaction.connection_id = uuid::Uuid::new_v4();
    transaction.created_at = started;
    transaction.updated_at = started;
    transaction.state = TransactionState::RequestStarted;
    transaction.response = None;
    transaction.comparison = None;
    transaction.timing = crate::traffic::TransactionTiming {
        request_started_ms: started.unix_timestamp_nanos() as i64 / 1_000_000,
        ..Default::default()
    };
    let url = request_url(baseline);
    let method = match Method::from_bytes(baseline.request.method.as_bytes()) {
        Ok(method) => method,
        Err(error) => return failed(transaction, error.to_string()),
    };
    let mut request = client.request(method, url);
    for header in &baseline.request.headers {
        if ["host", "content-length", "connection", "proxy-connection"]
            .iter()
            .any(|name| header.name.eq_ignore_ascii_case(name))
        {
            continue;
        }
        request = request.header(&header.name, &header.value);
    }
    if let Some(body) = baseline
        .request
        .body
        .bytes()
        .filter(|body| !body.is_empty())
    {
        request = request.body(body.to_vec());
    }
    transaction.state = TransactionState::RequestComplete;
    transaction.timing.request_complete_ms = Some(now_ms());
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let headers = response.headers().clone();
            let version = format!("{:?}", response.version());
            match response.bytes().await {
                Ok(bytes) => {
                    let mut body = bytes.to_vec();
                    let content_type = headers
                        .get("content-type")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned);
                    if content_type
                        .as_deref()
                        .is_some_and(|value| value.to_ascii_lowercase().contains("json"))
                        && let Ok(mut json) = serde_json::from_slice(&body)
                    {
                        redact_json(&mut json);
                        body = serde_json::to_vec(&json).unwrap_or_default();
                    }
                    let replay_response = CapturedResponse {
                        status: status.as_u16(),
                        reason: status.canonical_reason().map(str::to_owned),
                        headers: redact_headers(
                            &headers
                                .iter()
                                .map(|(name, value)| HeaderEntry {
                                    name: name.to_string(),
                                    value: value.to_str().unwrap_or("<binary>").to_owned(),
                                })
                                .collect::<Vec<_>>(),
                        ),
                        body: BodyStorage::Inline {
                            bytes: body.clone(),
                        },
                        content_type,
                        decoded_size: body.len() as u64,
                        encoded_size: body.len() as u64,
                        http_version: version,
                    };
                    transaction.comparison = baseline
                        .response
                        .as_ref()
                        .map(|previous| compare(previous, &replay_response, baseline.id));
                    transaction.response = Some(replay_response);
                    transaction.state = TransactionState::ResponseComplete;
                    transaction.timing.response_started_ms = Some(now_ms());
                    transaction.timing.response_complete_ms =
                        transaction.timing.response_started_ms;
                    transaction.updated_at = OffsetDateTime::now_utc();
                    transaction
                }
                Err(error) => failed(transaction, error.to_string()),
            }
        }
        Err(error) => failed(transaction, error.to_string()),
    }
}

fn request_url(transaction: &HttpTransaction) -> String {
    let mut url = format!(
        "{}://{}{}{}",
        transaction.request.scheme,
        transaction.request.host,
        transaction
            .request
            .port
            .map(|port| format!(":{port}"))
            .unwrap_or_default(),
        transaction.request.path
    );
    if !transaction.request.query.is_empty() {
        url.push('?');
        url.push_str(
            &url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(
                    transaction
                        .request
                        .query
                        .iter()
                        .map(|entry| (&entry.name, &entry.value)),
                )
                .finish(),
        );
    }
    url
}
fn now_ms() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}
fn failed(mut transaction: HttpTransaction, reason: String) -> HttpTransaction {
    transaction.state = TransactionState::Failed;
    transaction.updated_at = OffsetDateTime::now_utc();
    transaction.timing.response_complete_ms = Some(now_ms());
    transaction.correlated_incidents = vec![];
    transaction.capture_quality = crate::traffic::CaptureQuality::Unavailable;
    transaction.request.headers.push(HeaderEntry {
        name: "x-app-tester-replay-error".into(),
        value: reason,
    });
    transaction
}
fn compare(
    previous: &CapturedResponse,
    current: &CapturedResponse,
    baseline: uuid::Uuid,
) -> ResponseComparison {
    let mut differences = Vec::new();
    if previous.status != current.status {
        differences.push(Difference {
            kind: DifferenceKind::StatusChanged,
            path: None,
            previous: Some(DisplayValue(previous.status.to_string())),
            current: Some(DisplayValue(current.status.to_string())),
            severity: DifferenceSeverity::Critical,
            ignored: false,
            explanation: "HTTP status changed".into(),
        });
    }
    if let (Some(before), Some(after)) = (
        previous
            .body
            .bytes()
            .and_then(|body| serde_json::from_slice(body).ok()),
        current
            .body
            .bytes()
            .and_then(|body| serde_json::from_slice(body).ok()),
    ) {
        differences.extend(crate::comparison::compare_json(
            &before,
            &after,
            &crate::comparison::ComparisonRules::default(),
        ));
    }
    ResponseComparison {
        baseline_transaction_id: Some(baseline),
        compatibility: ComparisonCompatibility::Exact,
        differences,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_redacted_requests_from_replay() {
        let transaction: HttpTransaction = serde_json::from_value(serde_json::json!({
            "id":uuid::Uuid::new_v4(),"session_id":uuid::Uuid::new_v4(),"connection_id":uuid::Uuid::new_v4(),"request":{"method":"GET","scheme":"https","host":"api.example.test","path":"/v1","query":[],"headers":[{"name":"authorization","value":"<redacted>"}],"body":{"storage":"empty"},"http_version":"HTTP_1_1"},"response":{"status":200,"headers":[],"body":{"storage":"empty"},"decoded_size":0,"encoded_size":0,"http_version":"HTTP_1_1"},"state":"response_complete","timing":{"request_started_ms":0},"capture_quality":"complete","correlated_incidents":[],"created_at":"2026-07-24T00:00:00Z","updated_at":"2026-07-24T00:00:00Z"
        })).unwrap();
        assert_eq!(
            replay_blocker(&transaction),
            Some("request contains redacted credentials or data")
        );
    }
}
