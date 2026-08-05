//! Environments and their variables: the `{{name}}` values resolved before
//! manual requests are sent. A `NULL` environment_id marks global variables;
//! the active environment overrides globals of the same name.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Database, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSummary {
    pub id: Uuid,
    pub name: String,
    pub variable_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableRecord {
    pub id: Uuid,
    /// `None` marks a global variable.
    pub environment_id: Option<Uuid>,
    pub name: String,
    pub value: String,
    pub is_secret: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// RFC3339 (the same format transactions use over the wire), so timestamps
/// are directly parseable by the frontend (`new Date`).
fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().to_string())
}

impl Database {
    pub fn create_environment(&self, name: &str) -> Result<EnvironmentSummary, StoreError> {
        let id = Uuid::new_v4();
        let timestamp = now();
        self.connection()?.execute(
            "INSERT INTO composer_environments (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            params![id.to_string(), name, timestamp],
        )?;
        Ok(EnvironmentSummary {
            id,
            name: name.to_string(),
            variable_count: 0,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        })
    }

    pub fn rename_environment(&self, id: Uuid, name: &str) -> Result<(), StoreError> {
        self.connection()?.execute(
            "UPDATE composer_environments SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn delete_environment(&self, id: Uuid) -> Result<(), StoreError> {
        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM variables WHERE environment_id = ?1",
            params![id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM composer_environments WHERE id = ?1",
            params![id.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_environments(&self) -> Result<Vec<EnvironmentSummary>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT e.id, e.name, e.created_at, e.updated_at, COUNT(v.id) AS variable_count
             FROM composer_environments e
             LEFT JOIN variables v ON v.environment_id = e.id
             GROUP BY e.id
             ORDER BY e.created_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(EnvironmentSummary {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                variable_count: row.get::<_, i64>(4)? as usize,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Lists variables of one environment (`Some`) or the global scope (`None`).
    pub fn list_variables(
        &self,
        environment_id: Option<Uuid>,
    ) -> Result<Vec<VariableRecord>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, value, is_secret, created_at, updated_at
             FROM variables WHERE environment_id IS ?1
             ORDER BY created_at",
        )?;
        let environment = environment_id.map(|id| id.to_string());
        let rows = statement.query_map(params![environment], move |row| {
            Ok(VariableRecord {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                environment_id,
                name: row.get(1)?,
                value: row.get(2)?,
                is_secret: row.get::<_, bool>(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Inserts a variable when `id` is `None`, otherwise updates it in place
    /// (which may also move it to another scope).
    pub fn save_variable(
        &self,
        id: Option<Uuid>,
        environment_id: Option<Uuid>,
        name: &str,
        value: &str,
        is_secret: bool,
    ) -> Result<VariableRecord, StoreError> {
        let connection = self.connection()?;
        let timestamp = now();
        let environment = environment_id.map(|id| id.to_string());
        let id = match id {
            Some(id) => {
                connection.execute(
                    "UPDATE variables
                     SET environment_id = ?1, name = ?2, value = ?3, is_secret = ?4, updated_at = ?5
                     WHERE id = ?6",
                    params![
                        environment,
                        name,
                        value,
                        is_secret,
                        timestamp,
                        id.to_string()
                    ],
                )?;
                id
            }
            None => {
                let id = Uuid::new_v4();
                connection.execute(
                    "INSERT INTO variables
                     (id, environment_id, name, value, is_secret, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![
                        id.to_string(),
                        environment,
                        name,
                        value,
                        is_secret,
                        timestamp
                    ],
                )?;
                id
            }
        };
        let created_at = connection
            .query_row(
                "SELECT created_at FROM variables WHERE id = ?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| timestamp.clone());
        Ok(VariableRecord {
            id,
            environment_id,
            name: name.to_string(),
            value: value.to_string(),
            is_secret,
            created_at,
            updated_at: timestamp,
        })
    }

    pub fn delete_variable(&self, id: Uuid) -> Result<(), StoreError> {
        self.connection()?.execute(
            "DELETE FROM variables WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    // ---- async variants (spawn_blocking, same serialization as captures) ----

    pub async fn create_environment_async(
        self: &std::sync::Arc<Self>,
        name: &str,
    ) -> Result<EnvironmentSummary, StoreError> {
        let database = self.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || database.create_environment(&name)).await?
    }

    pub async fn rename_environment_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
        name: &str,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || database.rename_environment(id, &name)).await?
    }

    pub async fn delete_environment_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.delete_environment(id)).await?
    }

    pub async fn list_environments_async(
        self: &std::sync::Arc<Self>,
    ) -> Result<Vec<EnvironmentSummary>, StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.list_environments()).await?
    }

    pub async fn list_variables_async(
        self: &std::sync::Arc<Self>,
        environment_id: Option<Uuid>,
    ) -> Result<Vec<VariableRecord>, StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.list_variables(environment_id)).await?
    }

    pub async fn save_variable_async(
        self: &std::sync::Arc<Self>,
        id: Option<Uuid>,
        environment_id: Option<Uuid>,
        name: &str,
        value: &str,
        is_secret: bool,
    ) -> Result<VariableRecord, StoreError> {
        let database = self.clone();
        let (name, value) = (name.to_string(), value.to_string());
        tokio::task::spawn_blocking(move || {
            database.save_variable(id, environment_id, &name, &value, is_secret)
        })
        .await?
    }

    pub async fn delete_variable_async(
        self: &std::sync::Arc<Self>,
        id: Uuid,
    ) -> Result<(), StoreError> {
        let database = self.clone();
        tokio::task::spawn_blocking(move || database.delete_variable(id)).await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environments_crud_round_trip() {
        let database = Database::open_in_memory().unwrap();
        let dev = database.create_environment("Dev").unwrap();
        let staging = database.create_environment("Staging").unwrap();
        assert_eq!(database.list_environments().unwrap().len(), 2);

        database.rename_environment(dev.id, "Local").unwrap();
        assert_eq!(database.list_environments().unwrap()[0].name, "Local");

        database.delete_environment(staging.id).unwrap();
        assert_eq!(database.list_environments().unwrap().len(), 1);
    }

    #[test]
    fn variables_round_trip_in_global_and_environment_scopes() {
        let database = Database::open_in_memory().unwrap();
        let environment = database.create_environment("Dev").unwrap();

        let global = database
            .save_variable(None, None, "host", "api.test", false)
            .unwrap();
        let scoped = database
            .save_variable(None, Some(environment.id), "host", "api.dev.test", true)
            .unwrap();

        let globals = database.list_variables(None).unwrap();
        let scoped_list = database.list_variables(Some(environment.id)).unwrap();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].name, "host");
        assert_eq!(globals[0].value, "api.test");
        assert!(!globals[0].is_secret);
        assert_eq!(scoped_list.len(), 1);
        assert_eq!(scoped_list[0].value, "api.dev.test");
        assert!(scoped_list[0].is_secret);

        // Updating in place keeps the id and can move scopes.
        let updated = database
            .save_variable(Some(scoped.id), None, "host", "api.global.test", true)
            .unwrap();
        assert_eq!(updated.id, scoped.id);
        assert!(updated.environment_id.is_none());
        assert!(
            database
                .list_variables(Some(environment.id))
                .unwrap()
                .is_empty()
        );
        // Both the original global and the moved variable are now global.
        assert_eq!(database.list_variables(None).unwrap().len(), 2);

        database.delete_variable(global.id).unwrap();
        let remaining = database.list_variables(None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].value, "api.global.test");
    }

    #[test]
    fn deleting_an_environment_cascades_to_its_variables() {
        let database = Database::open_in_memory().unwrap();
        let environment = database.create_environment("Temp").unwrap();
        database
            .save_variable(None, Some(environment.id), "a", "1", false)
            .unwrap();
        database
            .save_variable(None, Some(environment.id), "b", "2", false)
            .unwrap();

        database.delete_environment(environment.id).unwrap();
        assert!(
            database
                .list_variables(Some(environment.id))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn list_environments_reports_variable_counts() {
        let database = Database::open_in_memory().unwrap();
        let empty = database.create_environment("Empty").unwrap();
        let full = database.create_environment("Full").unwrap();
        database
            .save_variable(None, Some(full.id), "token", "x", true)
            .unwrap();
        let listed = database.list_environments().unwrap();
        assert_eq!(
            listed
                .iter()
                .find(|env| env.id == full.id)
                .unwrap()
                .variable_count,
            1
        );
        assert_eq!(
            listed
                .iter()
                .find(|env| env.id == empty.id)
                .unwrap()
                .variable_count,
            0
        );
    }
}
