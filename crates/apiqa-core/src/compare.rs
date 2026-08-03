use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::Value;

use crate::{Difference, DifferenceKind, ResponseComparison, ResponseSnapshot};

#[derive(Debug, Clone, Default)]
pub struct ComparisonOptions {
    pub ignored_json_paths: HashSet<String>,
    pub ignored_headers: HashSet<String>,
}

pub fn compare_responses(
    baseline: &ResponseSnapshot,
    current: &ResponseSnapshot,
    options: &ComparisonOptions,
) -> ResponseComparison {
    let mut differences = Vec::new();

    if baseline.status != current.status {
        differences.push(Difference {
            kind: DifferenceKind::Status,
            path: "$status".into(),
            baseline: Some(Value::from(baseline.status)),
            current: Some(Value::from(current.status)),
            message: format!(
                "Status changed from {} to {}",
                baseline.status, current.status
            ),
        });
    }

    compare_headers(baseline, current, options, &mut differences);

    match (
        serde_json::from_str::<Value>(&baseline.body),
        serde_json::from_str::<Value>(&current.body),
    ) {
        (Ok(left), Ok(right)) => compare_json("$", &left, &right, options, &mut differences),
        _ if baseline.body != current.body => differences.push(Difference {
            kind: DifferenceKind::TextChanged,
            path: "$body".into(),
            baseline: Some(Value::String(baseline.body.clone())),
            current: Some(Value::String(current.body.clone())),
            message: "Response body changed".into(),
        }),
        _ => {}
    }

    if current.duration_ms >= baseline.duration_ms.saturating_add(500)
        && current.duration_ms >= baseline.duration_ms.saturating_mul(3) / 2
    {
        differences.push(Difference {
            kind: DifferenceKind::Timing,
            path: "$timing.total_ms".into(),
            baseline: Some(Value::from(baseline.duration_ms)),
            current: Some(Value::from(current.duration_ms)),
            message: format!(
                "Response slowed from {} ms to {} ms",
                baseline.duration_ms, current.duration_ms
            ),
        });
    }

    ResponseComparison {
        changed: !differences.is_empty(),
        differences,
    }
}

fn compare_headers(
    baseline: &ResponseSnapshot,
    current: &ResponseSnapshot,
    options: &ComparisonOptions,
    differences: &mut Vec<Difference>,
) {
    let ignored_headers = options
        .ignored_headers
        .iter()
        .map(|header| header.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let normalize = |headers: &[crate::KeyValue]| {
        let mut normalized = BTreeMap::<String, Vec<String>>::new();
        for header in headers {
            let key = header.key.trim().to_ascii_lowercase();
            if !ignored_headers.contains(&key) {
                normalized
                    .entry(key)
                    .or_default()
                    .push(header.value.clone());
            }
        }
        normalized
    };
    let left = normalize(&baseline.headers);
    let right = normalize(&current.headers);
    for key in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
        if left.get(key) != right.get(key) {
            differences.push(Difference {
                kind: DifferenceKind::Header,
                path: format!("$headers.{key}"),
                baseline: left.get(key).map(|values| header_values(values)),
                current: right.get(key).map(|values| header_values(values)),
                message: format!("Header {key} changed"),
            });
        }
    }
}

fn header_values(values: &[String]) -> Value {
    if values.len() == 1 {
        Value::String(values[0].clone())
    } else {
        Value::Array(values.iter().cloned().map(Value::String).collect())
    }
}

fn compare_json(
    path: &str,
    baseline: &Value,
    current: &Value,
    options: &ComparisonOptions,
    differences: &mut Vec<Difference>,
) {
    if options.ignored_json_paths.contains(path) {
        return;
    }
    match (baseline, current) {
        (Value::Object(left), Value::Object(right)) => {
            let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                let child = format!("{path}.{key}");
                match (left.get(key), right.get(key)) {
                    (Some(a), Some(b)) => compare_json(&child, a, b, options, differences),
                    (Some(a), None) => differences.push(Difference {
                        kind: DifferenceKind::Removed,
                        path: child,
                        baseline: Some(a.clone()),
                        current: None,
                        message: format!("Field {key} was removed"),
                    }),
                    (None, Some(b)) => differences.push(Difference {
                        kind: DifferenceKind::Added,
                        path: child,
                        baseline: None,
                        current: Some(b.clone()),
                        message: format!("Field {key} was added"),
                    }),
                    _ => {}
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                let child = format!("{path}[{index}]");
                match (left.get(index), right.get(index)) {
                    (Some(a), Some(b)) => compare_json(&child, a, b, options, differences),
                    (Some(a), None) => differences.push(Difference {
                        kind: DifferenceKind::Removed,
                        path: child,
                        baseline: Some(a.clone()),
                        current: None,
                        message: "Array item was removed".into(),
                    }),
                    (None, Some(b)) => differences.push(Difference {
                        kind: DifferenceKind::Added,
                        path: child,
                        baseline: None,
                        current: Some(b.clone()),
                        message: "Array item was added".into(),
                    }),
                    _ => {}
                }
            }
        }
        _ if std::mem::discriminant(baseline) != std::mem::discriminant(current) => {
            differences.push(Difference {
                kind: DifferenceKind::TypeChanged,
                path: path.into(),
                baseline: Some(baseline.clone()),
                current: Some(current.clone()),
                message: "Value type changed".into(),
            });
        }
        _ if baseline != current => differences.push(Difference {
            kind: DifferenceKind::ValueChanged,
            path: path.into(),
            baseline: Some(baseline.clone()),
            current: Some(current.clone()),
            message: "Value changed".into(),
        }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(key: &str, value: &str) -> crate::KeyValue {
        crate::KeyValue {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    fn response(body: &str) -> ResponseSnapshot {
        ResponseSnapshot {
            status: 200,
            headers: vec![],
            content_type: Some("application/json".into()),
            body: body.into(),
            body_hash: None,
            body_size: body.len() as u64,
            duration_ms: 10,
            truncated: false,
        }
    }

    #[test]
    fn ignores_json_key_order() {
        let result = compare_responses(
            &response(r#"{"a":1,"b":2}"#),
            &response(r#"{"b":2,"a":1}"#),
            &ComparisonOptions::default(),
        );
        assert!(!result.changed);
    }

    #[test]
    fn reports_nested_value_change() {
        let result = compare_responses(
            &response(r#"{"user":{"name":"Ada"}}"#),
            &response(r#"{"user":{"name":"Grace"}}"#),
            &ComparisonOptions::default(),
        );
        assert_eq!(result.differences[0].path, "$.user.name");
    }

    #[test]
    fn preserves_repeated_headers_and_normalizes_ignored_names() {
        let mut baseline = response("{}");
        baseline.headers = vec![
            header("Set-Cookie", "a=1"),
            header("set-cookie", "b=2"),
            header("X-Request-Id", "old"),
        ];
        let mut current = response("{}");
        current.headers = vec![
            header("SET-COOKIE", "a=1"),
            header("Set-Cookie", "b=3"),
            header("x-request-id", "new"),
        ];
        let options = ComparisonOptions {
            ignored_headers: HashSet::from([" X-REQUEST-ID ".into()]),
            ..ComparisonOptions::default()
        };

        let result = compare_responses(&baseline, &current, &options);

        assert_eq!(result.differences.len(), 1);
        assert_eq!(result.differences[0].path, "$headers.set-cookie");
        assert_eq!(
            result.differences[0].baseline,
            Some(serde_json::json!(["a=1", "b=2"]))
        );
        assert_eq!(
            result.differences[0].current,
            Some(serde_json::json!(["a=1", "b=3"]))
        );
    }

    #[test]
    fn reports_differences_in_deterministic_order() {
        let mut baseline = response(r#"{"z":1,"a":1}"#);
        baseline.headers = vec![header("Z-Header", "old")];
        let mut current = response(r#"{"z":2,"a":2}"#);
        current.headers = vec![header("Z-Header", "new"), header("A-Header", "added")];

        let result = compare_responses(&baseline, &current, &ComparisonOptions::default());
        let paths = result
            .differences
            .iter()
            .map(|difference| difference.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            ["$headers.a-header", "$headers.z-header", "$.a", "$.z"]
        );
    }
}
