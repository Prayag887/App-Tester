//! Persistence error types and legacy re-exports.
//!
//! The [`Database`] struct now lives in
//! `crate::infrastructure::persistence::sqlite::connection` and is re-exported
//! through `crate::persistence`.

use rusqlite;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database lock poisoned")]
    Poisoned,
    #[error("database task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("baseline transaction {0} does not exist")]
    BaselineTransactionMissing(Uuid),
    #[error("replay failed: {0}")]
    Replay(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comparison::ComparisonRules;
    use crate::persistence::Database;
    use std::sync::Arc;

    #[test]
    fn schema_has_no_navigation_tables() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.migration_version().unwrap(), 4);
        let count: i64 = db.connection().unwrap().query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'navigation_%'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn async_variants_round_trip_baselines_and_rules() {
        let database = Arc::new(Database::open_in_memory().unwrap());
        let transaction: crate::traffic::HttpTransaction = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(), "session_id": Uuid::new_v4(), "connection_id": Uuid::new_v4(),
            "request": {"method":"GET","scheme":"https","host":"api.test","path":"/v1","query":[],"headers":[],"body":{"storage":"empty"},"http_version":"HTTP_1_1"},
            "state": "response_complete", "timing": {"request_started_ms": 0}, "capture_quality": "complete",
            "correlated_incidents": [], "created_at": "2026-07-24T00:00:00Z", "updated_at": "2026-07-24T00:00:00Z"
        })).unwrap();
        database.upsert_async(transaction.clone()).await.unwrap();
        database
            .approve_baseline("GET api.test /v1", transaction.id)
            .unwrap();
        let pinned = database
            .pinned_baseline_async("GET api.test /v1".into())
            .await
            .unwrap();
        assert_eq!(pinned.unwrap().id, transaction.id);
        let rules = ComparisonRules {
            ignored_json_pointers: vec!["$.token".into()].into_iter().collect(),
            volatile_keys: vec!["timestamp".into()].into_iter().collect(),
        };
        database
            .save_comparison_rules("GET api.test /v1", &rules)
            .unwrap();
        let loaded = database
            .comparison_rules_async("GET api.test /v1".into())
            .await
            .unwrap();
        assert_eq!(loaded.ignored_json_pointers, rules.ignored_json_pointers);
        assert_eq!(loaded.volatile_keys, rules.volatile_keys);
    }

    #[test]
    fn opening_existing_database_does_not_lose_data() {
        let db = Database::open_in_memory().unwrap();
        let connection = db.connection().unwrap();
        connection
            .execute(
                "INSERT INTO projects(id,name,created_at) VALUES ('p1','Test','2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        drop(connection);
        let connection = db.connection().unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn legacy_dormant_tables_exist_and_are_queryable() {
        let db = Database::open_in_memory().unwrap();
        let connection = db.connection().unwrap();
        for table in &["projects", "environments", "devices", "sessions"] {
            let count: i64 = connection
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "legacy table '{table}' must exist");
        }
    }

    #[test]
    fn deleting_capture_data_removes_baselines_and_diagnostics() {
        let db = Database::open_in_memory().unwrap();
        let connection = db.connection().unwrap();
        let transaction_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO transactions(id,session_id,state,payload_json,created_at,updated_at)
             VALUES (?1, 'session', 'Completed', '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                [&transaction_id],
            )
            .unwrap();
        connection.execute(
            "INSERT INTO approved_baselines(id,endpoint_id,transaction_id,approved_at,provenance_json)
             VALUES ('baseline', 'endpoint', ?1, CURRENT_TIMESTAMP, '{}')",
            [&transaction_id],
        ).unwrap();
        connection
            .execute(
                "INSERT INTO log_incidents(id,session_id,signature,payload_json,occurrence_count)
             VALUES ('incident', 'session', 'signature', '{}', 1)",
                [],
            )
            .unwrap();
        drop(connection);

        db.delete_all_transactions().unwrap();

        let connection = db.connection().unwrap();
        for table in ["transactions", "approved_baselines", "log_incidents"] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} should be cleared");
        }
    }
}
