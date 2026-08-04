//! Hudsucker capture handler: redacts traffic, persists transactions, and
//! emits typed events.

use dashmap::DashMap;
use http_body_util::BodyExt;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse,
    hyper::{Request, Response},
};
use serde::Deserialize;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

use super::model::CompanionApp;
use super::stream;
use crate::{
    events::{EventBroadcaster, InspectorEvent},
    persistence::Database,
    traffic::{
        BodyStorage, CaptureQuality, CapturedRequest, CapturedResponse, EndpointIdentity,
        HeaderEntry, HttpTransaction, QueryParameter, TransactionState, TransactionTiming,
        generate_local_curl_with_authorization, is_secret, normalize_path, redact_headers,
        request_shape,
    },
};
use bytes::Bytes;

#[derive(Debug, Clone, Default)]
pub struct CompanionLink {
    pub apps: Vec<CompanionApp>,
    pub selected_package: Option<String>,
}

#[derive(Deserialize)]
struct CompanionRegistration {
    token: String,
    apps: Vec<CompanionApp>,
}

#[derive(Clone)]
pub struct CaptureHandler {
    pub session_id: Uuid,
    pub current_id: Option<Uuid>,
    pub transactions: Arc<DashMap<Uuid, HttpTransaction>>,
    pub recent_by_endpoint: Arc<DashMap<String, HttpTransaction>>,
    pub database: Arc<Database>,
    pub events: EventBroadcaster,
    pub preview_limit: usize,
    pub companion_links: Arc<DashMap<String, CompanionLink>>,
}

pub const MAX_RECENT_ENDPOINTS: usize = 10_000;

/// Upper bound on transactions held in memory during a capture. Completed
/// transactions are persisted, so evicting the oldest ones only releases
/// memory; in-flight transactions are never evicted.
pub const MAX_LIVE_TRANSACTIONS: usize = 10_000;

pub fn evict_completed_transactions(transactions: &DashMap<Uuid, HttpTransaction>) {
    evict_completed_transactions_to(transactions, MAX_LIVE_TRANSACTIONS);
}

fn evict_completed_transactions_to(transactions: &DashMap<Uuid, HttpTransaction>, cap: usize) {
    if transactions.len() <= cap {
        return;
    }
    let excess = transactions.len() - cap;
    let mut candidates: Vec<(Uuid, OffsetDateTime)> = transactions
        .iter()
        .filter(|entry| entry.state == TransactionState::ResponseComplete)
        .map(|entry| (*entry.key(), entry.updated_at))
        .collect();
    candidates.sort_by_key(|(_, updated_at)| *updated_at);
    for (id, _) in candidates.into_iter().take(excess) {
        transactions.remove(&id);
    }
}

pub fn headers(map: &hudsucker::hyper::HeaderMap) -> Vec<HeaderEntry> {
    map.iter()
        .map(|(name, value)| HeaderEntry {
            name: name.to_string(),
            value: value.to_str().unwrap_or("<binary>").to_owned(),
        })
        .collect()
}

pub fn version(version: hudsucker::hyper::Version) -> String {
    format!("{version:?}")
}

pub fn baseline_key(endpoint: &EndpointIdentity) -> String {
    format!(
        "{} {} {}",
        endpoint.method, endpoint.host, endpoint.path_template
    )
}

pub fn remember_recent_endpoint(
    recent_by_endpoint: &DashMap<String, HttpTransaction>,
    transaction: HttpTransaction,
) {
    let Some(endpoint) = transaction.endpoint_identity.as_ref() else {
        return;
    };
    let key = baseline_key(endpoint);
    if !recent_by_endpoint.contains_key(&key)
        && recent_by_endpoint.len() >= MAX_RECENT_ENDPOINTS
        && let Some(eviction_key) = recent_by_endpoint
            .iter()
            .min_by_key(|entry| entry.updated_at)
            .map(|entry| entry.key().clone())
    {
        recent_by_endpoint.remove(&eviction_key);
    }
    recent_by_endpoint.insert(key, transaction);
}

pub fn body_storage(bytes: &[u8], limit: usize) -> BodyStorage {
    if bytes.is_empty() {
        BodyStorage::Empty
    } else if bytes.len() <= limit {
        BodyStorage::Inline {
            bytes: bytes.to_vec(),
        }
    } else {
        BodyStorage::Truncated {
            preview: bytes[..limit].to_vec(),
            original_size: Some(bytes.len() as u64),
        }
    }
}

/// Maps a bounded-capture outcome onto the persisted body representation,
/// mirroring [`body_storage`] for the streaming path.
pub fn body_storage_from_capture(
    errored: bool,
    truncated: bool,
    preview: &[u8],
    original_size: Option<u64>,
) -> BodyStorage {
    if errored {
        BodyStorage::Unavailable {
            reason: "body stream interrupted".into(),
        }
    } else if truncated {
        BodyStorage::Truncated {
            preview: preview.to_vec(),
            original_size,
        }
    } else if preview.is_empty() {
        BodyStorage::Empty
    } else {
        BodyStorage::Inline {
            bytes: preview.to_vec(),
        }
    }
}

/// Whether a `content-type` header identifies a JSON payload. JSON bodies are
/// buffered in full so secrets can be redacted before truncation; every other
/// body type can be captured with bounded memory.
pub fn is_json_content_type(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| value.to_ascii_lowercase().contains("json"))
}

pub fn redact_body(bytes: &[u8], content_type: Option<&str>) -> Vec<u8> {
    if !is_json_content_type(content_type) {
        return bytes.to_vec();
    }
    let Ok(mut value) = serde_json::from_slice(bytes) else {
        return bytes.to_vec();
    };
    crate::traffic::redact_json(&mut value);
    serde_json::to_vec(&value).unwrap_or_default()
}

/// API payloads are normally small enough to retain for inspection.  Downloads are not:
/// collecting one before returning the response back-pressures the client until the whole file
/// has been read.  In particular, this made the locally served companion APK unreliable while
/// the Android device was configured to use the capture proxy.
pub fn should_stream_response(headers: &hudsucker::hyper::HeaderMap, preview_limit: usize) -> bool {
    headers
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > preview_limit)
        || headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.eq_ignore_ascii_case("application/vnd.android.package-archive")
            })
}

pub fn record_streamed_response(
    handler: &mut CaptureHandler,
    status: hudsucker::hyper::StatusCode,
    headers_map: &hudsucker::hyper::HeaderMap,
    response_version: hudsucker::hyper::Version,
) {
    let Some(id) = handler.current_id else {
        return;
    };
    let Some(mut transaction) = handler.transactions.get_mut(&id) else {
        return;
    };
    let now = OffsetDateTime::now_utc();
    let content_type = headers_map
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_length = headers_map
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    transaction.response = Some(CapturedResponse {
        status: status.as_u16(),
        reason: status.canonical_reason().map(str::to_owned),
        headers: redact_headers(&headers(headers_map)),
        body: BodyStorage::Truncated {
            preview: Vec::new(),
            original_size: content_length,
        },
        content_type,
        decoded_size: content_length.unwrap_or_default(),
        encoded_size: content_length.unwrap_or_default(),
        http_version: version(response_version),
    });
    transaction.state = TransactionState::ResponseComplete;
    transaction.capture_quality = CaptureQuality::PreviewOnly;
    transaction.updated_at = now;
    transaction.timing.response_started_ms = Some(now.unix_timestamp_nanos() as i64 / 1_000_000);
    transaction.timing.response_complete_ms = transaction.timing.response_started_ms;
    let completed = transaction.clone();
    remember_recent_endpoint(&handler.recent_by_endpoint, completed.clone());
    let _ = handler.database.upsert_transaction(&completed);
    handler
        .events
        .send(InspectorEvent::TransactionCompleted(completed));
}

/// A 400 response for malformed companion control requests.
fn invalid_registration_response(message: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(message));
    *response.status_mut() = hudsucker::hyper::StatusCode::BAD_REQUEST;
    response
}

impl HttpHandler for CaptureHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        request: Request<Body>,
    ) -> RequestOrResponse {
        let (parts, body) = request.into_parts();
        if parts.uri.path() == "/__app_tester/companion/register" {
            let response = match body.collect().await {
                Ok(collected) => {
                    serde_json::from_slice::<CompanionRegistration>(&collected.to_bytes())
                        .map(|registration| {
                            self.companion_links.insert(
                                registration.token,
                                CompanionLink {
                                    apps: registration.apps,
                                    selected_package: None,
                                },
                            );
                            Response::new(Body::from("{\"connected\":true}"))
                        })
                        .unwrap_or_else(|_| invalid_registration_response("invalid registration"))
                }
                Err(_) => invalid_registration_response("invalid body"),
            };
            return response.into();
        }
        if parts.uri.path() == "/__app_tester/companion/config" {
            let token = parts.uri.query().and_then(|query| {
                url::form_urlencoded::parse(query.as_bytes())
                    .find(|(name, _)| name == "token")
                    .map(|(_, value)| value.into_owned())
            });
            let package_name = token.and_then(|token| {
                self.companion_links
                    .get(&token)
                    .and_then(|link| link.selected_package.clone())
            });
            let body = serde_json::json!({"package_name": package_name}).to_string();
            return Response::new(Body::from(body)).into();
        }
        let now = OffsetDateTime::now_utc();
        let id = Uuid::new_v4();
        self.current_id = Some(id);
        let uri = parts.uri.clone();
        let query = uri
            .query()
            .map(|query| {
                url::form_urlencoded::parse(query.as_bytes())
                    .map(|(name, value)| QueryParameter {
                        value: if is_secret(&name) {
                            "<redacted>".into()
                        } else {
                            value.into_owned()
                        },
                        name: name.into_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let original_headers = headers(&parts.headers);
        let captured_request = CapturedRequest {
            method: parts.method.to_string(),
            scheme: uri.scheme_str().unwrap_or("http").into(),
            host: uri
                .host()
                .or_else(|| {
                    parts
                        .headers
                        .get("host")
                        .and_then(|value| value.to_str().ok())
                        .map(|host| host.split(':').next().unwrap_or(host))
                })
                .unwrap_or("unknown")
                .into(),
            port: uri.port_u16(),
            path: uri.path().to_owned(),
            query,
            content_type: content_type.clone(),
            headers: redact_headers(&original_headers),
            body: BodyStorage::Empty,
            http_version: version(parts.version),
        };
        let transaction = HttpTransaction {
            id,
            session_id: self.session_id,
            connection_id: Uuid::new_v4(),
            request: captured_request,
            response: None,
            state: TransactionState::RequestStarted,
            timing: TransactionTiming {
                request_started_ms: now.unix_timestamp_nanos() as i64 / 1_000_000,
                ..Default::default()
            },
            endpoint_identity: None,
            curl: None,
            capture_quality: CaptureQuality::MetadataOnly,
            comparison: None,
            correlated_incidents: vec![],
            created_at: now,
            updated_at: now,
        };
        self.transactions.insert(id, transaction.clone());
        let _ = self.database.upsert_async(transaction.clone()).await;
        self.events
            .send(InspectorEvent::TransactionCreated(transaction));

        // Capture the request body with bounded memory. JSON payloads are
        // buffered in full so secrets can be redacted before truncation;
        // every other body is streamed through, holding at most the preview
        // limit in memory even for multi-megabyte uploads.
        let jsonish = is_json_content_type(content_type.as_deref());
        let content_length = parts
            .headers
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let stream_body = !jsonish
            && (content_length.is_none()
                || content_length.is_some_and(|length| length > self.preview_limit as u64));
        let (request_body, forwarded_body, truncated) = if stream_body {
            let captured = stream::capture_prefix(body, self.preview_limit).await;
            let stored = body_storage_from_capture(
                captured.errored,
                captured.truncated,
                &captured.preview,
                content_length,
            );
            let forwarded = match captured.rest {
                Some(rest) => rest.into_hudsucker_body(),
                None => Body::from(Bytes::from(captured.preview)),
            };
            (stored, forwarded, captured.truncated)
        } else {
            let bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return Request::from_parts(parts, Body::empty()).into(),
            };
            let stored = if jsonish {
                body_storage(
                    &redact_body(&bytes, content_type.as_deref()),
                    self.preview_limit,
                )
            } else {
                body_storage(&bytes, self.preview_limit)
            };
            let truncated = bytes.len() > self.preview_limit;
            (stored, Body::from(bytes), truncated)
        };
        if let Some(mut transaction) = self.transactions.get_mut(&id) {
            let now = OffsetDateTime::now_utc();
            transaction.request.body = request_body;
            transaction.endpoint_identity = Some(EndpointIdentity {
                method: transaction.request.method.clone(),
                host: transaction.request.host.to_lowercase(),
                path_template: normalize_path(&transaction.request.path),
                content_type: transaction.request.content_type.clone(),
                request_shape: request_shape(&transaction.request.body),
            });
            transaction.curl = Some(generate_local_curl_with_authorization(
                &transaction.request,
                &original_headers,
            ));
            transaction.state = TransactionState::RequestComplete;
            transaction.timing.request_complete_ms =
                Some(now.unix_timestamp_nanos() as i64 / 1_000_000);
            transaction.updated_at = now;
            transaction.capture_quality = if truncated {
                CaptureQuality::PreviewOnly
            } else {
                CaptureQuality::Complete
            };
            let updated = transaction.clone();
            // Release the map's shard lock before the database hop so a slow
            // write cannot block other transactions on the same shard.
            drop(transaction);
            let _ = self.database.upsert_async(updated.clone()).await;
            self.events
                .send(InspectorEvent::TransactionUpdated(updated));
        }
        Request::from_parts(parts, forwarded_body).into()
    }
    async fn handle_response(
        &mut self,
        _ctx: &HttpContext,
        response: Response<Body>,
    ) -> Response<Body> {
        let (parts, body) = response.into_parts();
        if should_stream_response(&parts.headers, self.preview_limit) {
            record_streamed_response(self, parts.status, &parts.headers, parts.version);
            return Response::from_parts(parts, body);
        }
        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        // Chunked non-JSON responses are captured with bounded memory: only
        // the preview is buffered and the remainder is streamed to the client,
        // so downloads without a content-length cannot balloon memory.
        let jsonish = is_json_content_type(content_type.as_deref());
        let (captured, buffered) = if !parts.headers.contains_key("content-length") && !jsonish {
            (
                Some(stream::capture_prefix(body, self.preview_limit).await),
                None,
            )
        } else {
            let bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return Response::from_parts(parts, Body::empty()),
            };
            (None, Some(bytes))
        };
        let previous = if let Some(current_id) = self.current_id {
            let key = self
                .transactions
                .get(&current_id)
                .and_then(|transaction| transaction.endpoint_identity.clone())
                .map(|endpoint| baseline_key(&endpoint));
            match key {
                Some(key) => self
                    .database
                    .pinned_baseline_async(key.clone())
                    .await
                    .ok()
                    .flatten()
                    .or_else(|| self.recent_by_endpoint.get(&key).map(|entry| entry.clone())),
                None => None,
            }
        } else {
            None
        };
        if let Some(id) = self.current_id
            && let Some(mut transaction) = self.transactions.get_mut(&id)
        {
            let now = OffsetDateTime::now_utc();
            let (response_body, decoded_size) = if let Some(captured) = captured.as_ref() {
                let size = captured.total;
                let body = body_storage_from_capture(
                    captured.errored,
                    captured.truncated,
                    &captured.preview,
                    None,
                );
                (body, size)
            } else {
                let bytes = buffered.as_ref().map(|bytes| bytes.as_ref()).unwrap_or(b"");
                let body = if jsonish {
                    body_storage(
                        &redact_body(bytes, content_type.as_deref()),
                        self.preview_limit,
                    )
                } else {
                    body_storage(bytes, self.preview_limit)
                };
                (body, bytes.len() as u64)
            };
            transaction.response = Some(CapturedResponse {
                status: parts.status.as_u16(),
                reason: parts.status.canonical_reason().map(str::to_owned),
                headers: redact_headers(&headers(&parts.headers)),
                body: response_body,
                content_type,
                decoded_size,
                encoded_size: decoded_size,
                http_version: version(parts.version),
            });
            transaction.state = TransactionState::ResponseComplete;
            transaction.updated_at = now;
            transaction.timing.response_started_ms =
                Some(now.unix_timestamp_nanos() as i64 / 1_000_000);
            transaction.timing.response_complete_ms = transaction.timing.response_started_ms;
            if let (Some(previous), Some(current_endpoint), Some(current_response)) = (
                previous,
                transaction.endpoint_identity.as_ref(),
                transaction.response.as_ref(),
            ) {
                let mut differences = Vec::new();
                if let Some(previous_response) = previous.response.as_ref() {
                    if previous_response.status != current_response.status {
                        differences.push(crate::comparison::Difference {
                            kind: crate::comparison::DifferenceKind::StatusChanged,
                            path: None,
                            previous: Some(crate::comparison::DisplayValue(
                                previous_response.status.to_string(),
                            )),
                            current: Some(crate::comparison::DisplayValue(
                                current_response.status.to_string(),
                            )),
                            severity: crate::comparison::DifferenceSeverity::Critical,
                            ignored: false,
                            explanation: "HTTP status changed".into(),
                        });
                    }
                    if let (Some(before), Some(after)) = (
                        previous_response
                            .body
                            .bytes()
                            .and_then(|body| serde_json::from_slice(body).ok()),
                        current_response
                            .body
                            .bytes()
                            .and_then(|body| serde_json::from_slice(body).ok()),
                    ) {
                        let rules = self
                            .database
                            .comparison_rules_async(baseline_key(current_endpoint))
                            .await
                            .unwrap_or_default();
                        differences
                            .extend(crate::comparison::compare_json(&before, &after, &rules));
                    }
                }
                transaction.comparison = Some(crate::comparison::ResponseComparison {
                    baseline_transaction_id: Some(previous.id),
                    compatibility: previous
                        .endpoint_identity
                        .as_ref()
                        .map(|endpoint| {
                            crate::comparison::compatibility(endpoint, current_endpoint)
                        })
                        .unwrap_or(
                            crate::comparison::ComparisonCompatibility::PossiblyIncompatible,
                        ),
                    differences,
                });
            }
            let completed = transaction.clone();
            drop(transaction);
            remember_recent_endpoint(&self.recent_by_endpoint, completed.clone());
            let _ = self.database.upsert_async(completed.clone()).await;
            self.events
                .send(InspectorEvent::TransactionCompleted(completed));
            evict_completed_transactions(&self.transactions);
        }
        let forwarded = match captured {
            Some(captured) => match captured.rest {
                Some(rest) => rest.into_hudsucker_body(),
                None => Body::from(Bytes::from(captured.preview)),
            },
            None => Body::from(buffered.unwrap_or_default()),
        };
        Response::from_parts(parts, forwarded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_large_or_apk_responses_without_buffering_them() {
        let mut large = hudsucker::hyper::HeaderMap::new();
        large.insert("content-length", "1048577".parse().unwrap());
        assert!(should_stream_response(&large, 1024 * 1024));

        let mut apk = hudsucker::hyper::HeaderMap::new();
        apk.insert(
            "content-type",
            "application/vnd.android.package-archive".parse().unwrap(),
        );
        assert!(should_stream_response(&apk, 1024 * 1024));

        let mut small = hudsucker::hyper::HeaderMap::new();
        small.insert("content-length", "1024".parse().unwrap());
        assert!(!should_stream_response(&small, 1024 * 1024));
    }

    #[test]
    fn baseline_key_uses_the_normalized_endpoint_identity() {
        let endpoint = EndpointIdentity {
            method: "GET".into(),
            host: "api.example.test".into(),
            path_template: "/users/{id}".into(),
            content_type: Some("application/json".into()),
            request_shape: None,
        };
        assert_eq!(baseline_key(&endpoint), "GET api.example.test /users/{id}");
    }

    #[test]
    fn body_storage_truncates_over_the_preview_limit_and_keeps_small_bodies_inline() {
        let small = body_storage(b"hello", 8);
        assert!(matches!(small, BodyStorage::Inline { bytes } if bytes == b"hello"));

        let large = body_storage(b"0123456789", 4);
        let BodyStorage::Truncated {
            preview,
            original_size,
        } = large
        else {
            panic!("expected truncated storage");
        };
        assert_eq!(preview, b"0123");
        assert_eq!(original_size, Some(10));
    }

    #[test]
    fn redacts_json_bodies_but_passes_other_content_through() {
        let json = br#"{"token":"abc","name":"safe"}"#;
        let redacted: serde_json::Value =
            serde_json::from_slice(&redact_body(json, Some("application/json"))).unwrap();
        assert_eq!(redacted["token"], "<redacted>");
        assert_eq!(redacted["name"], "safe");
        assert_eq!(redact_body(json, Some("text/plain")), json);
        assert_eq!(
            redact_body(b"not json", Some("application/json")),
            b"not json"
        );
    }

    #[test]
    fn capture_storage_maps_outcomes_to_persisted_bodies() {
        assert!(matches!(
            body_storage_from_capture(true, false, b"partial", None),
            BodyStorage::Unavailable { .. }
        ));
        assert!(matches!(
            body_storage_from_capture(false, true, b"preview", Some(42)),
            BodyStorage::Truncated {
                original_size: Some(42),
                ..
            }
        ));
        assert!(matches!(
            body_storage_from_capture(false, false, b"", None),
            BodyStorage::Empty
        ));
        assert!(matches!(
            body_storage_from_capture(false, false, b"full", None),
            BodyStorage::Inline { .. }
        ));
    }

    #[test]
    fn detects_json_content_types_case_insensitively() {
        assert!(is_json_content_type(Some("application/json")));
        assert!(is_json_content_type(Some("Application/JSON")));
        assert!(is_json_content_type(Some("application/vnd.api+json")));
        assert!(!is_json_content_type(Some("text/plain")));
        assert!(!is_json_content_type(Some("text/event-stream")));
        assert!(!is_json_content_type(None));
    }

    #[test]
    fn evicts_only_completed_entries_when_fewer_than_excess() {
        let transactions = DashMap::new();
        for minute in 1..=2 {
            let mut transaction = test_transaction("a.test", minute);
            transaction.state = TransactionState::ResponseComplete;
            transactions.insert(transaction.id, transaction);
        }
        let mut in_flight_one = test_transaction("b.test", 3);
        in_flight_one.state = TransactionState::RequestStarted;
        let mut in_flight_two = test_transaction("b.test", 4);
        in_flight_two.state = TransactionState::RequestStarted;
        transactions.insert(in_flight_one.id, in_flight_one.clone());
        transactions.insert(in_flight_two.id, in_flight_two.clone());

        // Excess of 2 but only 2 completed candidates: both completed rows go,
        // the in-flight rows stay even though the map remains over the cap.
        evict_completed_transactions_to(&transactions, 2);

        assert_eq!(transactions.len(), 2);
        assert!(transactions.contains_key(&in_flight_one.id));
        assert!(transactions.contains_key(&in_flight_two.id));
    }

    #[test]
    fn recall_recent_endpoint_evicts_oldest_entries() {
        let recent = DashMap::new();
        let endpoint = |host: &str| EndpointIdentity {
            method: "GET".into(),
            host: host.into(),
            path_template: "/v1".into(),
            content_type: None,
            request_shape: None,
        };
        let transaction = |host: &str, updated_at: i64| HttpTransaction {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            connection_id: Uuid::new_v4(),
            request: CapturedRequest {
                method: "GET".into(),
                scheme: "https".into(),
                host: host.into(),
                port: None,
                path: "/v1".into(),
                query: vec![],
                headers: vec![],
                body: BodyStorage::Empty,
                content_type: None,
                http_version: "HTTP/1.1".into(),
            },
            response: None,
            state: TransactionState::RequestStarted,
            timing: TransactionTiming::default(),
            endpoint_identity: Some(endpoint(host)),
            curl: None,
            capture_quality: CaptureQuality::MetadataOnly,
            comparison: None,
            correlated_incidents: vec![],
            created_at: OffsetDateTime::from_unix_timestamp(updated_at).unwrap(),
            updated_at: OffsetDateTime::from_unix_timestamp(updated_at).unwrap(),
        };
        let first = transaction("a.test", 1);
        let second = transaction("b.test", 2);
        remember_recent_endpoint(&recent, first);
        remember_recent_endpoint(&recent, second);
        assert_eq!(recent.len(), 2);
        let mut third = transaction("c.test", 3);
        third.updated_at = OffsetDateTime::from_unix_timestamp(3).unwrap();
        remember_recent_endpoint(&recent, third);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn evicts_oldest_completed_transactions_but_keeps_in_flight_ones() {
        let transactions = DashMap::new();
        let completed = |minute: i64| {
            let mut transaction = test_transaction("a.test", minute);
            transaction.state = TransactionState::ResponseComplete;
            transaction
        };
        let oldest = completed(1);
        let middle = completed(2);
        let newest = completed(3);
        for tx in [&oldest, &middle, &newest] {
            transactions.insert(tx.id, tx.clone());
        }
        let mut in_flight = test_transaction("b.test", 4);
        in_flight.state = TransactionState::RequestStarted;
        transactions.insert(in_flight.id, in_flight.clone());

        evict_completed_transactions_to(&transactions, 2);

        assert_eq!(transactions.len(), 2);
        assert!(transactions.contains_key(&newest.id));
        assert!(transactions.contains_key(&in_flight.id));
        assert!(!transactions.contains_key(&oldest.id));
        assert!(!transactions.contains_key(&middle.id));
    }

    fn test_transaction(host: &str, updated_at: i64) -> HttpTransaction {
        HttpTransaction {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            connection_id: Uuid::new_v4(),
            request: CapturedRequest {
                method: "GET".into(),
                scheme: "https".into(),
                host: host.into(),
                port: None,
                path: "/v1".into(),
                query: vec![],
                headers: vec![],
                body: BodyStorage::Empty,
                content_type: None,
                http_version: "HTTP/1.1".into(),
            },
            response: None,
            state: TransactionState::ResponseComplete,
            timing: TransactionTiming::default(),
            endpoint_identity: Some(EndpointIdentity {
                method: "GET".into(),
                host: host.into(),
                path_template: "/v1".into(),
                content_type: None,
                request_shape: None,
            }),
            curl: None,
            capture_quality: CaptureQuality::MetadataOnly,
            comparison: None,
            correlated_incidents: vec![],
            created_at: OffsetDateTime::from_unix_timestamp(updated_at).unwrap(),
            updated_at: OffsetDateTime::from_unix_timestamp(updated_at).unwrap(),
        }
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    use hudsucker::hyper::{HeaderMap, Version};

    fn header_map(pairs: &[(&str, &str)]) -> HeaderMap {
        use hudsucker::hyper::header::{HeaderName, HeaderValue};
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn converts_hyper_headers_to_entries_in_order() {
        let map = header_map(&[
            ("content-type", "application/json"),
            ("x-app-tester", "yes"),
        ]);
        let entries = headers(&map);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "content-type");
        assert_eq!(entries[0].value, "application/json");
        assert_eq!(entries[1].name, "x-app-tester");
        assert_eq!(entries[1].value, "yes");
    }

    #[test]
    fn maps_http_versions_to_stable_strings() {
        assert_eq!(version(Version::HTTP_09), "HTTP/0.9");
        assert_eq!(version(Version::HTTP_10), "HTTP/1.0");
        assert_eq!(version(Version::HTTP_11), "HTTP/1.1");
        assert_eq!(version(Version::HTTP_2), "HTTP/2.0");
        assert_eq!(version(Version::HTTP_3), "HTTP/3.0");
    }

    #[test]
    fn streams_large_payloads_and_apks_but_buffers_small_responses() {
        assert!(should_stream_response(
            &header_map(&[("content-length", "999999")]),
            1024
        ));
        assert!(!should_stream_response(
            &header_map(&[("content-length", "100")]),
            1024
        ));
        assert!(should_stream_response(
            &header_map(&[("content-type", "application/vnd.android.package-archive")]),
            1024
        ));
        assert!(!should_stream_response(
            &header_map(&[("content-type", "application/json")]),
            1024
        ));
        assert!(!should_stream_response(&HeaderMap::new(), 1024));
    }
}
