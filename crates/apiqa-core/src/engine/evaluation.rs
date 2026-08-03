use crate::{
    ApiRequest, AssertionResult, ExtractedValue, ExtractionRule, ResponseAssertion,
    ResponseSnapshot,
};

pub(super) fn assertions(
    request: &ApiRequest,
    response: &ResponseSnapshot,
) -> Vec<AssertionResult> {
    request
        .assertions
        .iter()
        .map(|assertion| match assertion {
            ResponseAssertion::StatusEquals { expected, name } => AssertionResult {
                name: name.clone(),
                passed: response.status == *expected,
                message: if response.status == *expected {
                    format!("Status was {expected}")
                } else {
                    format!("Expected status {expected}, received {}", response.status)
                },
            },
        })
        .collect()
}

pub(super) fn extractions(
    request: &ApiRequest,
    response: &ResponseSnapshot,
) -> Vec<ExtractedValue> {
    request
        .extractions
        .iter()
        .filter_map(|rule| match rule {
            ExtractionRule::JsonPath { name, path } => {
                let body: serde_json::Value = serde_json::from_str(&response.body).ok()?;
                let value = json_path(&body, path)?;
                Some(ExtractedValue {
                    name: name.clone(),
                    value: value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string()),
                    source: path.clone(),
                })
            }
            ExtractionRule::Header { name, header } => response
                .headers
                .iter()
                .find(|item| item.key.eq_ignore_ascii_case(header))
                .map(|item| ExtractedValue {
                    name: name.clone(),
                    value: item.value.clone(),
                    source: format!("header:{header}"),
                }),
        })
        .collect()
}

fn json_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.trim_start_matches("$.").split('.') {
        if segment.is_empty() {
            continue;
        }
        current = current.get(segment)?;
    }
    Some(current)
}
