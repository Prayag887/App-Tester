use std::collections::HashMap;

use reqwest::{Client, Method};
use thiserror::Error;

use super::variables::substitute;
use crate::{
    ApiKeyLocation, ApiRequest, Auth, BodyKind, KeyValue, ResponseSnapshot,
    model::MAX_CAPTURED_BODY_SIZE,
};

#[derive(Debug, Error)]
pub(super) enum RequestError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("transport error: {0}")]
    Transport(#[source] reqwest::Error),
}

pub(super) async fn send(
    client: &Client,
    request: &ApiRequest,
    variables: &HashMap<String, String>,
) -> Result<ResponseSnapshot, RequestError> {
    let url = substitute(&request.url, variables);
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(RequestError::Invalid(
            "only HTTP and HTTPS URLs are allowed".into(),
        ));
    }
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|error| RequestError::Invalid(format!("invalid HTTP method: {error}")))?;
    let mut builder = client.request(method, &url);
    for header in request.headers.iter().filter(|header| header.enabled) {
        builder = builder.header(&header.key, substitute(&header.value, variables));
    }
    for query in request.query.iter().filter(|query| query.enabled) {
        builder = builder.query(&[(query.key.as_str(), substitute(&query.value, variables))]);
    }
    match &request.auth {
        Auth::None => {}
        Auth::Basic { username, password } => {
            builder = builder.basic_auth(
                substitute(username, variables),
                Some(substitute(password, variables)),
            );
        }
        Auth::Bearer { token } => builder = builder.bearer_auth(substitute(token, variables)),
        Auth::ApiKey {
            key,
            value,
            location,
        } => match location {
            ApiKeyLocation::Header => builder = builder.header(key, substitute(value, variables)),
            ApiKeyLocation::Query => {
                builder = builder.query(&[(key, substitute(value, variables))])
            }
        },
    }
    builder = apply_body(builder, request, variables)?;
    capture(builder.send().await.map_err(classify_reqwest_error)?).await
}

fn apply_body(
    mut builder: reqwest::RequestBuilder,
    request: &ApiRequest,
    variables: &HashMap<String, String>,
) -> Result<reqwest::RequestBuilder, RequestError> {
    match request.body_kind {
        BodyKind::Raw => {
            builder = builder.body(substitute(
                request.body.as_deref().unwrap_or_default(),
                variables,
            ));
        }
        BodyKind::UrlEncoded => {
            let values: Vec<KeyValue> = parse_form_values(request)?;
            let form = values
                .into_iter()
                .filter(|value| value.enabled)
                .map(|value| (value.key, substitute(&value.value, variables)))
                .collect::<Vec<_>>();
            builder = builder.form(&form);
        }
        BodyKind::FormData => {
            let values: Vec<KeyValue> = parse_form_values(request)?;
            let mut form = reqwest::multipart::Form::new();
            for value in values.into_iter().filter(|value| value.enabled) {
                form = form.text(value.key, substitute(&value.value, variables));
            }
            builder = builder.multipart(form);
        }
        BodyKind::None => {}
    }
    Ok(builder)
}

fn parse_form_values(request: &ApiRequest) -> Result<Vec<KeyValue>, RequestError> {
    serde_json::from_str(request.body.as_deref().unwrap_or("[]"))
        .map_err(|error| RequestError::Invalid(format!("invalid form body: {error}")))
}

fn classify_reqwest_error(error: reqwest::Error) -> RequestError {
    if error.is_builder() {
        RequestError::Invalid(error.to_string())
    } else {
        RequestError::Transport(error)
    }
}

async fn capture(mut response: reqwest::Response) -> Result<ResponseSnapshot, RequestError> {
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| KeyValue {
            key: key.to_string(),
            value: if matches!(
                key.as_str(),
                "set-cookie" | "authorization" | "proxy-authorization"
            ) {
                "[REDACTED]".into()
            } else {
                value.to_str().unwrap_or("[binary]").to_string()
            },
            enabled: true,
        })
        .collect();
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or_default()
            .min(MAX_CAPTURED_BODY_SIZE as u64) as usize,
    );
    let mut body_size = 0_u64;
    while let Some(chunk) = response.chunk().await.map_err(RequestError::Transport)? {
        body_size = body_size.saturating_add(chunk.len() as u64);
        let remaining = MAX_CAPTURED_BODY_SIZE.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    let truncated = body_size > bytes.len() as u64;
    let body = bounded_utf8(bytes);
    Ok(ResponseSnapshot {
        status,
        headers,
        content_type,
        body,
        body_hash: None,
        body_size,
        duration_ms: 0,
        truncated,
    })
}

fn bounded_utf8(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(body) => body,
        Err(error) => {
            let mut body = String::with_capacity(MAX_CAPTURED_BODY_SIZE);
            for chunk in error.as_bytes().utf8_chunks() {
                push_bounded(&mut body, chunk.valid());
                if !chunk.invalid().is_empty()
                    && body.len() + '\u{fffd}'.len_utf8() <= MAX_CAPTURED_BODY_SIZE
                {
                    body.push('\u{fffd}');
                }
                if body.len() == MAX_CAPTURED_BODY_SIZE {
                    break;
                }
            }
            body
        }
    }
}

fn push_bounded(target: &mut String, value: &str) {
    let remaining = MAX_CAPTURED_BODY_SIZE.saturating_sub(target.len());
    let mut end = value.len().min(remaining);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    target.push_str(&value[..end]);
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
