//! cURL command generation from captured requests.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use super::model::{CapturedRequest, GeneratedCurl, HeaderEntry};
use super::redact::{is_secret, redact_headers};

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub fn generate_curl(request: &CapturedRequest) -> GeneratedCurl {
    generate_curl_with_headers(request, redact_headers(&request.headers))
}

/// Produces the locally displayed cURL command with the original Authorization
/// value, while retaining redaction for every other sensitive header.
///
/// The resulting command is intentionally omitted from portable exports.
pub fn generate_local_curl_with_authorization(
    request: &CapturedRequest,
    original_headers: &[HeaderEntry],
) -> GeneratedCurl {
    let headers = request
        .headers
        .iter()
        .map(|header| {
            let value = if matches!(
                header.name.to_ascii_lowercase().as_str(),
                "authorization" | "proxy-authorization"
            ) {
                original_headers
                    .iter()
                    .find(|original| original.name.eq_ignore_ascii_case(&header.name))
                    .map(|original| original.value.clone())
                    .unwrap_or_else(|| header.value.clone())
            } else if is_secret(&header.name) {
                "<redacted>".into()
            } else {
                header.value.clone()
            };
            HeaderEntry {
                name: header.name.clone(),
                value,
            }
        })
        .collect();
    generate_curl_with_headers(request, headers)
}

fn generate_curl_with_headers(
    request: &CapturedRequest,
    headers: Vec<HeaderEntry>,
) -> GeneratedCurl {
    let mut url = format!(
        "{}://{}{}{}",
        request.scheme,
        request.host,
        request
            .port
            .map(|port| format!(":{port}"))
            .unwrap_or_default(),
        request.path
    );
    if !request.query.is_empty() {
        let query = request
            .query
            .iter()
            .map(|entry| {
                format!(
                    "{}={}",
                    url::form_urlencoded::byte_serialize(entry.name.as_bytes()).collect::<String>(),
                    url::form_urlencoded::byte_serialize(if is_secret(&entry.name) {
                        b"<redacted>"
                    } else {
                        entry.value.as_bytes()
                    })
                    .collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&query);
    }
    let ignored = [
        "host",
        "content-length",
        "connection",
        "proxy-connection",
        "accept-encoding",
    ];
    let headers = headers
        .into_iter()
        .filter(|header| {
            !ignored
                .iter()
                .any(|name| header.name.eq_ignore_ascii_case(name))
        })
        .collect::<Vec<_>>();
    let mut args = vec![
        "curl".to_owned(),
        "--request".into(),
        request.method.clone(),
        "--url".into(),
        shell_quote(&url),
    ];
    for header in headers {
        args.extend([
            "--header".into(),
            shell_quote(&format!("{}: {}", header.name, header.value)),
        ]);
    }
    let encoded_body = request
        .body
        .bytes()
        .filter(|bytes| !bytes.is_empty())
        .and_then(|body| {
            if body_needs_base64(request, body) {
                args.extend(["--data-binary".into(), "@-".into()]);
                Some(BASE64.encode(body))
            } else {
                let body = std::str::from_utf8(body).expect("text body was validated as UTF-8");
                args.extend(["--data-raw".into(), shell_quote(body)]);
                None
            }
        });
    let curl_compact = args.join(" ");
    let curl_multiline = args
        .chunks(2)
        .enumerate()
        .map(|(index, chunk)| {
            let line = chunk.join(" ");
            if index == 0 {
                line
            } else {
                format!("  {line}")
            }
        })
        .collect::<Vec<_>>()
        .join(" \\\n");
    let (compact, multiline) = if let Some(encoded) = encoded_body {
        let decoder = format!(
            "printf '%s' {} | openssl base64 -d -A | ",
            shell_quote(&encoded)
        );
        (
            format!("{decoder}{curl_compact}"),
            format!("{decoder}{curl_multiline}"),
        )
    } else {
        (curl_compact, curl_multiline)
    };
    GeneratedCurl {
        compact,
        multiline,
        redacted: true,
    }
}

fn body_needs_base64(request: &CapturedRequest, body: &[u8]) -> bool {
    request.content_type.as_deref().is_some_and(|content_type| {
        content_type
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("multipart/")
    }) || std::str::from_utf8(body).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::{BodyStorage, CapturedRequest};

    #[test]
    fn escapes_curl_and_normalizes_routes() {
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn local_curl_keeps_authorization_but_redacts_other_secrets() {
        let request = CapturedRequest {
            method: "GET".into(),
            scheme: "https".into(),
            host: "example.com".into(),
            port: None,
            path: "/profile".into(),
            query: vec![],
            headers: redact_headers(&[
                HeaderEntry {
                    name: "Authorization".into(),
                    value: "Bearer original-token".into(),
                },
                HeaderEntry {
                    name: "X-Api-Key".into(),
                    value: "private-key".into(),
                },
            ]),
            body: BodyStorage::Empty,
            content_type: None,
            http_version: "HTTP/1.1".into(),
        };
        let curl = generate_local_curl_with_authorization(
            &request,
            &[
                HeaderEntry {
                    name: "Authorization".into(),
                    value: "Bearer original-token".into(),
                },
                HeaderEntry {
                    name: "X-Api-Key".into(),
                    value: "private-key".into(),
                },
            ],
        );
        assert!(
            curl.compact
                .contains("Authorization: Bearer original-token")
        );
        assert!(curl.compact.contains("X-Api-Key: <redacted>"));
        assert!(!curl.compact.contains("private-key"));
    }

    #[test]
    fn redacts_secret_query_values_in_generated_url() {
        let request = CapturedRequest {
            method: "GET".into(),
            scheme: "https".into(),
            host: "example.com".into(),
            port: None,
            path: "/search".into(),
            query: vec![
                crate::traffic::QueryParameter {
                    name: "token".into(),
                    value: "s3cret".into(),
                },
                crate::traffic::QueryParameter {
                    name: "q".into(),
                    value: "hello".into(),
                },
            ],
            headers: vec![],
            body: BodyStorage::Empty,
            content_type: None,
            http_version: "HTTP/1.1".into(),
        };
        let curl = generate_curl(&request);
        assert!(curl.compact.contains("token=%3Credacted%3E"));
        assert!(curl.compact.contains("q=hello"));
        assert!(!curl.compact.contains("s3cret"));
    }

    #[test]
    fn omits_hop_by_hop_headers_from_generated_curl() {
        let request = CapturedRequest {
            method: "POST".into(),
            scheme: "http".into(),
            host: "example.com".into(),
            port: None,
            path: "/v1/items".into(),
            query: vec![],
            headers: vec![
                HeaderEntry {
                    name: "host".into(),
                    value: "example.com".into(),
                },
                HeaderEntry {
                    name: "content-length".into(),
                    value: "5".into(),
                },
                HeaderEntry {
                    name: "x-trace-id".into(),
                    value: "abc".into(),
                },
            ],
            body: BodyStorage::Empty,
            content_type: None,
            http_version: "HTTP/1.1".into(),
        };
        let curl = generate_curl(&request);
        assert!(!curl.compact.contains("content-length"));
        assert!(curl.compact.contains("x-trace-id: abc"));
    }

    #[test]
    fn base64_encodes_multipart_bodies_without_lossy_utf8_conversion() {
        let body = b"--boundary\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\n\x00\xff\x80\r\n--boundary--\r\n";
        let request = CapturedRequest {
            method: "POST".into(),
            scheme: "https".into(),
            host: "example.com".into(),
            port: None,
            path: "/upload".into(),
            query: vec![],
            headers: vec![HeaderEntry {
                name: "Content-Type".into(),
                value: "multipart/form-data; boundary=boundary".into(),
            }],
            body: BodyStorage::Inline {
                bytes: body.to_vec(),
            },
            content_type: Some("multipart/form-data; boundary=boundary".into()),
            http_version: "HTTP/1.1".into(),
        };

        let curl = generate_curl(&request);

        assert!(curl.compact.contains(&BASE64.encode(body)));
        assert!(curl.compact.contains("openssl base64 -d -A"));
        assert!(curl.compact.contains("--data-binary @-"));
        assert!(!curl.compact.contains('\u{fffd}'));
    }
}
