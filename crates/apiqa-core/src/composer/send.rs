//! The send pipeline: compose-ready request in, recorded transaction out.
//!
//! Every send is recorded exactly like captured traffic — same model, same
//! storage, same events — so a manual request is indistinguishable from a
//! proxied one in the UI. Memory stays bounded: request bodies are small by
//! construction and response bodies are captured with a preview cap.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::{Client, Method, Proxy, Url, redirect::Policy};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{
    body::{PreparedBody, prepare},
    capture::{CapturedBody, capture_bounded},
    model::{AuthSpec, ManualBody, ManualRequest, SendError, SendOptions, SendResult},
};
use crate::{
    events::{EventBroadcaster, InspectorEvent},
    persistence::Database,
    proxy::{body_storage_from_capture, redact_body},
    traffic::{
        BodyStorage, CaptureQuality, CapturedRequest, CapturedResponse, HeaderEntry,
        HttpTransaction, QueryParameter, TransactionState, TransactionTiming,
    },
};

/// How many option profiles keep a pooled client at most.
const CLIENT_CACHE_CAP: usize = 8;

/// Reuses pooled reqwest clients per option profile. Building a fresh client
/// per send would discard connection and TLS session caches.
struct ClientCache {
    clients: Mutex<Vec<(SendOptions, Client)>>,
}

static CLIENT_CACHE: ClientCache = ClientCache {
    clients: Mutex::new(Vec::new()),
};

impl ClientCache {
    fn get(&self, options: &SendOptions) -> Option<Client> {
        let clients = self
            .clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clients
            .iter()
            .find(|(key, _)| key == options)
            .map(|(_, client)| client.clone())
    }

    fn insert(&self, options: SendOptions, client: Client) {
        let mut clients = self
            .clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clients.retain(|(key, _)| key != &options);
        clients.insert(0, (options, client));
        clients.truncate(CLIENT_CACHE_CAP);
    }
}

/// Sends a composed request and records the outcome as a transaction in the
/// session, streaming `TransactionCreated` then `TransactionCompleted`.
/// Failed sends are still recorded, with [`TransactionState::Failed`].
pub async fn send_manual(
    database: Arc<Database>,
    events: EventBroadcaster,
    session_id: Uuid,
    request: ManualRequest,
    options: SendOptions,
) -> Result<SendResult, SendError> {
    let started = OffsetDateTime::now_utc();
    let started_ms = now_ms();
    let url = resolve_url(&request)?;

    let mut transaction = build_transaction(session_id, &request, &url, started, started_ms);
    events.send(InspectorEvent::TransactionCreated(transaction.clone()));

    let result = execute(&request, &options, &url, &mut transaction, started_ms).await;

    database
        .upsert_async(transaction.clone())
        .await
        .map_err(|error| SendError::Storage(error.to_string()))?;
    events.send(InspectorEvent::TransactionCompleted(transaction));

    result
}

/// Parses the URL and appends editor query rows plus query-based API keys.
fn resolve_url(request: &ManualRequest) -> Result<Url, SendError> {
    let mut url =
        Url::parse(&request.url).map_err(|error| SendError::InvalidUrl(error.to_string()))?;
    for entry in &request.query {
        url.query_pairs_mut().append_pair(&entry.name, &entry.value);
    }
    if let AuthSpec::ApiKey {
        key,
        value,
        in_query: true,
    } = &request.auth
    {
        url.query_pairs_mut().append_pair(key, value);
    }
    Ok(url)
}

async fn execute(
    request: &ManualRequest,
    options: &SendOptions,
    url: &Url,
    transaction: &mut HttpTransaction,
    started_ms: i64,
) -> Result<SendResult, SendError> {
    let client = client_for(options).await?;
    let prepared = prepare(&request.body).await?;
    let request_content_type = effective_content_type(request, &prepared);
    let body_bytes = prepared.wire_bytes().to_vec();
    let effective_headers = effective_headers(request);

    let mut builder = request_builder(&client, request, &effective_headers, url)?;
    match prepared {
        PreparedBody::Multipart(form) => builder = builder.multipart(form),
        _ => {
            if !body_bytes.is_empty() {
                builder = builder.body(body_bytes.clone());
            }
            transaction.request.body = redacted_storage(&body_bytes, &request_content_type);
        }
    }
    transaction.request.headers = effective_headers;
    transaction.request.content_type = request_content_type.clone();
    if !user_set_content_type(request)
        && let Some(media_type) = &request_content_type
    {
        builder = builder.header("content-type", media_type.clone());
    }

    transaction.state = TransactionState::RequestComplete;
    transaction.timing.request_complete_ms = Some(now_ms());

    match builder.send().await {
        Ok(response) => {
            transaction.timing.response_started_ms = Some(now_ms());
            let status = response.status();
            let headers = response
                .headers()
                .iter()
                .map(|(name, value)| HeaderEntry {
                    name: name.as_str().to_owned(),
                    value: String::from_utf8_lossy(value.as_bytes()).into_owned(),
                })
                .collect::<Vec<_>>();
            let response_content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let http_version = format!("{:?}", response.version());

            let captured = capture_bounded(response).await;
            transaction.timing.response_complete_ms = Some(now_ms());
            Ok(finish(
                transaction,
                CapturedResponse {
                    status: status.as_u16(),
                    reason: status.canonical_reason().map(str::to_owned),
                    headers,
                    body: BodyStorage::Empty,
                    content_type: response_content_type.clone(),
                    decoded_size: 0,
                    encoded_size: 0,
                    http_version,
                },
                captured,
                started_ms,
            ))
        }
        Err(error) => {
            transaction.state = TransactionState::Failed;
            transaction.capture_quality = CaptureQuality::Unavailable;
            Err(SendError::Request(error.to_string()))
        }
    }
}

/// User headers plus auth-derived ones, in send order — what is stored is
/// exactly what goes on the wire.
fn effective_headers(request: &ManualRequest) -> Vec<HeaderEntry> {
    let mut headers = request.headers.clone();
    match &request.auth {
        AuthSpec::None => {}
        AuthSpec::Bearer { token } => headers.push(HeaderEntry {
            name: "Authorization".into(),
            value: format!("Bearer {token}"),
        }),
        AuthSpec::Basic { username, password } => {
            let credentials = BASE64.encode(format!("{username}:{password}"));
            headers.push(HeaderEntry {
                name: "Authorization".into(),
                value: format!("Basic {credentials}"),
            });
        }
        AuthSpec::ApiKey {
            key,
            value,
            in_query: false,
        } => headers.push(HeaderEntry {
            name: key.clone(),
            value: value.clone(),
        }),
        AuthSpec::ApiKey { in_query: true, .. } => {}
    }
    headers
}

fn request_builder(
    client: &Client,
    request: &ManualRequest,
    headers: &[HeaderEntry],
    url: &Url,
) -> Result<reqwest::RequestBuilder, SendError> {
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|error| SendError::InvalidMethod(error.to_string()))?;
    let mut builder = client.request(method, url.clone());
    for header in headers {
        if is_hop_header(&header.name) {
            continue;
        }
        builder = builder.header(&header.name, &header.value);
    }
    Ok(builder)
}

fn effective_content_type(request: &ManualRequest, prepared: &PreparedBody) -> Option<String> {
    match (&request.body, prepared) {
        (ManualBody::Raw { media_type, .. }, _) => media_type.clone(),
        (_, prepared) => prepared.content_type().map(str::to_owned),
    }
}

fn user_set_content_type(request: &ManualRequest) -> bool {
    request
        .headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case("content-type"))
}

fn is_hop_header(name: &str) -> bool {
    [
        "host",
        "content-length",
        "connection",
        "proxy-connection",
        "transfer-encoding",
        "upgrade",
    ]
    .iter()
    .any(|hop| name.eq_ignore_ascii_case(hop))
}

fn redacted_storage(bytes: &[u8], content_type: &Option<String>) -> BodyStorage {
    let redacted = redact_body(bytes, content_type.as_deref());
    body_storage_from_capture(false, false, &redacted, Some(bytes.len() as u64))
}

fn build_transaction(
    session_id: Uuid,
    request: &ManualRequest,
    url: &Url,
    started: OffsetDateTime,
    started_ms: i64,
) -> HttpTransaction {
    HttpTransaction {
        id: Uuid::new_v4(),
        session_id,
        connection_id: Uuid::new_v4(),
        request: CapturedRequest {
            method: request.method.clone(),
            scheme: url.scheme().to_string(),
            host: url.host_str().unwrap_or_default().to_string(),
            port: url.port(),
            path: url.path().to_string(),
            query: url
                .query_pairs()
                .map(|(name, value)| QueryParameter {
                    name: name.into_owned(),
                    value: value.into_owned(),
                })
                .collect(),
            headers: request.headers.clone(),
            body: BodyStorage::Empty,
            content_type: None,
            http_version: "HTTP/1.1".into(),
        },
        response: None,
        state: TransactionState::RequestStarted,
        timing: TransactionTiming {
            request_started_ms: started_ms,
            ..Default::default()
        },
        endpoint_identity: None,
        curl: None,
        capture_quality: CaptureQuality::MetadataOnly,
        comparison: None,
        correlated_incidents: vec![],
        created_at: started,
        updated_at: started,
    }
}

/// Fills the captured response into the transaction and returns the summary
/// the composer UI renders. Built from owned values, so no `Option` reads.
fn finish(
    transaction: &mut HttpTransaction,
    mut response: CapturedResponse,
    captured: CapturedBody,
    started_ms: i64,
) -> SendResult {
    let redacted = redact_body(&captured.preview, response.content_type.as_deref());
    response.body = body_storage_from_capture(
        captured.errored,
        captured.truncated,
        &redacted,
        Some(captured.total_bytes),
    );
    response.decoded_size = captured.total_bytes;
    response.encoded_size = captured.total_bytes;
    transaction.updated_at = OffsetDateTime::now_utc();

    let result = SendResult {
        transaction_id: transaction.id,
        state: TransactionState::ResponseComplete,
        status: response.status,
        reason: response.reason.clone(),
        elapsed_ms: (now_ms() - started_ms).max(0) as u64,
        total_bytes: captured.total_bytes,
        body: response.body.clone(),
        content_type: response.content_type.clone(),
        headers: response.headers.clone(),
        http_version: response.http_version.clone(),
    };
    transaction.response = Some(response);
    transaction.state = result.state.clone();
    transaction.capture_quality = if captured.errored {
        CaptureQuality::Unavailable
    } else if captured.truncated {
        CaptureQuality::PreviewOnly
    } else {
        CaptureQuality::Complete
    };
    result
}

async fn client_for(options: &SendOptions) -> Result<Client, SendError> {
    if let Some(client) = CLIENT_CACHE.get(options) {
        return Ok(client);
    }
    let mut builder = Client::builder()
        .timeout(Duration::from_millis(options.timeout_ms))
        .redirect(if options.follow_redirects {
            Policy::limited(options.max_redirects as usize)
        } else {
            Policy::none()
        })
        .danger_accept_invalid_certs(!options.verify_tls);
    if let Some(proxy_url) = &options.proxy_url {
        let proxy = Proxy::all(proxy_url)
            .map_err(|error| SendError::Request(format!("invalid proxy URL: {error}")))?;
        builder = builder.proxy(proxy);
    }
    let client = builder
        .build()
        .map_err(|error| SendError::Request(error.to_string()))?;
    CLIENT_CACHE.insert(options.clone(), client.clone());
    Ok(client)
}

fn now_ms() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        composer::{
            capture::PREVIEW_LIMIT,
            model::{ManualBody, MultipartField},
        },
        persistence::Database,
        traffic::BodyStorage,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // ------------------------------------------------------------------
    // Test HTTP server: reads a full request, hands it to a closure, and
    // writes back whatever the closure returns.
    // ------------------------------------------------------------------

    async fn serve(handler: impl Fn(&[u8]) -> Vec<u8> + Send + Sync + 'static) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let request = read_request(&mut socket).await;
                let response = handler(&request);
                let _ = socket.write_all(&response).await;
            }
        });
        port
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut data = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) => break,
                Ok(read) => {
                    data.extend_from_slice(&buffer[..read]);
                    if request_complete(&data) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        data
    }

    fn request_complete(data: &[u8]) -> bool {
        let Some(header_end) = data.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&data[..header_end]);
        match headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|value| value.trim().to_owned())
            })
            .and_then(|value| value.parse::<usize>().ok())
        {
            Some(length) => data.len() >= header_end + 4 + length,
            None => true,
        }
    }

    fn http_response(status: &str, content_type: &str, body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn redirect_response(location: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .into_bytes()
    }

    async fn send(
        request: ManualRequest,
        options: SendOptions,
    ) -> (SendResult, HttpTransaction, Vec<InspectorEvent>) {
        let database = Arc::new(Database::open_in_memory().unwrap());
        let events = EventBroadcaster::default();
        let mut receiver = events.subscribe();
        let session_id = Uuid::new_v4();
        let result = send_manual(database.clone(), events, session_id, request, options)
            .await
            .unwrap();
        let stored = database
            .transactions_between_async(
                time::OffsetDateTime::now_utc() - time::Duration::minutes(1),
                time::OffsetDateTime::now_utc() + time::Duration::minutes(1),
            )
            .await
            .unwrap()
            .into_iter()
            .find(|transaction| transaction.id == result.transaction_id)
            .unwrap();
        let mut seen = vec![];
        while let Ok(event) = receiver.try_recv() {
            seen.push(event);
        }
        (result, stored, seen)
    }

    #[tokio::test]
    async fn sends_get_and_stores_redacted_response() {
        let port =
            serve(|_| http_response("200 OK", "application/json", r#"{"token":"sk-secret"}"#))
                .await;
        let request = ManualRequest {
            url: format!("http://127.0.0.1:{port}/v1/items?page=1"),
            ..Default::default()
        };
        let (result, stored, seen) = send(request, SendOptions::default()).await;

        assert_eq!(result.status, 200);
        assert_eq!(stored.state, TransactionState::ResponseComplete);
        assert_eq!(stored.request.host, "127.0.0.1");
        assert_eq!(stored.request.path, "/v1/items");
        assert_eq!(stored.request.query.len(), 1);
        let body = stored.response.unwrap().body;
        let bytes = body.bytes().unwrap();
        assert!(!bytes.windows(9).any(|part| part == b"sk-secret"));
        assert!(String::from_utf8_lossy(bytes).contains("<redacted>"));
        assert!(matches!(
            seen.as_slice(),
            [
                InspectorEvent::TransactionCreated(_),
                InspectorEvent::TransactionCompleted(_)
            ]
        ));
    }

    #[tokio::test]
    async fn sends_urlencoded_forms_and_stores_exact_bytes() {
        let port = serve(|request| {
            let text = String::from_utf8_lossy(request);
            if text
                .to_ascii_lowercase()
                .contains("application/x-www-form-urlencoded")
                && text.contains("name=two+words")
            {
                http_response("200 OK", "text/plain", "received")
            } else {
                http_response("400 Bad Request", "text/plain", "bad form")
            }
        })
        .await;
        let request = ManualRequest {
            method: "POST".into(),
            url: format!("http://127.0.0.1:{port}/form"),
            body: ManualBody::Form {
                fields: vec![
                    ("name".into(), "two words".into()),
                    ("n".into(), "1".into()),
                ],
            },
            ..Default::default()
        };
        let (result, stored, _) = send(request, SendOptions::default()).await;

        assert_eq!(result.status, 200);
        assert_eq!(
            stored.request.body.bytes(),
            Some(b"name=two+words&n=1".as_slice())
        );
        assert_eq!(
            stored.request.content_type.as_deref(),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[tokio::test]
    async fn sends_multipart_with_a_streamed_file() {
        let directory = std::env::temp_dir().join(format!("composer-send-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let file_path = directory.join("payload.bin");
        std::fs::write(&file_path, b"file-content-123").unwrap();

        let port = serve(|request| {
            let text = String::from_utf8_lossy(request);
            if text
                .to_ascii_lowercase()
                .contains("multipart/form-data; boundary=")
                && text.contains("file-content-123")
                && text.contains("payload.bin")
            {
                http_response("200 OK", "text/plain", "received")
            } else {
                http_response("400 Bad Request", "text/plain", "bad multipart")
            }
        })
        .await;
        let request = ManualRequest {
            method: "POST".into(),
            url: format!("http://127.0.0.1:{port}/upload"),
            body: ManualBody::Multipart {
                fields: vec![MultipartField {
                    name: "upload".into(),
                    value: None,
                    file: Some(file_path.to_string_lossy().into_owned()),
                    media_type: Some("application/octet-stream".into()),
                }],
            },
            ..Default::default()
        };
        let (result, stored, _) = send(request, SendOptions::default()).await;

        assert_eq!(result.status, 200);
        // Multipart bodies stream from disk and are not buffered for storage.
        assert!(matches!(stored.request.body, BodyStorage::Empty));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[tokio::test]
    async fn follows_redirects_when_enabled() {
        let port = serve(|request| {
            let text = String::from_utf8_lossy(request);
            if text.starts_with("GET /start") {
                redirect_response("/end")
            } else {
                http_response("200 OK", "text/plain", "done")
            }
        })
        .await;

        let (result, _, _) = send(
            ManualRequest {
                url: format!("http://127.0.0.1:{port}/start"),
                ..Default::default()
            },
            SendOptions::default(),
        )
        .await;
        assert_eq!(result.status, 200);

        let (result, _, _) = send(
            ManualRequest {
                url: format!("http://127.0.0.1:{port}/start"),
                ..Default::default()
            },
            SendOptions {
                follow_redirects: false,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(result.status, 302);
    }

    #[tokio::test]
    async fn truncates_responses_larger_than_the_preview() {
        let large = "x".repeat(PREVIEW_LIMIT + 4096);
        let port =
            serve(move |_| http_response("200 OK", "application/octet-stream", &large)).await;
        let (result, stored, _) = send(
            ManualRequest {
                url: format!("http://127.0.0.1:{port}/large"),
                ..Default::default()
            },
            SendOptions::default(),
        )
        .await;

        assert_eq!(result.total_bytes, (PREVIEW_LIMIT + 4096) as u64);
        assert_eq!(result.body.bytes().unwrap().len(), PREVIEW_LIMIT);
        assert!(matches!(
            stored.response.unwrap().body,
            BodyStorage::Truncated { .. }
        ));
        assert!(matches!(
            stored.capture_quality,
            CaptureQuality::PreviewOnly
        ));
    }

    #[tokio::test]
    async fn records_failed_transactions_on_timeout() {
        let port = serve(|_| {
            std::thread::sleep(std::time::Duration::from_secs(5));
            http_response("200 OK", "text/plain", "too late")
        })
        .await;

        let database = Arc::new(Database::open_in_memory().unwrap());
        let events = EventBroadcaster::default();
        let result = send_manual(
            database.clone(),
            events,
            Uuid::new_v4(),
            ManualRequest {
                url: format!("http://127.0.0.1:{port}/slow"),
                ..Default::default()
            },
            SendOptions {
                timeout_ms: 200,
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_err());
        let stored = database
            .transactions_between_async(
                time::OffsetDateTime::now_utc() - time::Duration::minutes(1),
                time::OffsetDateTime::now_utc() + time::Duration::minutes(1),
            )
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].state, TransactionState::Failed);
        assert!(matches!(
            stored[0].capture_quality,
            CaptureQuality::Unavailable
        ));
    }

    #[tokio::test]
    async fn basic_auth_sends_an_encoded_authorization_header() {
        let port = serve(|request| {
            let text = String::from_utf8_lossy(request);
            // Header names are normalized to lowercase on the wire; the
            // base64 value keeps its case.
            if text.to_ascii_lowercase().contains("authorization: basic ")
                && text.contains("dXNlcjpwYXNz")
            {
                http_response("200 OK", "text/plain", "authorized")
            } else {
                http_response("401 Unauthorized", "text/plain", "denied")
            }
        })
        .await;
        let (result, stored, _) = send(
            ManualRequest {
                url: format!("http://127.0.0.1:{port}/auth"),
                auth: AuthSpec::Basic {
                    username: "user".into(),
                    password: "pass".into(),
                },
                ..Default::default()
            },
            SendOptions::default(),
        )
        .await;
        assert_eq!(result.status, 200);
        assert!(
            stored
                .request
                .headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("authorization"))
        );
    }

    #[test]
    fn client_cache_reuses_and_evicts_by_option_profile() {
        // A private instance: the global cache is shared by concurrent
        // integration tests, so only local state is asserted here.
        let cache = ClientCache {
            clients: Mutex::new(Vec::new()),
        };
        let options = SendOptions::default();
        assert!(cache.get(&options).is_none());
        cache.insert(options.clone(), Client::new());
        assert!(cache.get(&options).is_some());

        for index in 0..(CLIENT_CACHE_CAP + 2) {
            let profile = SendOptions {
                timeout_ms: (1 + index as u64) * 1_000_000,
                ..Default::default()
            };
            cache.insert(profile.clone(), Client::new());
            assert!(cache.get(&profile).is_some());
        }
        let clients = cache.clients.lock().unwrap();
        assert!(clients.len() <= CLIENT_CACHE_CAP);
        assert!(
            clients.iter().any(|(key, _)| key.timeout_ms == 10_000_000),
            "the newest profile is kept"
        );
        assert!(
            !clients.iter().any(|(key, _)| key.timeout_ms == 30_000),
            "the default profile is evicted once the cache is full"
        );
        assert!(
            !clients.iter().any(|(key, _)| key.timeout_ms == 1_000_000),
            "the oldest profile is evicted once the cache is full"
        );
    }
}
