use crate::{comparison::ComparisonRules, traffic::HttpTransaction};
use rusqlite::{Connection, params};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use thiserror::Error;
use time::OffsetDateTime;
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
pub struct Database {
    connection: Mutex<Connection>,
}
impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.busy_timeout(Duration::from_secs(5))?;
        super::migrations::apply(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        super::migrations::apply(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
    pub(crate) fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::Poisoned)
    }
    pub fn upsert_transaction(&self, transaction: &HttpTransaction) -> Result<(), StoreError> {
        self.connection()?.execute(
            "INSERT INTO transactions(id,session_id,state,payload_json,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET
             state=excluded.state,payload_json=excluded.payload_json,updated_at=excluded.updated_at",
            params![transaction.id.to_string(), transaction.session_id.to_string(),
                format!("{:?}", transaction.state), serde_json::to_string(transaction)?,
                transaction.created_at.to_string(), transaction.updated_at.to_string()])?;
        Ok(())
    }
    pub fn list_transactions(
        &self,
        session_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT payload_json FROM transactions WHERE session_id=?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3")?;
        let rows = statement.query_map(
            params![session_id.to_string(), limit as i64, offset as i64],
            |row| row.get::<_, String>(0),
        )?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn get_transaction(&self, id: Uuid) -> Result<Option<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT payload_json FROM transactions WHERE id=?1")?;
        let mut rows = statement.query([id.to_string()])?;
        rows.next()?
            .map(|row| Ok(serde_json::from_str(&row.get::<_, String>(0)?)?))
            .transpose()
    }
    pub fn all_session_transactions(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT payload_json FROM transactions WHERE session_id=?1 ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map([session_id.to_string()], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn transactions_between(
        &self,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<Vec<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT payload_json FROM transactions WHERE created_at >= ?1 AND created_at < ?2 ORDER BY created_at ASC")?;
        let rows = statement.query_map(params![start.to_string(), end.to_string()], |row| {
            row.get::<_, String>(0)
        })?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    /// Removes every persisted capture-derived record, including comparison and
    /// diagnostic metadata that could otherwise reveal information about a
    /// deleted capture after the next application launch.
    pub fn delete_all_transactions(&self) -> Result<(), StoreError> {
        self.connection()?.execute_batch(
            "DELETE FROM request_headers;
             DELETE FROM response_headers;
             DELETE FROM body_artifacts;
             DELETE FROM observations;
             DELETE FROM approved_baselines;
             DELETE FROM comparisons;
             DELETE FROM differences;
             DELETE FROM correlations;
             DELETE FROM log_incidents;
             DELETE FROM interaction_windows;
             DELETE FROM performance_samples;
             DELETE FROM issue_occurrences;
             DELETE FROM issues;
             DELETE FROM transactions;",
        )?;
        Ok(())
    }
    pub fn approve_baseline(
        &self,
        endpoint_id: &str,
        transaction_id: Uuid,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM transactions WHERE id=?1)",
            [transaction_id.to_string()],
            |row| row.get(0),
        )?;
        if !transaction_exists {
            return Err(StoreError::BaselineTransactionMissing(transaction_id));
        }
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM approved_baselines WHERE endpoint_id=?1",
            [endpoint_id],
        )?;
        transaction.execute(
            "INSERT INTO approved_baselines(id,endpoint_id,transaction_id,approved_at,provenance_json)
             VALUES (?1,?2,?3,CURRENT_TIMESTAMP,'{\"source\":\"user\"}')",
            params![Uuid::new_v4().to_string(), endpoint_id, transaction_id.to_string()])?;
        transaction.commit()?;
        Ok(())
    }
    pub fn delete_baseline(&self, endpoint_id: &str) -> Result<bool, StoreError> {
        Ok(self.connection()?.execute(
            "DELETE FROM approved_baselines WHERE endpoint_id=?1",
            [endpoint_id],
        )? > 0)
    }
    pub fn pinned_baseline(
        &self,
        endpoint_id: &str,
    ) -> Result<Option<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT transactions.payload_json FROM approved_baselines
             JOIN transactions ON transactions.id=approved_baselines.transaction_id
             WHERE approved_baselines.endpoint_id=?1
             ORDER BY approved_baselines.approved_at DESC LIMIT 1",
        )?;
        let mut rows = statement.query([endpoint_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?))
    }
    pub fn comparison_rules(&self, endpoint_id: &str) -> Result<ComparisonRules, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT payload_json FROM comparison_rules
             WHERE project_id='default' AND endpoint_id=?1 ORDER BY rowid DESC LIMIT 1",
        )?;
        let mut rows = statement.query([endpoint_id])?;
        let Some(row) = rows.next()? else {
            return Ok(ComparisonRules::default());
        };
        Ok(serde_json::from_str(&row.get::<_, String>(0)?)?)
    }
    pub fn save_comparison_rules(
        &self,
        endpoint_id: &str,
        rules: &ComparisonRules,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM comparison_rules WHERE project_id='default' AND endpoint_id=?1",
            [endpoint_id],
        )?;
        transaction.execute(
            "INSERT INTO comparison_rules(id,project_id,endpoint_id,payload_json)
             VALUES (?1, 'default', ?2, ?3)",
            params![
                Uuid::new_v4().to_string(),
                endpoint_id,
                serde_json::to_string(rules)?
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }
    pub fn migration_version(&self) -> Result<i64, StoreError> {
        Ok(self
            .connection()?
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?
            .unwrap_or_default())
    }

    /// Writes a transaction from an async context. SQLite is synchronous, so
    /// the write hops to a blocking thread; running it inline on a tokio
    /// worker would stall the capture proxy and every other command.
    pub async fn upsert_async(
        self: &Arc<Self>,
        transaction: HttpTransaction,
    ) -> Result<(), StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || database.upsert_transaction(&transaction)).await?
    }

    pub async fn pinned_baseline_async(
        self: &Arc<Self>,
        endpoint_id: String,
    ) -> Result<Option<HttpTransaction>, StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || database.pinned_baseline(&endpoint_id)).await?
    }

    pub async fn comparison_rules_async(
        self: &Arc<Self>,
        endpoint_id: String,
    ) -> Result<ComparisonRules, StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || database.comparison_rules(&endpoint_id)).await?
    }

    pub async fn transactions_between_async(
        self: &Arc<Self>,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<Vec<HttpTransaction>, StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || database.transactions_between(start, end)).await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_has_no_navigation_tables() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.migration_version().unwrap(), 2);
        let count: i64 = db.connection().unwrap().query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'navigation_%'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn async_variants_round_trip_baselines_and_rules() {
        let database = Arc::new(Database::open_in_memory().unwrap());
        let transaction: HttpTransaction = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(), "session_id": uuid::Uuid::new_v4(), "connection_id": uuid::Uuid::new_v4(),
            "request": {"method":"GET","scheme":"https","host":"api.test","path":"/v1","query":[],"headers":[],"body":{"storage":"empty"},"http_version":"HTTP_1_1"},
            "state": "response_complete", "timing": {"request_started_ms": 0}, "capture_quality": "complete",
            "correlated_incidents": [], "created_at": "2026-07-24T00:00:00Z", "updated_at": "2026-07-24T00:00:00Z"
        }))
        .unwrap();
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
