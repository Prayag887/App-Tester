//! Regression replay of yesterday's captured traffic.

use std::{sync::Arc, time::Duration};

use time::{OffsetDateTime, PrimitiveDateTime, Time};

use super::{ReplaySummary, replay, replay_blocker};
use crate::{
    events::{EventBroadcaster, InspectorEvent},
    persistence::{Database, StoreError},
    traffic::TransactionState,
};
use uuid::Uuid;

const REPLAY_TIMEOUT: Duration = Duration::from_secs(30);

/// Replays every complete, non-redacted transaction captured between midnight
/// and midnight of the previous UTC day, stores the results in the database,
/// and streams a `TransactionCompleted` event per replay.
pub async fn run_daily_replay(
    database: Arc<Database>,
    events: EventBroadcaster,
    session_id: Uuid,
) -> Result<ReplaySummary, StoreError> {
    let today = OffsetDateTime::now_utc().date();
    let yesterday = today
        .previous_day()
        .ok_or_else(|| StoreError::Replay("could not calculate yesterday".into()))?;
    let start = PrimitiveDateTime::new(yesterday, Time::MIDNIGHT).assume_utc();
    let end = PrimitiveDateTime::new(today, Time::MIDNIGHT).assume_utc();
    let baselines = database.transactions_between_async(start, end).await?;
    let client = reqwest::Client::builder()
        .timeout(REPLAY_TIMEOUT)
        .build()
        .map_err(|error| StoreError::Replay(error.to_string()))?;
    let mut summary = ReplaySummary::default();
    for baseline in baselines {
        if replay_blocker(&baseline).is_some() {
            summary.skipped += 1;
            continue;
        }
        summary.attempted += 1;
        let result = replay(&client, &baseline, session_id).await;
        if result.state == TransactionState::Failed {
            summary.failed += 1;
        } else {
            summary.completed += 1;
            if result
                .comparison
                .as_ref()
                .is_some_and(|comparison| !comparison.differences.is_empty())
            {
                summary.changed += 1;
            }
        }
        database.upsert_async(result.clone()).await?;
        events.send(InspectorEvent::TransactionCompleted(result));
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        persistence::Database,
        traffic::{
            BodyStorage, CaptureQuality, CapturedRequest, CapturedResponse, HeaderEntry,
            HttpTransaction, QueryParameter, TransactionState, TransactionTiming,
        },
    };

    fn baseline(host: &str, port: u16, timestamp: OffsetDateTime) -> HttpTransaction {
        HttpTransaction {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            connection_id: Uuid::new_v4(),
            request: CapturedRequest {
                method: "GET".into(),
                scheme: "http".into(),
                host: host.into(),
                port: Some(port),
                path: "/v1/test".into(),
                query: vec![QueryParameter {
                    name: "q".into(),
                    value: "1".into(),
                }],
                headers: vec![HeaderEntry {
                    name: "x-replay-probe".into(),
                    value: "1".into(),
                }],
                body: BodyStorage::Empty,
                content_type: None,
                http_version: "HTTP/1.1".into(),
            },
            response: Some(CapturedResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: vec![],
                body: BodyStorage::Inline {
                    bytes: br#"{"ok":true}"#.to_vec(),
                },
                content_type: Some("application/json".into()),
                decoded_size: 11,
                encoded_size: 11,
                http_version: "HTTP/1.1".into(),
            }),
            state: TransactionState::ResponseComplete,
            timing: TransactionTiming::default(),
            endpoint_identity: None,
            curl: None,
            capture_quality: CaptureQuality::Complete,
            comparison: None,
            correlated_incidents: vec![],
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    async fn serve_json_response() -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
            let _ = socket.write_all(response.as_bytes()).await;
        });
        port
    }

    #[tokio::test]
    async fn replays_yesterdays_transactions_and_reports_summary() {
        let database = Arc::new(Database::open_in_memory().unwrap());
        let port = serve_json_response().await;
        let today = OffsetDateTime::now_utc().date();
        let yesterday = today.previous_day().unwrap();
        let stored = baseline(
            "127.0.0.1",
            port,
            PrimitiveDateTime::new(yesterday, Time::from_hms(12, 0, 0).unwrap()).assume_utc(),
        );
        database.upsert_transaction(&stored).unwrap();
        let redacted = {
            let mut transaction = baseline(
                "127.0.0.1",
                port,
                PrimitiveDateTime::new(yesterday, Time::from_hms(13, 0, 0).unwrap()).assume_utc(),
            );
            transaction.request.query = vec![QueryParameter {
                name: "token".into(),
                value: "<redacted>".into(),
            }];
            transaction
        };
        database.upsert_transaction(&redacted).unwrap();
        let events = EventBroadcaster::default();
        let mut receiver = events.subscribe();
        let summary = run_daily_replay(database, events, Uuid::new_v4())
            .await
            .unwrap();
        assert_eq!(summary.attempted, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.changed, 0);
        let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
            .await
            .expect("replay event");
        assert!(matches!(event, Ok(InspectorEvent::TransactionCompleted(_))));
    }

    #[tokio::test]
    async fn ignores_todays_transactions() {
        let database = Arc::new(Database::open_in_memory().unwrap());
        let port = serve_json_response().await;
        let now = OffsetDateTime::now_utc();
        let today_transaction = baseline("127.0.0.1", port, now);
        database.upsert_transaction(&today_transaction).unwrap();
        let summary = run_daily_replay(database, EventBroadcaster::default(), Uuid::new_v4())
            .await
            .unwrap();
        assert_eq!(summary.attempted, 0);
        assert_eq!(summary.skipped, 0);
    }
}
