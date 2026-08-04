use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum InspectorEvent {
    ProxyStatusChanged(crate::proxy::ProxyStatus),
    SessionStatusChanged(crate::session::SessionStatus),
    TransactionCreated(crate::traffic::HttpTransaction),
    TransactionUpdated(crate::traffic::HttpTransaction),
    TransactionCompleted(crate::traffic::HttpTransaction),
    ComparisonCompleted {
        transaction_id: Uuid,
        comparison: crate::comparison::ResponseComparison,
    },
    IncidentCreated(crate::diagnostics::LogIncident),
    IssueCreated(crate::issues::Issue),
    DeviceStatusChanged(String),
}

#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<InspectorEvent>,
}
impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new(512)
    }
}
impl EventBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
    pub fn send(&self, event: InspectorEvent) {
        let _ = self.sender.send(event);
    }
    pub fn subscribe(&self) -> broadcast::Receiver<InspectorEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcasts_to_every_subscriber_in_send_order() {
        let broadcaster = EventBroadcaster::default();
        let mut first = broadcaster.subscribe();
        let mut second = broadcaster.subscribe();
        broadcaster.send(InspectorEvent::DeviceStatusChanged("emulator-5554".into()));
        broadcaster.send(InspectorEvent::DeviceStatusChanged("R58M123".into()));
        assert!(matches!(
            first.try_recv().unwrap(),
            InspectorEvent::DeviceStatusChanged(serial) if serial == "emulator-5554"
        ));
        assert!(matches!(
            first.try_recv().unwrap(),
            InspectorEvent::DeviceStatusChanged(serial) if serial == "R58M123"
        ));
        assert!(matches!(
            second.try_recv().unwrap(),
            InspectorEvent::DeviceStatusChanged(serial) if serial == "emulator-5554"
        ));
    }

    #[test]
    fn send_ignores_lagging_subscribers_without_panicking() {
        let broadcaster = EventBroadcaster::new(2);
        let mut receiver = broadcaster.subscribe();
        broadcaster.send(InspectorEvent::DeviceStatusChanged("one".into()));
        broadcaster.send(InspectorEvent::DeviceStatusChanged("two".into()));
        std::mem::drop(receiver.recv());
        // The receiver now lags behind the small buffer; the next send must
        // drop the oldest event for it rather than aborting the broadcaster.
        broadcaster.send(InspectorEvent::DeviceStatusChanged("three".into()));
        std::mem::drop(receiver.recv());
        assert!(matches!(
            receiver.try_recv(),
            Ok(InspectorEvent::DeviceStatusChanged(_))
                | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
        ));
    }

    #[test]
    fn serde_round_trips_a_transaction_event_with_kind_tag() {
        let transaction: crate::traffic::HttpTransaction = serde_json::from_value(
            serde_json::json!({
                "id": uuid::Uuid::new_v4(), "session_id": uuid::Uuid::new_v4(), "connection_id": uuid::Uuid::new_v4(),
                "request": {"method":"GET","scheme":"https","host":"api.test","path":"/v1","query":[],"headers":[],"body":{"storage":"empty"},"http_version":"HTTP_1_1"},
                "state": "request_started", "timing": {"request_started_ms": 0}, "capture_quality": "metadata_only",
                "correlated_incidents": [], "created_at": "2026-07-24T00:00:00Z", "updated_at": "2026-07-24T00:00:00Z"
            }),
        )
        .unwrap();
        let event = InspectorEvent::TransactionCreated(transaction);
        let encoded = serde_json::to_value(&event).unwrap();
        assert_eq!(encoded["kind"], "transaction_created");
        let decoded: InspectorEvent = serde_json::from_value(encoded).unwrap();
        match decoded {
            InspectorEvent::TransactionCreated(decoded) => {
                assert_eq!(decoded.request.host, "api.test");
            }
            other => panic!("expected TransactionCreated, got {other:?}"),
        }
    }
}
