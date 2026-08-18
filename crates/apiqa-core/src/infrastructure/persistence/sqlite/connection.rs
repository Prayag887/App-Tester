//! SQLite connection wrapper with WAL mode and migration engine wiring.
//!
//! Owns the [`Database`] struct that serves as the application's persistence
//! handle. On open, the migration engine validates and upgrades the schema.

use crate::persistence::StoreError;
use rusqlite::{Connection, OptionalExtension};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use super::migrate::MigrationEngine;

const MIN_RECLAIMABLE_BYTES: i64 = 8 * 1024 * 1024;
const MIN_FREE_PAGE_RATIO_DENOMINATOR: i64 = 4;

fn storage_compaction_is_worthwhile(page_count: i64, free_pages: i64, page_size: i64) -> bool {
    if page_count <= 0 || free_pages <= 0 || page_size <= 0 {
        return false;
    }
    let reclaimable = free_pages.saturating_mul(page_size);
    reclaimable >= MIN_RECLAIMABLE_BYTES
        && free_pages.saturating_mul(MIN_FREE_PAGE_RATIO_DENOMINATOR) >= page_count
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        // Migration engine handles backup + upgrade before the connection
        // is handed to the rest of the application.
        MigrationEngine::prepare(path)?;

        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.busy_timeout(Duration::from_secs(5))?;

        MigrationEngine::migrate(&connection, path)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        MigrationEngine::migrate_in_memory(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::Poisoned)
    }

    pub fn migration_version(&self) -> Result<i64, StoreError> {
        Ok(self
            .connection()?
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?
            .unwrap_or_default())
    }

    /// Reclaims a materially sparse database after history compaction. The
    /// threshold avoids paying VACUUM's rewrite cost for routine small gaps.
    pub fn compact_storage_if_worthwhile(&self) -> Result<bool, StoreError> {
        let connection = self.connection()?;
        let page_count: i64 = connection.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let free_pages: i64 =
            connection.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        let page_size: i64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        if !storage_compaction_is_worthwhile(page_count, free_pages, page_size) {
            return Ok(false);
        }
        connection.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE);
             VACUUM;
             PRAGMA wal_checkpoint(TRUNCATE);",
        )?;
        Ok(true)
    }

    // Async wrappers — these live here so the Database struct is self-contained.
    // Persistence modules call these helpers.

    pub async fn upsert_async(
        self: &Arc<Self>,
        transaction: crate::traffic::HttpTransaction,
    ) -> Result<Option<crate::traffic::DailyChangeSummary>, StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            database.upsert_transaction(&transaction)?;
            Ok(database
                .get_transaction(transaction.id)?
                .and_then(|stored| stored.daily_changes))
        })
        .await?
    }

    pub async fn pinned_baseline_async(
        self: &Arc<Self>,
        endpoint_id: String,
    ) -> Result<Option<crate::traffic::HttpTransaction>, StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || database.pinned_baseline(&endpoint_id)).await?
    }

    pub async fn comparison_rules_async(
        self: &Arc<Self>,
        endpoint_id: String,
    ) -> Result<crate::comparison::ComparisonRules, StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || database.comparison_rules(&endpoint_id)).await?
    }

    pub async fn transactions_between_async(
        self: &Arc<Self>,
        start: time::OffsetDateTime,
        end: time::OffsetDateTime,
    ) -> Result<Vec<crate::traffic::HttpTransaction>, StoreError> {
        let database = Arc::clone(self);
        tokio::task::spawn_blocking(move || database.transactions_between(start, end)).await?
    }
}

use crate::comparison::ComparisonRules;
use crate::traffic::{DailyChangeSummary, HttpTransaction, TransactionState};
#[allow(unused_imports)]
use rusqlite::params;
use time::OffsetDateTime;
use uuid::Uuid;

// Business-logic methods on Database — these stay here because Database
// is the persistence handle. They delegate through connection().

type StoredTransactionRow = (String, Option<u32>, Option<String>);

fn transaction_params(transaction: &HttpTransaction) -> Result<[String; 6], serde_json::Error> {
    Ok([
        transaction.id.to_string(),
        transaction.session_id.to_string(),
        format!("{:?}", transaction.state),
        serde_json::to_string(transaction)?,
        transaction.created_at.to_string(),
        transaction.updated_at.to_string(),
    ])
}

fn stored_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredTransactionRow> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn parse_timestamp(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
}

fn hydrate_transaction(row: StoredTransactionRow) -> Result<HttpTransaction, StoreError> {
    let (payload, change_count, last_changed_at) = row;
    let mut transaction: HttpTransaction = serde_json::from_str(&payload)?;
    transaction.daily_changes =
        change_count
            .filter(|count| *count > 0)
            .map(|count| DailyChangeSummary {
                count,
                last_changed_at: last_changed_at.as_deref().and_then(parse_timestamp),
            });
    Ok(transaction)
}

fn response_changed(previous: &HttpTransaction, current: &HttpTransaction) -> bool {
    match (previous.response.as_ref(), current.response.as_ref()) {
        (Some(before), Some(after)) => {
            before.status != after.status
                || before.content_type != after.content_type
                || before.decoded_size != after.decoded_size
                || before.body != after.body
        }
        (None, None) => false,
        _ => true,
    }
}

impl Database {
    pub fn upsert_transaction(&self, transaction: &HttpTransaction) -> Result<(), StoreError> {
        if transaction.state != TransactionState::ResponseComplete
            || transaction.endpoint_identity.is_none()
        {
            self.connection()?.execute(
                "INSERT INTO transactions(id,session_id,state,payload_json,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET
                 state=excluded.state,payload_json=excluded.payload_json,updated_at=excluded.updated_at",
                transaction_params(transaction)?,
            )?;
            return Ok(());
        }

        self.upsert_daily_snapshot(transaction)
    }

    fn upsert_daily_snapshot(&self, transaction: &HttpTransaction) -> Result<(), StoreError> {
        let Some(endpoint) = transaction.endpoint_identity.as_ref() else {
            return Ok(());
        };
        let endpoint_key = crate::proxy::baseline_key(endpoint);
        let observed_day = transaction.created_at.date().to_string();
        let mut connection = self.connection()?;
        let sql_transaction = connection.transaction()?;

        let current_snapshot = sql_transaction
            .query_row(
                "SELECT d.transaction_id,d.change_count,d.last_changed_at,t.payload_json
                 FROM endpoint_daily_snapshots d
                 JOIN transactions t ON t.id=d.transaction_id
                 WHERE d.endpoint_key=?1 AND d.observed_day=?2",
                params![endpoint_key, observed_day],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        let previous_payload = match current_snapshot.as_ref() {
            Some((_, _, _, payload)) => Some(payload.clone()),
            None => sql_transaction
                .query_row(
                    "SELECT t.payload_json
                     FROM endpoint_daily_snapshots d
                     JOIN transactions t ON t.id=d.transaction_id
                     WHERE d.endpoint_key=?1
                     ORDER BY d.observed_day DESC LIMIT 1",
                    [endpoint_key.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?,
        };
        let changed = previous_payload
            .as_deref()
            .map(serde_json::from_str::<HttpTransaction>)
            .transpose()?
            .is_some_and(|previous| response_changed(&previous, transaction));
        let previous_count = current_snapshot
            .as_ref()
            .map(|(_, count, _, _)| *count)
            .unwrap_or_default();
        let change_count = previous_count.saturating_add(u32::from(changed));
        let last_changed_at = if changed {
            Some(transaction.updated_at)
        } else {
            current_snapshot
                .as_ref()
                .and_then(|(_, _, value, _)| value.as_deref())
                .and_then(parse_timestamp)
        };

        let mut stored = transaction.clone();
        stored.daily_changes = (change_count > 0).then_some(DailyChangeSummary {
            count: change_count,
            last_changed_at,
        });
        sql_transaction.execute(
            "INSERT INTO transactions(id,session_id,state,payload_json,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET
             state=excluded.state,payload_json=excluded.payload_json,updated_at=excluded.updated_at",
            transaction_params(&stored)?,
        )?;
        sql_transaction.execute(
            "INSERT INTO endpoint_daily_snapshots(
                endpoint_key,observed_day,transaction_id,change_count,last_changed_at
             ) VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(endpoint_key,observed_day) DO UPDATE SET
                transaction_id=excluded.transaction_id,
                change_count=excluded.change_count,
                last_changed_at=excluded.last_changed_at",
            params![
                endpoint_key,
                observed_day,
                stored.id.to_string(),
                change_count,
                last_changed_at.map(|value| value.to_string()),
            ],
        )?;
        if let Some((previous_id, _, _, _)) = current_snapshot
            && previous_id != stored.id.to_string()
        {
            sql_transaction.execute(
                "DELETE FROM transactions
                 WHERE id=?1 AND NOT EXISTS(
                    SELECT 1 FROM approved_baselines WHERE transaction_id=?1
                 )",
                [previous_id],
            )?;
        }
        sql_transaction.commit()?;
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
            "SELECT t.payload_json,d.change_count,d.last_changed_at
             FROM transactions t LEFT JOIN endpoint_daily_snapshots d ON d.transaction_id=t.id
             WHERE t.session_id=?1 ORDER BY t.created_at DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = statement.query_map(
            params![session_id.to_string(), limit as i64, offset as i64],
            stored_row,
        )?;
        rows.map(|row| hydrate_transaction(row?)).collect()
    }

    pub fn get_transaction(&self, id: Uuid) -> Result<Option<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT t.payload_json,d.change_count,d.last_changed_at
                 FROM transactions t LEFT JOIN endpoint_daily_snapshots d ON d.transaction_id=t.id
                 WHERE t.id=?1",
        )?;
        let mut rows = statement.query([id.to_string()])?;
        rows.next()?
            .map(|row| hydrate_transaction(stored_row(row)?))
            .transpose()
    }

    pub fn all_session_transactions(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT t.payload_json,d.change_count,d.last_changed_at
             FROM transactions t LEFT JOIN endpoint_daily_snapshots d ON d.transaction_id=t.id
             WHERE t.session_id=?1 ORDER BY t.created_at ASC",
        )?;
        let rows = statement.query_map([session_id.to_string()], stored_row)?;
        rows.map(|row| hydrate_transaction(row?)).collect()
    }

    pub fn transactions_between(
        &self,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<Vec<HttpTransaction>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT t.payload_json,d.change_count,d.last_changed_at
             FROM transactions t LEFT JOIN endpoint_daily_snapshots d ON d.transaction_id=t.id
             WHERE t.created_at >= ?1 AND t.created_at < ?2 ORDER BY t.created_at ASC",
        )?;
        let rows = statement.query_map(params![start.to_string(), end.to_string()], stored_row)?;
        rows.map(|row| hydrate_transaction(row?)).collect()
    }

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
             DELETE FROM endpoint_daily_snapshots;
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
            params![
                Uuid::new_v4().to_string(),
                endpoint_id,
                transaction_id.to_string()
            ],
        )?;
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
}

#[cfg(test)]
mod daily_snapshot_tests {
    use super::*;

    #[test]
    fn compacts_only_when_space_and_fragmentation_are_material() {
        let page_size = 4096;
        let pages_per_mebibyte = 1024 * 1024 / page_size;

        assert!(storage_compaction_is_worthwhile(
            40 * pages_per_mebibyte,
            12 * pages_per_mebibyte,
            page_size,
        ));
        assert!(!storage_compaction_is_worthwhile(
            64 * pages_per_mebibyte,
            12 * pages_per_mebibyte,
            page_size,
        ));
        assert!(!storage_compaction_is_worthwhile(
            8 * pages_per_mebibyte,
            4 * pages_per_mebibyte,
            page_size,
        ));
        assert!(!storage_compaction_is_worthwhile(0, 0, page_size));
    }

    #[test]
    fn vacuum_reclaims_a_sparse_on_disk_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sparse.sqlite");
        let database = Database::open(&path).unwrap();
        {
            let connection = database.connection().unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE compaction_probe (payload BLOB NOT NULL);
                     INSERT INTO compaction_probe(payload) VALUES (zeroblob(12582912));
                     DELETE FROM compaction_probe;
                     PRAGMA wal_checkpoint(TRUNCATE);",
                )
                .unwrap();
        }
        let size_before = std::fs::metadata(&path).unwrap().len();

        assert!(database.compact_storage_if_worthwhile().unwrap());

        let size_after = std::fs::metadata(&path).unwrap().len();
        let free_pages: i64 = database
            .connection()
            .unwrap()
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))
            .unwrap();
        assert!(size_after < size_before / 2);
        assert_eq!(free_pages, 0);
    }

    fn completed(session_id: Uuid, body: &str, second: u8) -> HttpTransaction {
        serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "session_id": session_id,
            "connection_id": Uuid::new_v4(),
            "request": {
                "method": "GET", "scheme": "https", "host": "api.test",
                "path": "/users/42", "query": [], "headers": [],
                "body": {"storage": "empty"}, "http_version": "HTTP/2"
            },
            "response": {
                "status": 200, "headers": [],
                "body": {"storage": "inline", "bytes": body.as_bytes()},
                "content_type": "application/json", "decoded_size": body.len(),
                "encoded_size": body.len(), "http_version": "HTTP/2"
            },
            "state": "response_complete",
            "timing": {"request_started_ms": 0, "response_complete_ms": 1},
            "endpoint_identity": {
                "method": "GET", "host": "api.test", "path_template": "/users/{id}",
                "content_type": "application/json", "request_shape": null
            },
            "capture_quality": "complete", "correlated_incidents": [],
            "created_at": format!("2026-08-17T10:00:{second:02}Z"),
            "updated_at": format!("2026-08-17T10:00:{second:02}Z")
        }))
        .unwrap()
    }

    #[test]
    fn replaces_same_day_snapshots_and_preserves_change_count() {
        let database = Database::open_in_memory().unwrap();
        let session_id = Uuid::new_v4();
        let first = completed(session_id, r#"{"value":1}"#, 1);
        let second = completed(session_id, r#"{"value":2}"#, 2);
        let third = completed(session_id, r#"{"value":2}"#, 3);

        database.upsert_transaction(&first).unwrap();
        database.upsert_transaction(&second).unwrap();
        database.upsert_transaction(&third).unwrap();

        let stored = database.list_transactions(session_id, 10, 0).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, third.id);
        assert_eq!(
            stored[0].daily_changes.as_ref().map(|item| item.count),
            Some(1)
        );
    }

    #[test]
    fn keeps_a_pinned_snapshot_when_the_daily_latest_is_replaced() {
        let database = Database::open_in_memory().unwrap();
        let session_id = Uuid::new_v4();
        let first = completed(session_id, r#"{"value":1}"#, 1);
        let second = completed(session_id, r#"{"value":2}"#, 2);
        database.upsert_transaction(&first).unwrap();
        database
            .approve_baseline("GET api.test /users/{id}", first.id)
            .unwrap();
        database.upsert_transaction(&second).unwrap();

        assert!(database.get_transaction(first.id).unwrap().is_some());
        assert_eq!(
            database
                .get_transaction(second.id)
                .unwrap()
                .unwrap()
                .daily_changes
                .unwrap()
                .count,
            1
        );
    }
}
