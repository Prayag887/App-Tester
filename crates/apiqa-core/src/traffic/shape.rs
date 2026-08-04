//! Endpoint path normalization and JSON request-shape hashing.

use std::{collections::BTreeMap, sync::OnceLock};

use super::model::BodyStorage;

/// The literal regexes used for path normalization, compiled once per
/// process instead of on every request.
#[allow(clippy::expect_used)] // infallible: patterns are source literals exercised by tests
fn static_regexes() -> &'static [regex::Regex; 2] {
    static REGEXES: OnceLock<[regex::Regex; 2]> = OnceLock::new();
    REGEXES.get_or_init(|| {
        [
            regex::Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f-]{27}$").expect("valid uuid regex"),
            regex::Regex::new(r"(?i)^[0-9a-f]{16,}$").expect("valid hex regex"),
        ]
    })
}

pub fn normalize_path(path: &str) -> String {
    let regexes = static_regexes();
    path.split('/')
        .map(|segment| {
            if segment.parse::<u64>().is_ok()
                || regexes[0].is_match(segment)
                || regexes[1].is_match(segment)
            {
                "{id}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn request_shape(body: &BodyStorage) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body.bytes()?).ok()?;
    let object = value.as_object()?;
    let shape: BTreeMap<_, _> = object
        .iter()
        .map(|(key, value)| (key, json_type(value)))
        .collect();
    Some(
        blake3::hash(serde_json::to_string(&shape).ok()?.as_bytes())
            .to_hex()
            .to_string(),
    )
}

fn json_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_routes() {
        assert_eq!(normalize_path("/users/847"), "/users/{id}");
        assert_eq!(normalize_path("/users/me"), "/users/me");
        assert_eq!(
            normalize_path("/orders/1f2e3d4c5b6a7988/details"),
            "/orders/{id}/details"
        );
        assert_eq!(
            normalize_path("/items/4c2f8b8e-1d1e-4f3e-9b6a-2f3c4d5e6f7a"),
            "/items/{id}"
        );
    }

    #[test]
    fn hashes_json_request_shape_ignoring_values() {
        let shape = |value: serde_json::Value| {
            request_shape(&BodyStorage::Inline {
                bytes: serde_json::to_vec(&value).unwrap(),
            })
        };
        let first = shape(serde_json::json!({"user":{"id":1,"name":"a"},"items":[1,2]}));
        let second = shape(serde_json::json!({"user":{"id":99,"name":"b"},"items":[]}));
        let different = shape(serde_json::json!({"user":{"id":1,"name":"a"},"extra":true}));
        assert_eq!(first, second);
        assert_ne!(first, different);
        assert!(shape(serde_json::json!([1, 2, 3])).is_none());
        assert!(request_shape(&BodyStorage::Empty).is_none());
    }
}
