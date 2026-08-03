use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;
use uuid::Uuid;

pub const BODY_LIMIT: usize = 48 * 1024;
pub const HEADER_COUNT_LIMIT: usize = 100;
pub const HEADER_TOTAL_LIMIT: usize = 32 * 1024;
pub const HEADER_VALUE_LIMIT: usize = 8 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedBody {
    pub text: String,
    pub original_size: usize,
    pub truncated: bool,
}
impl CapturedBody {
    pub fn unavailable(original_size: Option<usize>) -> Self {
        Self {
            text: String::new(),
            original_size: original_size.unwrap_or_default(),
            truncated: true,
        }
    }
    pub fn from_bytes(source: &[u8], content_type: Option<&str>) -> Self {
        let original_size = source.len();
        let mut bytes = source[..source.len().min(BODY_LIMIT)].to_vec();
        if content_type.is_some_and(|v| v.to_ascii_lowercase().contains("json")) {
            if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                redact_json(&mut value);
                bytes = serde_json::to_vec(&value).unwrap_or_default();
            } else {
                let fragment = String::from_utf8_lossy(&bytes);
                let pattern = regex::Regex::new(
                    r#"(?i)(\"(?:authorization|proxy[-_]?authorization|password|passwd|secret|token|access[-_]?token|refresh[-_]?token|api[-_]?key|client[-_]?secret|session[-_]?(?:id|token)|cookie|set[-_]?cookie|otp|pin|private[-_]?key)\"\s*:\s*\")[^\"]*"#,
                )
                .expect("JSON secret regex");
                bytes = pattern
                    .replace_all(&fragment, "${1}<redacted>")
                    .as_bytes()
                    .to_vec();
            }
        } else {
            bytes = redact_text(&String::from_utf8_lossy(&bytes)).into_bytes();
        }
        bytes.truncate(BODY_LIMIT);
        Self {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            original_size,
            truncated: original_size > BODY_LIMIT,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: CapturedBody,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: CapturedBody,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpTransaction {
    pub id: Uuid,
    pub started_at_ms: i64,
    pub request: CapturedRequest,
    pub response: Option<CapturedResponse>,
}
pub fn secret(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxy_authorization"
            | "cookie"
            | "set_cookie"
            | "x_api_key"
            | "x_auth_token"
            | "password"
            | "passwd"
            | "secret"
            | "token"
            | "access_token"
            | "refresh_token"
            | "api_key"
            | "client_secret"
            | "session_id"
            | "session_token"
            | "otp"
            | "pin"
            | "private_key"
    )
}
pub fn headers(input: &hudsucker::hyper::HeaderMap) -> BTreeMap<String, String> {
    let mut retained = BTreeMap::new();
    let mut total = 0usize;
    if input.len() > HEADER_COUNT_LIMIT {
        return retained;
    }
    for (name, value) in input {
        let bytes = name.as_str().len().saturating_add(value.as_bytes().len());
        total = total.saturating_add(bytes);
        if value.as_bytes().len() > HEADER_VALUE_LIMIT || total > HEADER_TOTAL_LIMIT {
            return BTreeMap::new();
        }
        retained.insert(
            name.to_string(),
            if secret(name.as_str()) {
                "<redacted>".into()
            } else {
                value.to_str().unwrap_or("<binary>").into()
            },
        );
    }
    retained
}
pub fn redact_url(input: &str) -> String {
    let relative = !input.contains("://");
    let relative_path = if input.starts_with('/') {
        input.to_owned()
    } else {
        format!("/{input}")
    };
    let Ok(mut url) =
        Url::parse(input).or_else(|_| Url::parse(&format!("http://apiqa.invalid{relative_path}")))
    else {
        return input
            .split_once('?')
            .map_or_else(|| input.into(), |(path, _)| path.into());
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let pairs = url
        .query_pairs()
        .map(|(k, _)| (k.into_owned(), "<redacted>"))
        .collect::<Vec<_>>();
    url.query_pairs_mut().clear().extend_pairs(pairs);
    if relative {
        let mut output = url.path().to_owned();
        if let Some(query) = url.query() {
            output.push('?');
            output.push_str(query);
        }
        if let Some(fragment) = url.fragment() {
            output.push('#');
            output.push_str(fragment);
        }
        output
    } else {
        url.to_string()
    }
}
fn redact_text(input: &str) -> String {
    let jwt =
        regex::Regex::new(r"\beyJ[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+){2}\b").expect("JWT regex");
    let secrets = regex::Regex::new(
        r#"(?i)((?:authorization|proxy-authorization|password|passwd|secret|client[_-]?secret|token|access[_-]?token|refresh[_-]?token|api[_-]?key|session[_-]?(?:id|token)|cookie)\s*[:=]\s*[\"']?)([^\"'&;,\s}]+)"#,
    )
    .expect("text secret regex");
    secrets
        .replace_all(&jwt.replace_all(input, "[REDACTED_JWT]"), "${1}<redacted>")
        .into_owned()
}
fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if secret(k) {
                    *v = serde_json::Value::String("<redacted>".into())
                } else {
                    redact_json(v)
                }
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(redact_json),
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use hudsucker::hyper::header::{HeaderName, HeaderValue};
    #[test]
    fn redacts_json_and_bounds_body() {
        let source = format!(
            r#"{{"token":"secret","safe":"{}"}}"#,
            "x".repeat(BODY_LIMIT)
        );
        let b = CapturedBody::from_bytes(source.as_bytes(), Some("application/json"));
        assert!(!b.text.contains("secret"));
        assert!(b.truncated);
    }
    #[test]
    fn redacts_non_json_and_all_url_values() {
        let body = CapturedBody::from_bytes(
            b"password=hunter2&safe=value eyJabc.def.ghi",
            Some("application/x-www-form-urlencoded"),
        );
        assert!(!body.text.contains("hunter2"));
        assert!(!body.text.contains("eyJabc"));
        let url = redact_url("https://user:pass@example.test/path?safe=value&token=secret");
        assert!(!url.contains("user"));
        assert!(!url.contains("pass"));
        assert!(!url.contains("value"));
        assert!(!url.contains("secret"));
    }

    #[test]
    fn redacts_secret_aliases_in_complete_and_truncated_json() {
        let complete = CapturedBody::from_bytes(
            br#"{"passwd":"one","session_id":"two","secret":"three"}"#,
            Some("application/json"),
        );
        assert!(!complete.text.contains("one"));
        assert!(!complete.text.contains("two"));
        assert!(!complete.text.contains("three"));

        let malformed = CapturedBody::from_bytes(
            br#"{"cookie":"one","session-token":"two""#,
            Some("application/json"),
        );
        assert!(!malformed.text.contains("one"));
        assert!(!malformed.text.contains("two"));
    }
    #[test]
    fn redacts_origin_and_relative_uri_query_values() {
        assert_eq!(
            redact_url("/v1/items?safe=value&token=secret"),
            "/v1/items?safe=%3Credacted%3E&token=%3Credacted%3E"
        );
        assert_eq!(redact_url("items?safe=value"), "/items?safe=%3Credacted%3E");
    }
    #[test]
    fn rejects_entire_header_set_when_any_bound_is_exceeded() {
        let mut input = hudsucker::hyper::HeaderMap::new();
        input.insert("safe", HeaderValue::from_static("value"));
        assert_eq!(
            headers(&input).get("safe").map(String::as_str),
            Some("value")
        );
        input.insert(
            "large",
            HeaderValue::from_bytes(&vec![b'x'; HEADER_VALUE_LIMIT + 1]).unwrap(),
        );
        assert!(headers(&input).is_empty());

        let mut total = hudsucker::hyper::HeaderMap::new();
        for index in 0..5 {
            total.insert(
                HeaderName::from_bytes(format!("x-total-{index}").as_bytes()).unwrap(),
                HeaderValue::from_bytes(&vec![b'x'; HEADER_VALUE_LIMIT]).unwrap(),
            );
        }
        assert!(headers(&total).is_empty());

        let mut numerous = hudsucker::hyper::HeaderMap::new();
        for index in 0..=HEADER_COUNT_LIMIT {
            numerous.insert(
                HeaderName::from_bytes(format!("x-{index}").as_bytes()).unwrap(),
                HeaderValue::from_static("x"),
            );
        }
        assert!(headers(&numerous).is_empty());
    }
}
