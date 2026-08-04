//! Secret redaction for headers, query parameters, JSON payloads, and URLs.

use super::model::HeaderEntry;

pub const SECRET_NAMES: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "x-auth-token",
    "x-access-token",
    "x-refresh-token",
    "password",
    "passcode",
    "secret",
    "token",
    "access_token",
    "refresh_token",
    "api_key",
    "apikey",
    "session",
    "session_id",
    "otp",
    "pin",
    "private_key",
    "client_secret",
];

pub fn is_secret(name: &str) -> bool {
    SECRET_NAMES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

pub fn redact_headers(headers: &[HeaderEntry]) -> Vec<HeaderEntry> {
    headers
        .iter()
        .map(|header| HeaderEntry {
            name: header.name.clone(),
            value: if is_secret(&header.name) {
                "<redacted>".into()
            } else {
                header.value.clone()
            },
        })
        .collect()
}

pub fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if is_secret(key) {
                    *value = serde_json::Value::String("<redacted>".into());
                } else {
                    redact_json(value);
                }
            }
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(redact_json),
        _ => {}
    }
}

pub fn redact_url(input: &str) -> String {
    let Ok(mut url) = url::Url::parse(input) else {
        return input.to_owned();
    };
    let pairs = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if is_secret(&key) {
                "<redacted>".into()
            } else {
                value
            };
            (key.into_owned(), value.into_owned())
        })
        .collect::<Vec<_>>();
    url.query_pairs_mut().clear().extend_pairs(pairs);
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_headers_and_nested_json() {
        assert_eq!(
            redact_headers(&[HeaderEntry {
                name: "Authorization".into(),
                value: "Bearer x".into()
            }])[0]
                .value,
            "<redacted>"
        );
        let mut value = serde_json::json!({"profile":{"password":"x","name":"safe"}});
        redact_json(&mut value);
        assert_eq!(value["profile"]["password"], "<redacted>");
    }

    #[test]
    fn recognizes_secret_names_case_insensitively() {
        assert!(is_secret("Authorization"));
        assert!(is_secret("X-API-KEY"));
        assert!(is_secret("access_token"));
        assert!(!is_secret("x-request-id"));
        assert!(!is_secret("content-type"));
    }

    #[test]
    fn redacts_secret_query_values_and_leaves_public_values() {
        assert!(redact_url("https://x.test/?token=abc&q=1").contains("token=%3Credacted%3E"));
        assert!(redact_url("https://x.test/?token=abc&q=1").contains("q=1"));
    }
}
