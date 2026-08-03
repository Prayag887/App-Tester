use std::io::{Cursor, Read};

use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use super::Store;
use crate::{
    CoreError, CoreResult, RequestExecution, Run, RunState, model::MAX_CAPTURED_BODY_SIZE,
};

impl Store {
    pub fn begin_run(&self, run: &Run) -> CoreResult<()> {
        let mut metadata = run.clone();
        metadata.executions.clear();
        self.connection()?.execute(
            "INSERT INTO runs(id,collection_id,started_at,state,data) VALUES(?1,?2,?3,?4,?5)",
            params![
                run.id,
                run.collection_id,
                run.started_at.to_rfc3339(),
                state_name(&run.state),
                serde_json::to_string(&metadata)?
            ],
        )?;
        Ok(())
    }

    pub fn append_execution(
        &self,
        run_id: &str,
        position: usize,
        execution: &RequestExecution,
    ) -> CoreResult<()> {
        let mut stored = execution.clone();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        store_body(&transaction, &mut stored)?;
        transaction.execute(
            "INSERT INTO run_executions(run_id,position,data) VALUES(?1,?2,?3)",
            params![run_id, position as i64, serde_json::to_string(&stored)?],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn finish_run(&self, run: &Run) -> CoreResult<()> {
        let mut metadata = run.clone();
        metadata.executions.clear();
        self.connection()?.execute(
            "UPDATE runs SET state=?2,data=?3 WHERE id=?1",
            params![
                run.id,
                state_name(&run.state),
                serde_json::to_string(&metadata)?
            ],
        )?;
        Ok(())
    }

    pub fn run(&self, id: &str) -> CoreResult<Option<Run>> {
        let connection = self.connection()?;
        let value = connection
            .query_row("SELECT data FROM runs WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        value
            .map(|value| decode_run(&connection, &value, true))
            .transpose()
    }

    pub fn runs(&self, collection_id: Option<&str>) -> CoreResult<Vec<Run>> {
        let connection = self.connection()?;
        let (sql, parameter) = match collection_id {
            Some(id) => (
                "SELECT data FROM runs WHERE collection_id=?1 ORDER BY started_at DESC",
                Some(id),
            ),
            None => ("SELECT data FROM runs ORDER BY started_at DESC", None),
        };
        let mut statement = connection.prepare(sql)?;
        let values = match parameter {
            Some(value) => statement
                .query_map([value], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            None => statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };
        values
            .into_iter()
            .map(|value| decode_run(&connection, &value, true))
            .collect()
    }

    /// Returns run metadata and executions without decompressing response bodies.
    pub fn run_summaries(&self, collection_id: Option<&str>) -> CoreResult<Vec<Run>> {
        let connection = self.connection()?;
        let (sql, parameter) = match collection_id {
            Some(id) => (
                "SELECT data FROM runs WHERE collection_id=?1 ORDER BY started_at DESC",
                Some(id),
            ),
            None => ("SELECT data FROM runs ORDER BY started_at DESC", None),
        };
        let mut statement = connection.prepare(sql)?;
        let values = match parameter {
            Some(value) => statement
                .query_map([value], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            None => statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };
        values
            .into_iter()
            .map(|value| decode_run(&connection, &value, false))
            .collect()
    }

    pub fn run_count(&self) -> CoreResult<u64> {
        Ok(self
            .connection()?
            .query_row("SELECT count(*) FROM runs", [], |row| row.get::<_, i64>(0))?
            as u64)
    }

    pub fn eligible_baseline(&self, collection_id: &str) -> CoreResult<Option<Run>> {
        let connection = self.connection()?;
        let value = connection.query_row(
            "SELECT data FROM runs WHERE collection_id=?1 AND state IN ('Completed','CompletedWithFindings') ORDER BY started_at DESC LIMIT 1",
            [collection_id], |row| row.get::<_, String>(0),
        ).optional()?;
        value
            .map(|value| decode_run(&connection, &value, true))
            .transpose()
    }

    pub fn set_run_pinned(&self, id: &str, pinned: bool) -> CoreResult<()> {
        let connection = self.connection()?;
        let updated = connection.execute(
            "UPDATE runs SET data=json_set(data, '$.pinned', CASE WHEN ?2 THEN json('true') ELSE json('false') END) WHERE id=?1",
            params![id, pinned],
        )?;
        if updated == 0 {
            return Err(CoreError::NotFound("run"));
        }
        Ok(())
    }
}

fn store_body(
    transaction: &rusqlite::Transaction<'_>,
    execution: &mut RequestExecution,
) -> CoreResult<()> {
    let Some(response) = execution.response.as_mut() else {
        return Ok(());
    };
    if response.body.is_empty() {
        return Ok(());
    }
    let hash = format!("{:x}", Sha256::digest(response.body.as_bytes()));
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM response_blobs WHERE hash=?1)",
        [&hash],
        |row| row.get(0),
    )?;
    if !exists {
        let compressed = zstd::stream::encode_all(Cursor::new(response.body.as_bytes()), 3)?;
        transaction.execute(
            "INSERT INTO response_blobs(hash,compressed,original_bytes) VALUES(?1,?2,?3)",
            params![hash, compressed, response.body.len() as i64],
        )?;
    }
    response.body_hash = Some(hash);
    response.body.clear();
    Ok(())
}

pub(super) fn decode_run(
    connection: &rusqlite::Connection,
    value: &str,
    load_bodies: bool,
) -> CoreResult<Run> {
    let mut run: Run = serde_json::from_str(value)?;
    let mut statement =
        connection.prepare("SELECT data FROM run_executions WHERE run_id=?1 ORDER BY position")?;
    let incremental = statement
        .query_map([&run.id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !incremental.is_empty() {
        run.executions = incremental
            .into_iter()
            .map(|value| serde_json::from_str(&value))
            .collect::<Result<_, _>>()?;
    }
    if load_bodies {
        for execution in &mut run.executions {
            load_body(connection, execution)?;
        }
    }
    Ok(run)
}

fn load_body(
    connection: &rusqlite::Connection,
    execution: &mut RequestExecution,
) -> CoreResult<()> {
    let Some(response) = execution.response.as_mut() else {
        return Ok(());
    };
    if !response.body.is_empty() {
        return Ok(());
    }
    let Some(hash) = response.body_hash.as_deref() else {
        return Ok(());
    };
    let stored = connection
        .query_row(
            "SELECT compressed,original_bytes FROM response_blobs WHERE hash=?1",
            [hash],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    if let Some((bytes, original_bytes)) = stored {
        let expected = usize::try_from(original_bytes)
            .map_err(|_| CoreError::InvalidInput("invalid stored response body size".into()))?;
        if expected > MAX_CAPTURED_BODY_SIZE {
            return Err(CoreError::InvalidInput(
                "stored response body exceeds capture limit".into(),
            ));
        }
        let mut decoded = Vec::with_capacity(expected);
        zstd::stream::read::Decoder::new(Cursor::new(bytes))?
            .take(MAX_CAPTURED_BODY_SIZE as u64 + 1)
            .read_to_end(&mut decoded)?;
        if decoded.len() != expected {
            return Err(CoreError::InvalidInput(
                "stored response body size mismatch".into(),
            ));
        }
        response.body = String::from_utf8(decoded)
            .map_err(|_| CoreError::InvalidInput("response body is not UTF-8".into()))?;
    }
    Ok(())
}

fn state_name(state: &RunState) -> &'static str {
    match state {
        RunState::Running => "Running",
        RunState::Completed => "Completed",
        RunState::CompletedWithFindings => "CompletedWithFindings",
        RunState::Failed => "Failed",
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rusqlite::params;

    use super::*;
    use crate::{Collection, ExecutionState, ResponseSnapshot};

    fn setup() -> Store {
        let store = Store::open(":memory:").unwrap();
        store
            .save_collection(&Collection {
                id: "c1".into(),
                name: "demo".into(),
                requests: vec![],
                variables: vec![],
                imported_at: Utc::now(),
                import_warnings: vec![],
            })
            .unwrap();
        store
    }

    fn run(id: &str, state: RunState) -> Run {
        Run {
            id: id.into(),
            collection_id: "c1".into(),
            collection_name: "demo".into(),
            environment_name: None,
            started_at: Utc::now(),
            completed_at: None,
            state,
            baseline_run_id: None,
            executions: vec![],
            pinned: false,
        }
    }

    fn execution(run_id: &str, body: &str) -> RequestExecution {
        RequestExecution {
            id: format!("e-{run_id}"),
            run_id: run_id.into(),
            request_id: "r1".into(),
            request_name: "request".into(),
            state: ExecutionState::Passed,
            started_at: Utc::now(),
            response: Some(ResponseSnapshot {
                status: 200,
                headers: vec![],
                content_type: None,
                body: body.into(),
                body_hash: None,
                body_size: body.len() as u64,
                duration_ms: 1,
                truncated: false,
            }),
            error: None,
            comparison: None,
            assertions: vec![],
            extractions: vec![],
        }
    }

    #[test]
    fn appends_executions_without_rewriting_run_metadata() {
        let store = setup();
        let value = run("new", RunState::Running);
        store.begin_run(&value).unwrap();
        let before: String = store
            .connection()
            .unwrap()
            .query_row("SELECT data FROM runs WHERE id='new'", [], |row| row.get(0))
            .unwrap();
        store
            .append_execution("new", 0, &execution("new", "same body"))
            .unwrap();
        store
            .append_execution("new", 1, &execution("new", "same body"))
            .unwrap();
        let after: String = store
            .connection()
            .unwrap()
            .query_row("SELECT data FROM runs WHERE id='new'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(store.run("new").unwrap().unwrap().executions.len(), 2);
        let blobs: i64 = store
            .connection()
            .unwrap()
            .query_row("SELECT count(*) FROM response_blobs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(blobs, 1);
        let summaries = store.run_summaries(Some("c1")).unwrap();
        assert_eq!(summaries[0].executions.len(), 2);
        assert!(summaries[0].executions.iter().all(|execution| {
            execution
                .response
                .as_ref()
                .is_none_or(|response| response.body.is_empty())
        }));
        assert_eq!(store.run_count().unwrap(), 1);
    }

    #[test]
    fn reads_shipped_embedded_execution_format() {
        let store = setup();
        let mut legacy = run("legacy", RunState::Completed);
        legacy.executions.push(execution("legacy", "inline body"));
        store
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO runs(id,collection_id,started_at,state,data) VALUES(?1,?2,?3,?4,?5)",
                params![
                    legacy.id,
                    legacy.collection_id,
                    legacy.started_at.to_rfc3339(),
                    "Completed",
                    serde_json::to_string(&legacy).unwrap()
                ],
            )
            .unwrap();
        assert_eq!(
            store.run("legacy").unwrap().unwrap().executions[0]
                .response
                .as_ref()
                .unwrap()
                .body,
            "inline body"
        );
        store.set_run_pinned("legacy", true).unwrap();
        let reloaded = store.run("legacy").unwrap().unwrap();
        assert!(reloaded.pinned);
        assert_eq!(reloaded.executions.len(), 1);
    }

    #[test]
    fn baseline_excludes_running_and_failed_runs() {
        let store = setup();
        for (id, state) in [
            ("completed", RunState::Completed),
            ("failed", RunState::Failed),
            ("running", RunState::Running),
        ] {
            store.begin_run(&run(id, state.clone())).unwrap();
            store.finish_run(&run(id, state)).unwrap();
        }
        assert_eq!(
            store.eligible_baseline("c1").unwrap().unwrap().id,
            "completed"
        );
    }
}
