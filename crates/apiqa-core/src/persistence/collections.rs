//! Collections and saved requests: the composer's library.
//!
//! Everything here is additive to the capture schema — these tables are only
//! touched by composer commands. Requests are stored as serialized
//! [`ManualRequest`] payloads with method/url denormalized for listing.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Database, StoreError};
use crate::composer::model::ManualRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub request_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRequest {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub name: String,
    pub request: ManualRequest,
    pub created_at: String,
    pub updated_at: String,
}

fn now() -> String {
    OffsetDateTime::now_utc().to_string()
}

impl Database {
    pub fn create_collection(
        &self,
        name: &str,
        description: &str,
        color: &str,
    ) -> Result<CollectionSummary, StoreError> {
        let id = Uuid::new_v4();
        let timestamp = now();
        self.connection()?.execute(
            "INSERT INTO collections (id, name, description, color, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id.to_string(), name, description, color, timestamp],
        )?;
        Ok(CollectionSummary {
            id,
            name: name.to_string(),
            description: description.to_string(),
            color: color.to_string(),
            request_count: 0,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        })
    }

    pub fn rename_collection(&self, id: Uuid, name: &str) -> Result<(), StoreError> {
        self.connection()?.execute(
            "UPDATE collections SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn delete_collection(&self, id: Uuid) -> Result<(), StoreError> {
        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM requests WHERE collection_id = ?1",
            params![id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM collections WHERE id = ?1",
            params![id.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_collections(&self) -> Result<Vec<CollectionSummary>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT c.id, c.name, c.description, c.color, c.created_at, c.updated_at,
                    COUNT(r.id) AS request_count
             FROM collections c
             LEFT JOIN requests r ON r.collection_id = c.id
             GROUP BY c.id
             ORDER BY c.created_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(CollectionSummary {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                description: row.get(2)?,
                color: row.get(3)?,
                request_count: row.get::<_, i64>(6)? as usize,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Inserts a request when `id` is `None`, otherwise updates it in place
    /// (which may also move it to another collection).
    pub fn save_request(
        &self,
        id: Option<Uuid>,
        collection_id: Uuid,
        name: &str,
        request: &ManualRequest,
    ) -> Result<SavedRequest, StoreError> {
        let connection = self.connection()?;
        let timestamp = now();
        let payload = serde_json::to_string(request)?;
        let id = match id {
            Some(id) => {
                connection.execute(
                    "UPDATE requests
                     SET collection_id = ?1, name = ?2, method = ?3, url = ?4,
                         payload_json = ?5, updated_at = ?6
                     WHERE id = ?7",
                    params![
                        collection_id.to_string(),
                        name,
                        request.method,
                        request.url,
                        payload,
                        timestamp,
                        id.to_string()
                    ],
                )?;
                id
            }
            None => {
                let id = Uuid::new_v4();
                connection.execute(
                    "INSERT INTO requests
                     (id, collection_id, name, method, url, payload_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    params![
                        id.to_string(),
                        collection_id.to_string(),
                        name,
                        request.method,
                        request.url,
                        payload,
                        timestamp
                    ],
                )?;
                id
            }
        };
        let created_at = connection
            .query_row(
                "SELECT created_at FROM requests WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| timestamp.clone());
        Ok(SavedRequest {
            id,
            collection_id,
            name: name.to_string(),
            request: request.clone(),
            created_at,
            updated_at: timestamp,
        })
    }

    pub fn list_requests(&self, collection_id: Uuid) -> Result<Vec<SavedRequest>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, payload_json, created_at, updated_at
             FROM requests WHERE collection_id = ?1
             ORDER BY position, created_at",
        )?;
        let rows = statement.query_map(params![collection_id.to_string()], |row| {
            let payload: String = row.get(2)?;
            let request = serde_json::from_str(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(SavedRequest {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                collection_id,
                name: row.get(1)?,
                request,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn delete_request(&self, id: Uuid) -> Result<(), StoreError> {
        self.connection()?.execute(
            "DELETE FROM requests WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    // ---- async variants (spawn_blocking, same serialization as captures) ----

    pub async fn create_collection_async(
        self: &std::sync::Arc<Self>,
        name: &str,
        description: &str,
        color: &str,
    ) -> Result<CollectionSummary, StoreError> {
        let database = self.clone();
        let (name, description, color) =
            (name.to_string(), description.to_string(), color.to_string());
        tokio::task::spawn_blocking(move || database.create_collection(&name, &description, &color))
            .await?
    }

    pub async fn rename_collection_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
        name: &str,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || database.rename_collection(id, &name)).await?
    }

    pub async fn delete_collection_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.delete_collection(id)).await?
    }

    pub async fn list_collections_async(
        self: &std::sync::Arc<Self>,
    ) -> Result<Vec<CollectionSummary>, StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.list_collections()).await?
    }

    pub async fn save_request_async(
        self: &std::sync::Arc<Self>,
        id: Option<Uuid>,
        collection_id: Uuid,
        name: &str,
        request: &ManualRequest,
    ) -> Result<SavedRequest, StoreError> {
        let database = self.clone();
        let (name, request) = (name.to_string(), request.clone());
        tokio::task::spawn_blocking(move || {
            database.save_request(id, collection_id, &name, &request)
        })
        .await?
    }

    pub async fn list_requests_async(
        self: &std::sync::Arc<Self>,
        collection_id: Uuid,
    ) -> Result<Vec<SavedRequest>, StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.list_requests(collection_id)).await?
    }

    pub async fn delete_request_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.delete_request(id)).await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::model::{AuthSpec, ManualBody};

    fn collection(database: &Database, name: &str) -> CollectionSummary {
        database.create_collection(name, "", "").unwrap()
    }

    fn request(name: &str) -> ManualRequest {
        ManualRequest {
            method: "POST".into(),
            url: format!("https://api.test/{name}"),
            query: vec![],
            headers: vec![],
            body: ManualBody::Raw {
                media_type: Some("application/json".into()),
                text: format!("{{\"name\":\"{name}\"}}"),
            },
            auth: AuthSpec::Bearer {
                token: "tok".into(),
            },
        }
    }

    #[test]
    fn collections_crud_round_trip() {
        let database = Database::open_in_memory().unwrap();
        let created = collection(&database, "Payments");
        assert_eq!(created.request_count, 0);
        assert_eq!(database.list_collections().unwrap().len(), 1);

        database.rename_collection(created.id, "Billing").unwrap();
        let listed = database.list_collections().unwrap();
        assert_eq!(listed[0].name, "Billing");
        assert_eq!(listed[0].id, created.id);

        database.delete_collection(created.id).unwrap();
        assert!(database.list_collections().unwrap().is_empty());
    }

    #[test]
    fn saved_requests_round_trip_with_full_payload() {
        let database = Database::open_in_memory().unwrap();
        let collection = collection(&database, "API");
        let saved = database
            .save_request(None, collection.id, "Create item", &request("item"))
            .unwrap();
        assert_eq!(saved.name, "Create item");

        let listed = database.list_requests(collection.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].request.method, "POST");
        assert_eq!(
            listed[0].request.body,
            ManualBody::Raw {
                media_type: Some("application/json".into()),
                text: "{\"name\":\"item\"}".into()
            }
        );
        assert_eq!(
            listed[0].request.auth,
            AuthSpec::Bearer {
                token: "tok".into()
            }
        );
    }

    #[test]
    fn saving_with_an_id_updates_in_place_and_can_move_collections() {
        let database = Database::open_in_memory().unwrap();
        let first = collection(&database, "First");
        let second = collection(&database, "Second");
        let saved = database
            .save_request(None, first.id, "Login", &request("login"))
            .unwrap();

        let updated = database
            .save_request(Some(saved.id), second.id, "Login v2", &request("login"))
            .unwrap();
        assert_eq!(updated.id, saved.id);
        assert_eq!(updated.collection_id, second.id);

        assert!(database.list_requests(first.id).unwrap().is_empty());
        let listed = database.list_requests(second.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Login v2");
    }

    #[test]
    fn deleting_a_collection_cascades_to_its_requests() {
        let database = Database::open_in_memory().unwrap();
        let collection = collection(&database, "Temp");
        database
            .save_request(None, collection.id, "A", &request("a"))
            .unwrap();
        database
            .save_request(None, collection.id, "B", &request("b"))
            .unwrap();
        assert_eq!(database.list_requests(collection.id).unwrap().len(), 2);

        database.delete_collection(collection.id).unwrap();
        assert!(database.list_requests(collection.id).unwrap().is_empty());
        assert_eq!(
            database
                .connection()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM requests", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn list_collections_reports_request_counts() {
        let database = Database::open_in_memory().unwrap();
        let empty = collection(&database, "Empty");
        let full = collection(&database, "Full");
        database
            .save_request(None, full.id, "A", &request("a"))
            .unwrap();
        let listed = database.list_collections().unwrap();
        assert_eq!(listed.len(), 2);
        let full_summary = listed.iter().find(|c| c.id == full.id).unwrap();
        let empty_summary = listed.iter().find(|c| c.id == empty.id).unwrap();
        assert_eq!(full_summary.request_count, 1);
        assert_eq!(empty_summary.request_count, 0);
    }
}
