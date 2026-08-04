//! Forwards core events to the WebView with stable, snake_case names.

use androidqa_core::events::InspectorEvent;
use tauri::{Emitter, Manager};

pub fn event_name(event: &InspectorEvent) -> &'static str {
    match event {
        InspectorEvent::ProxyStatusChanged(_) => "proxy-status-changed",
        InspectorEvent::SessionStatusChanged(_) => "session-status-changed",
        InspectorEvent::TransactionCreated(_) => "transaction-created",
        InspectorEvent::TransactionUpdated(_) => "transaction-updated",
        InspectorEvent::TransactionCompleted(_) => "transaction-completed",
        InspectorEvent::ComparisonCompleted { .. } => "comparison-completed",
        InspectorEvent::IncidentCreated(_) => "incident-created",
        InspectorEvent::IssueCreated(_) => "issue-created",
        InspectorEvent::DeviceStatusChanged(_) => "device-status-changed",
    }
}

/// Spawns the broadcaster loop that relays core events to the WebView.
pub fn forward_events(app: &tauri::AppHandle) {
    let mut receiver = app
        .state::<crate::state::InspectorState>()
        .proxy
        .events()
        .subscribe();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            let name = event_name(&event);
            let _ = handle.emit(name, event);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use androidqa_core::proxy::ProxyStatus;

    #[test]
    fn maps_every_event_variant_to_a_stable_webview_channel() {
        use androidqa_core::{diagnostics::LogIncident, traffic::HttpTransaction};
        let transaction: HttpTransaction = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(),"session_id":uuid::Uuid::new_v4(),"connection_id":uuid::Uuid::new_v4(),
            "request":{"method":"GET","scheme":"https","host":"api.test","path":"/v1","query":[],"headers":[],"body":{"storage":"empty"},"http_version":"HTTP_1_1"},
            "state":"request_started","timing":{"request_started_ms":0},"capture_quality":"metadata_only","correlated_incidents":[],
            "created_at":"2026-07-24T00:00:00Z","updated_at":"2026-07-24T00:00:00Z"
        }))
        .unwrap();
        let incident: LogIncident = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(),"session_id":uuid::Uuid::new_v4(),"category":"error","signature":"sig","title":"t","message":"m",
            "summary":"s","where_occurred":"w","how_occurred":"h","likely_cause":"l","reproduction_steps":[],
            "lines":[],"occurrence_count":1,"first_occurred_at":"2026-07-24T00:00:00Z","occurred_at":"2026-07-24T00:00:00Z"
        }))
        .unwrap();
        assert_eq!(
            event_name(&InspectorEvent::ProxyStatusChanged(ProxyStatus::Running)),
            "proxy-status-changed"
        );
        assert_eq!(
            event_name(&InspectorEvent::TransactionCreated(transaction)),
            "transaction-created"
        );
        assert_eq!(
            event_name(&InspectorEvent::IncidentCreated(incident)),
            "incident-created"
        );
    }
}
