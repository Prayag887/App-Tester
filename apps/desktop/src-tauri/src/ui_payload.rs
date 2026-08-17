//! Bounds body bytes crossing the native-to-WebView boundary.
//!
//! Complete captures remain in SQLite/native storage. The UI only renders a
//! short preview, so serializing larger byte arrays into JavaScript wastes
//! memory and can destabilize WebKit during long capture sessions.

use androidqa_core::{
    comparison::Difference,
    composer::model::SendResult,
    events::InspectorEvent,
    traffic::{BodyStorage, HttpTransaction},
};

pub const UI_DETAIL_PREVIEW_LIMIT: usize = 64 * 1024;
const UI_DETAIL_COLLECTION_LIMIT: usize = 200;
const UI_DETAIL_TEXT_LIMIT: usize = 8 * 1024;
const UI_SUMMARY_TEXT_LIMIT: usize = 1024;

fn cap_body(body: &mut BodyStorage, limit: usize) {
    match body {
        BodyStorage::Inline { bytes } if bytes.len() > limit => {
            let original_size = bytes.len() as u64;
            let preview = bytes[..limit].to_vec();
            *body = BodyStorage::Truncated {
                preview,
                original_size: Some(original_size),
            };
        }
        BodyStorage::Artifact { preview, .. } | BodyStorage::Truncated { preview, .. } => {
            preview.truncate(limit);
        }
        BodyStorage::Empty | BodyStorage::Inline { .. } | BodyStorage::Unavailable { .. } => {}
    }
}

fn truncate_string(value: &mut String, limit: usize) {
    if value.len() <= limit {
        return;
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

fn cap_difference(difference: &mut Difference, text_limit: usize) {
    if let Some(path) = difference.path.as_mut() {
        truncate_string(path, text_limit);
    }
    if let Some(previous) = difference.previous.as_mut() {
        truncate_string(&mut previous.0, text_limit);
    }
    if let Some(current) = difference.current.as_mut() {
        truncate_string(&mut current.0, text_limit);
    }
    truncate_string(&mut difference.explanation, text_limit);
}

fn cap_metadata(transaction: &mut HttpTransaction, collection_limit: usize, text_limit: usize) {
    for value in [
        &mut transaction.request.method,
        &mut transaction.request.scheme,
        &mut transaction.request.host,
        &mut transaction.request.path,
        &mut transaction.request.http_version,
    ] {
        truncate_string(value, text_limit);
    }
    transaction.request.query.truncate(collection_limit);
    for query in &mut transaction.request.query {
        truncate_string(&mut query.name, text_limit);
        truncate_string(&mut query.value, text_limit);
    }
    transaction.request.headers.truncate(collection_limit);
    for header in &mut transaction.request.headers {
        truncate_string(&mut header.name, text_limit);
        truncate_string(&mut header.value, text_limit);
    }
    if let Some(response) = transaction.response.as_mut() {
        response.headers.truncate(collection_limit);
        for header in &mut response.headers {
            truncate_string(&mut header.name, text_limit);
            truncate_string(&mut header.value, text_limit);
        }
        truncate_string(&mut response.http_version, text_limit);
        if let Some(reason) = response.reason.as_mut() {
            truncate_string(reason, text_limit);
        }
    }
    if let Some(comparison) = transaction.comparison.as_mut() {
        comparison.differences.truncate(collection_limit);
        comparison
            .differences
            .iter_mut()
            .for_each(|difference| cap_difference(difference, text_limit));
    }
    transaction.correlated_incidents.truncate(collection_limit);
}

fn cap_transaction(transaction: &mut HttpTransaction, body_limit: usize, text_limit: usize) {
    cap_body(&mut transaction.request.body, body_limit);
    if let Some(response) = transaction.response.as_mut() {
        cap_body(&mut response.body, body_limit);
    }
    if let Some(curl) = transaction.curl.as_mut() {
        truncate_string(&mut curl.compact, text_limit);
        truncate_string(&mut curl.multiline, text_limit);
    }
}

pub fn cap_transaction_summary(transaction: &mut HttpTransaction) {
    let changed_difference = transaction
        .comparison
        .as_ref()
        .and_then(|comparison| {
            comparison
                .differences
                .iter()
                .find(|difference| !difference.ignored)
        })
        .cloned();
    cap_transaction(transaction, 0, 0);
    cap_metadata(transaction, 0, UI_SUMMARY_TEXT_LIMIT);
    // One unignored difference preserves the Changed row classification.
    if let Some(comparison) = transaction.comparison.as_mut() {
        comparison.differences = changed_difference.into_iter().collect();
        comparison
            .differences
            .iter_mut()
            .for_each(|difference| cap_difference(difference, UI_SUMMARY_TEXT_LIMIT));
    }
}

pub fn cap_transaction_detail(transaction: &mut HttpTransaction) {
    cap_transaction(
        transaction,
        UI_DETAIL_PREVIEW_LIMIT,
        UI_DETAIL_PREVIEW_LIMIT,
    );
    cap_metadata(
        transaction,
        UI_DETAIL_COLLECTION_LIMIT,
        UI_DETAIL_TEXT_LIMIT,
    );
}

pub fn cap_event(event: &mut InspectorEvent) {
    match event {
        InspectorEvent::TransactionCreated(transaction)
        | InspectorEvent::TransactionUpdated(transaction)
        | InspectorEvent::TransactionCompleted(transaction) => cap_transaction_summary(transaction),
        InspectorEvent::ProxyStatusChanged(_)
        | InspectorEvent::SessionStatusChanged(_)
        | InspectorEvent::ComparisonCompleted { .. }
        | InspectorEvent::IncidentCreated(_)
        | InspectorEvent::IssueCreated(_)
        | InspectorEvent::DeviceStatusChanged(_) => {}
    }
}

pub fn cap_send_result(result: &mut SendResult) {
    cap_body(&mut result.body, UI_DETAIL_PREVIEW_LIMIT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_large_inline_bodies_to_bounded_previews() {
        let mut body = BodyStorage::Inline {
            bytes: vec![7; UI_DETAIL_PREVIEW_LIMIT + 10],
        };

        cap_body(&mut body, UI_DETAIL_PREVIEW_LIMIT);

        assert!(matches!(
            body,
            BodyStorage::Truncated {
                preview,
                original_size: Some(size),
            } if preview.len() == UI_DETAIL_PREVIEW_LIMIT
                && size == (UI_DETAIL_PREVIEW_LIMIT + 10) as u64
        ));
    }

    #[test]
    fn trims_existing_previews_without_losing_the_original_size() {
        let mut body = BodyStorage::Truncated {
            preview: vec![9; UI_DETAIL_PREVIEW_LIMIT + 10],
            original_size: Some(5_000_000),
        };

        cap_body(&mut body, UI_DETAIL_PREVIEW_LIMIT);

        assert!(matches!(
            body,
            BodyStorage::Truncated {
                preview,
                original_size: Some(5_000_000),
            } if preview.len() == UI_DETAIL_PREVIEW_LIMIT
        ));
    }

    #[test]
    fn summaries_never_carry_body_or_curl_payloads() {
        let mut transaction: HttpTransaction = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(), "session_id": uuid::Uuid::new_v4(), "connection_id": uuid::Uuid::new_v4(),
            "request": {"method":"POST","scheme":"https","host":"api.test","path":"/v1","query":[{"name":"q","value":"value"}],"headers":[{"name":"x-large","value":"value"}],"body":{"storage":"inline","bytes":[1,2,3]},"http_version":"HTTP_1_1"},
            "response": {"status":400,"headers":[{"name":"x-large","value":"value"}],"body":{"storage":"inline","bytes":[4,5,6]},"decoded_size":3,"encoded_size":3,"http_version":"HTTP_1_1"},
            "state":"response_complete","timing":{"request_started_ms":0},"curl":{"compact":"curl data","multiline":"curl data","redacted":true},
            "capture_quality":"complete","comparison":{"baseline_transaction_id":null,"compatibility":"incompatible","differences":[{"kind":"status_changed","path":"$.status","previous":"200","current":"400","severity":"critical","ignored":false,"explanation":"changed"}]},"correlated_incidents":[],"created_at":"2026-07-24T00:00:00Z","updated_at":"2026-07-24T00:00:00Z"
        })).unwrap();

        cap_transaction_summary(&mut transaction);

        assert_eq!(transaction.request.body.bytes(), Some([].as_slice()));
        assert_eq!(
            transaction
                .response
                .as_ref()
                .and_then(|response| response.body.bytes()),
            Some([].as_slice())
        );
        assert_eq!(
            transaction
                .curl
                .as_ref()
                .map(|curl| curl.multiline.as_str()),
            Some("")
        );
        assert!(transaction.request.query.is_empty());
        assert!(transaction.request.headers.is_empty());
        assert!(transaction.response.as_ref().unwrap().headers.is_empty());
        assert_eq!(
            transaction
                .comparison
                .as_ref()
                .map(|comparison| comparison.differences.len()),
            Some(1)
        );
    }
}
