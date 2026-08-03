use rusqlite::{Connection, params};

use super::Store;
use crate::{Collection, CoreResult, Environment};

impl Store {
    pub fn save_collection(&self, collection: &Collection) -> CoreResult<()> {
        let connection = self.connection()?;
        save_collection(&connection, collection)
    }

    pub fn save_environment(&self, environment: &Environment) -> CoreResult<()> {
        let connection = self.connection()?;
        save_environment(&connection, environment)
    }

    pub fn save_project(
        &self,
        collection: &Collection,
        environments: &[Environment],
    ) -> CoreResult<()> {
        self.save_workspace(std::slice::from_ref(collection), environments)
    }

    pub fn save_workspace(
        &self,
        collections: &[Collection],
        environments: &[Environment],
    ) -> CoreResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for collection in collections {
            save_collection(&transaction, collection)?;
        }
        for environment in environments {
            save_environment(&transaction, environment)?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn save_collection(connection: &Connection, collection: &Collection) -> CoreResult<()> {
    connection.execute(
        "INSERT INTO collections(id,name,data,imported_at) VALUES(?1,?2,?3,?4)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name,data=excluded.data,imported_at=excluded.imported_at",
        params![
            collection.id,
            collection.name,
            serde_json::to_string(collection)?,
            collection.imported_at.to_rfc3339()
        ],
    )?;
    Ok(())
}

fn save_environment(connection: &Connection, environment: &Environment) -> CoreResult<()> {
    connection.execute(
        "INSERT INTO environments(id,name,data) VALUES(?1,?2,?3)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name,data=excluded.data",
        params![
            environment.id,
            environment.name,
            serde_json::to_string(environment)?
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn collection(id: &str) -> Collection {
        Collection {
            id: id.into(),
            name: id.into(),
            requests: vec![],
            variables: vec![],
            imported_at: Utc::now(),
            import_warnings: vec![],
        }
    }

    #[test]
    fn workspace_save_rolls_back_every_record_on_failure() {
        let store = Store::open(":memory:").unwrap();
        store
            .connection()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_environment BEFORE INSERT ON environments
                 BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
            )
            .unwrap();

        let result = store.save_workspace(
            &[collection("first"), collection("second")],
            &[Environment {
                id: "env".into(),
                name: "env".into(),
                variables: vec![],
            }],
        );

        assert!(result.is_err());
        assert!(store.collections().unwrap().is_empty());
        assert!(store.environments().unwrap().is_empty());
    }
}
