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

pub fn redact_body(bytes: &[u8], content_type: Option<&str>) -> Vec<u8> {
    if !content_type.is_some_and(|value| value.to_ascii_lowercase().contains("json")) {
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
                        .unwrap_or_else(|_| {
                            Response::builder()
                                .status(400)
                                .body(Body::from("invalid registration"))
                                .expect("valid response")
                        })
                }
                Err(_) => Response::builder()
                    .status(400)
                    .body(Body::from("invalid body"))
                    .expect("valid response"),
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
        let _ = self.database.upsert_transaction(&transaction);
        self.events
            .send(InspectorEvent::TransactionCreated(transaction));

        let bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return Request::from_parts(parts, Body::empty()).into(),
        };
        let redacted = redact_body(&bytes, content_type.as_deref());
        if let Some(mut transaction) = self.transactions.get_mut(&id) {
            let now = OffsetDateTime::now_utc();
            transaction.request.body = body_storage(&redacted, self.preview_limit);
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
            transaction.capture_quality = if bytes.len() > self.preview_limit {
                CaptureQuality::PreviewOnly
            } else {
                CaptureQuality::Complete
            };
            let updated = transaction.clone();
            let _ = self.database.upsert_transaction(&updated);
            self.events
                .send(InspectorEvent::TransactionUpdated(updated));
        }
        Request::from_parts(parts, Body::from(bytes)).into()
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
        let bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return Response::from_parts(parts, Body::empty()),
        };
        let previous = self.current_id.and_then(|current_id| {
            let endpoint = self
                .transactions
                .get(&current_id)?
                .endpoint_identity
                .clone()?;
            let key = baseline_key(&endpoint);
            self.database
                .pinned_baseline(&key)
                .ok()
                .flatten()
                .or_else(|| self.recent_by_endpoint.get(&key).map(|entry| entry.clone()))
        });
        if let Some(id) = self.current_id
            && let Some(mut transaction) = self.transactions.get_mut(&id)
        {
            let now = OffsetDateTime::now_utc();
            let content_type = parts
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let redacted = redact_body(&bytes, content_type.as_deref());
            transaction.response = Some(CapturedResponse {
                status: parts.status.as_u16(),
                reason: parts.status.canonical_reason().map(str::to_owned),
                headers: redact_headers(&headers(&parts.headers)),
                body: body_storage(&redacted, self.preview_limit),
                content_type,
                decoded_size: bytes.len() as u64,
                encoded_size: bytes.len() as u64,
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
                            .comparison_rules(&baseline_key(current_endpoint))
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
            remember_recent_endpoint(&self.recent_by_endpoint, completed.clone());
            let _ = self.database.upsert_transaction(&completed);
            self.events
                .send(InspectorEvent::TransactionCompleted(completed));
        }
        Response::from_parts(parts, Body::from(bytes))
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
