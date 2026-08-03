mod cleanup;
mod runs;
mod saves;

use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Collection, ComparisonRule, CoreError, CoreResult, Environment, RetentionPolicy};

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> CoreResult<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(CoreError::InvalidInput(format!(
                "database integrity check failed: {integrity}"
            )));
        }
        connection.execute_batch(include_str!("schema.sql"))?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn connection(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| CoreError::InvalidInput("database lock poisoned".into()))
    }

    pub fn collections(&self) -> CoreResult<Vec<Collection>> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT data FROM collections ORDER BY imported_at DESC")?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|value| Ok(serde_json::from_str(&value)?))
            .collect()
    }

    pub fn collection(&self, id: &str) -> CoreResult<Option<Collection>> {
        let value = self
            .connection()?
            .query_row("SELECT data FROM collections WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        value
            .map(|value| Ok(serde_json::from_str(&value)?))
            .transpose()
    }

    pub fn environments(&self) -> CoreResult<Vec<Environment>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT data FROM environments ORDER BY name")?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|value| Ok(serde_json::from_str(&value)?))
            .collect()
    }

    pub fn environment(&self, id: &str) -> CoreResult<Option<Environment>> {
        let value = self
            .connection()?
            .query_row("SELECT data FROM environments WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        value
            .map(|value| Ok(serde_json::from_str(&value)?))
            .transpose()
    }

    pub fn retention_policy(&self) -> CoreResult<RetentionPolicy> {
        let value = self
            .connection()?
            .query_row(
                "SELECT value FROM settings WHERE key='retention'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default())
    }

    pub fn set_retention_policy(&self, policy: &RetentionPolicy) -> CoreResult<()> {
        if !(7..=365).contains(&policy.days) {
            return Err(CoreError::InvalidInput(
                "retention must be between 7 and 365 days".into(),
            ));
        }
        self.connection()?.execute("INSERT INTO settings(key,value) VALUES('retention',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(policy)?])?;
        Ok(())
    }

    pub fn save_comparison_rule(&self, rule: &ComparisonRule) -> CoreResult<()> {
        self.connection()?.execute("INSERT INTO comparison_rules(id,version,scope_id,created_at,data) VALUES(?1,?2,?3,?4,?5)", params![rule.id, rule.version, rule.scope_id, rule.created_at.to_rfc3339(), serde_json::to_string(rule)?])?;
        Ok(())
    }

    pub fn comparison_rules(&self, scope_id: &str) -> CoreResult<Vec<ComparisonRule>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT data FROM comparison_rules WHERE scope_id=?1 ORDER BY id,version")?;
        let values = statement
            .query_map([scope_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|value| Ok(serde_json::from_str(&value)?))
            .collect()
    }
}
