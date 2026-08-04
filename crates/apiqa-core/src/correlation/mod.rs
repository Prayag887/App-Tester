use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrelationConfidence {
    Confirmed,
    High,
    Medium,
    Low,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionWindow {
    pub id: Uuid,
    pub session_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ended_at: Option<OffsetDateTime>,
    pub foreground_package: Option<String>,
    pub foreground_activity: Option<String>,
    pub screen_label: Option<String>,
    pub trigger: InteractionTrigger,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionTrigger {
    ActivityChanged,
    HierarchyChanged,
    RequestBurst,
    UserMarker,
    ProcessChanged,
}

pub fn correlate(
    transaction: &crate::traffic::HttpTransaction,
    incident: &crate::diagnostics::LogIncident,
    target_package: &str,
) -> CorrelationConfidence {
    let delta = (incident.occurred_at - transaction.updated_at)
        .whole_milliseconds()
        .unsigned_abs();
    let endpoint_mentioned = transaction
        .endpoint_identity
        .as_ref()
        .is_some_and(|endpoint| incident.message.contains(&endpoint.path_template));
    let app_frame = incident
        .first_app_frame
        .as_ref()
        .is_some_and(|frame| frame.contains(target_package));
    if delta <= 1500 && endpoint_mentioned && app_frame {
        CorrelationConfidence::Confirmed
    } else if delta <= 2500 && app_frame {
        CorrelationConfidence::High
    } else if delta <= 5000 {
        CorrelationConfidence::Medium
    } else {
        CorrelationConfidence::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{IncidentCategory, LogIncident};
    use crate::traffic::HttpTransaction;
    use time::OffsetDateTime;

    fn transaction(updated_at: OffsetDateTime) -> HttpTransaction {
        let updated_at = updated_at
            .format(&time::format_description::well_known::Rfc3339)
            .expect("RFC3339 timestamp");
        serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(), "session_id": Uuid::new_v4(), "connection_id": Uuid::new_v4(),
            "request": {"method":"GET","scheme":"https","host":"api.test","path":"/v1/items","query":[],"headers":[],"body":{"storage":"empty"},"http_version":"HTTP_1_1"},
            "state": "response_complete", "timing": {"request_started_ms": 0},
            "endpoint_identity": {"method":"GET","host":"api.test","path_template":"/v1/{id}","content_type":null,"request_shape":null},
            "capture_quality": "complete", "correlated_incidents": [],
            "created_at": "2026-07-24T00:00:00Z", "updated_at": updated_at
        }))
        .unwrap()
    }

    fn incident(
        occurred_at: OffsetDateTime,
        message: &str,
        first_app_frame: Option<&str>,
    ) -> LogIncident {
        LogIncident {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            category: IncidentCategory::Error,
            signature: "sig".into(),
            title: "title".into(),
            message: message.into(),
            summary: "summary".into(),
            root_cause: None,
            first_app_frame: first_app_frame.map(str::to_owned),
            foreground_activity: None,
            where_occurred: "where".into(),
            how_occurred: "how".into(),
            likely_cause: "cause".into(),
            reproduction_steps: vec![],
            first_occurred_at: occurred_at,
            occurred_at,
            lines: vec![],
            occurrence_count: 1,
        }
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }

    #[test]
    fn confirms_when_close_in_time_endpoint_and_app_frame_align() {
        let tx = transaction(now());
        let issue = incident(now(), "failed to load /v1/{id}", Some("com.example.app"));
        assert_eq!(
            correlate(&tx, &issue, "com.example.app"),
            CorrelationConfidence::Confirmed
        );
    }

    #[test]
    fn high_when_app_frame_aligns_without_endpoint_mention() {
        let tx = transaction(now());
        let issue = incident(now(), "network change detected", Some("com.example.app"));
        assert_eq!(
            correlate(&tx, &issue, "com.example.app"),
            CorrelationConfidence::High
        );
    }

    #[test]
    fn medium_when_only_close_in_time() {
        let tx = transaction(now());
        let issue = incident(now(), "unrelated system message", None);
        assert_eq!(
            correlate(&tx, &issue, "com.example.app"),
            CorrelationConfidence::Medium
        );
    }

    #[test]
    fn low_when_far_apart_in_time() {
        let tx = transaction(now());
        let stale = now() - time::Duration::seconds(30);
        let issue = incident(stale, "failed to load /v1/{id}", Some("com.example.app"));
        assert_eq!(
            correlate(&tx, &issue, "com.example.app"),
            CorrelationConfidence::Low
        );
    }

    #[test]
    fn app_frame_must_mention_the_target_package() {
        let tx = transaction(now());
        let issue = incident(now(), "failed to load /v1/{id}", Some("com.other.app"));
        // Endpoint is mentioned and timing is tight, but the frame belongs to
        // another package: the correlation stays at the time-based level.
        assert_eq!(
            correlate(&tx, &issue, "com.example.app"),
            CorrelationConfidence::Medium
        );
    }
}
