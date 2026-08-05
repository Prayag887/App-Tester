//! Composer history: recently sent requests, deduplicated by exact content.
//!
//! Re-sending an identical request bumps its timestamp instead of adding a
//! row, so the list stays a clean "most recent distinct requests" view.
//! History rows carry the *resolved* request (what actually went on the
//! wire), matching the stored transactions.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Database, StoreError};
use crate::composer::model::ManualRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySummary {
    pub id: Uuid,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub sent_at: String,
}

const LIST_LIMIT: usize = 50;

/// RFC3339 (the same format transactions use over the wire), so timestamps
/// are directly parseable by the frontend (`new Date`).
fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().to_string())
}

impl Database {
    pub fn record_history(
        &self,
        request: &ManualRequest,
        status: Option<u16>,
    ) -> Result<(), StoreError> {
        self.connection()?.execute(
            "INSERT INTO history (id, method, url, request_json, status, sent_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(method, url, request_json)
             DO UPDATE SET status = excluded.status, sent_at = excluded.sent_at",
            params![
                Uuid::new_v4().to_string(),
                request.method,
                request.url,
                serde_json::to_string(request)?,
                status.map(i64::from),
                now(),
            ],
        )?;
        Ok(())
    }

    pub fn list_history(&self, limit: usize) -> Result<Vec<HistorySummary>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, method, url, status, sent_at
             FROM history ORDER BY sent_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit.min(LIST_LIMIT) as i64], |row| {
            Ok(HistorySummary {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                method: row.get(1)?,
                url: row.get(2)?,
                status: row.get::<_, Option<i64>>(3)?.map(|status| status as u16),
                sent_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// The full request behind a history row, loaded only when opened.
    pub fn get_history_request(&self, id: Uuid) -> Result<ManualRequest, StoreError> {
        let payload = self
            .connection()?
            .query_row(
                "SELECT request_json FROM history WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(StoreError::MissingHistory(id))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn delete_history(&self, id: Uuid) -> Result<(), StoreError> {
        self.connection()?
            .execute("DELETE FROM history WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<(), StoreError> {
        self.connection()?.execute("DELETE FROM history", [])?;
        Ok(())
    }

    // ---- async variants (spawn_blocking, same serialization as captures) ----

    pub async fn record_history_async(
        self: &std::sync::Arc<Self>,
        request: &ManualRequest,
        status: Option<u16>,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        let request = request.clone();
        tokio::task::spawn_blocking(move || database.record_history(&request, status)).await?
    }

    pub async fn list_history_async(
        self: &std::sync::Arc<Self>,
    ) -> Result<Vec<HistorySummary>, StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.list_history(LIST_LIMIT)).await?
    }

    pub async fn get_history_request_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
    ) -> Result<ManualRequest, StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.get_history_request(id)).await?
    }

    pub async fn delete_history_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.delete_history(id)).await?
    }

    pub async fn clear_history_async(self: &std::sync::Arc<Self>) -> Result<(), StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.clear_history()).await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, url: &str) -> ManualRequest {
        ManualRequest {
            method: method.to_string(),
            url: url.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn deduplicates_identical_requests_and_bumps_the_timestamp() {
        let database = Database::open_in_memory().unwrap();
        database
            .record_history(&request("GET", "https://a.test/v1"), Some(200))
            .unwrap();
        database
            .record_history(&request("POST", "https://a.test/v1"), Some(201))
            .unwrap();
        database
            .record_history(&request("GET", "https://a.test/v1"), Some(200))
            .unwrap();

        let entries = database.list_history(50).unwrap();
        assert_eq!(entries.len(), 2, "the repeated GET dedupes into one row");
        assert_eq!(entries[0].method, "GET");
        assert_eq!(entries[0].status, Some(200));
    }

    #[test]
    fn lists_most_recent_first_and_round_trips_requests() {
        let database = Database::open_in_memory().unwrap();
        database
            .record_history(&request("GET", "https://a.test/first"), None)
            .unwrap();
        database
            .record_history(&request("POST", "https://a.test/second"), Some(500))
            .unwrap();

        let entries = database.list_history(50).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].url, "https://a.test/second");

        let loaded = database.get_history_request(entries[1].id).unwrap();
        assert_eq!(loaded.method, "GET");
        assert_eq!(loaded.url, "https://a.test/first");
    }

    #[test]
    fn delete_and_clear_remove_entries() {
        let database = Database::open_in_memory().unwrap();
        database
            .record_history(&request("GET", "https://a.test/x"), Some(200))
            .unwrap();
        let entry = database.list_history(50).unwrap().remove(0);
        database.delete_history(entry.id).unwrap();
        assert!(database.list_history(50).unwrap().is_empty());

        database
            .record_history(&request("GET", "https://a.test/y"), Some(200))
            .unwrap();
        database.clear_history().unwrap();
        assert!(database.list_history(50).unwrap().is_empty());
    }
}
