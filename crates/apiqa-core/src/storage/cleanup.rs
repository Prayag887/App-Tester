use std::collections::{HashMap, HashSet};

use chrono::{Duration, Utc};

use super::Store;
use crate::{CleanupResult, CoreResult, RetentionPolicy};

impl Store {
    pub fn cleanup_history(&self, policy: &RetentionPolicy) -> CoreResult<CleanupResult> {
        let cutoff = (Utc::now() - Duration::days(policy.days as i64)).to_rfc3339();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let mut deleted_runs = transaction.execute(
            "DELETE FROM runs WHERE started_at < ?1 AND state != 'Running'
             AND COALESCE(json_extract(data, '$.pinned'), 0) != 1",
            [cutoff],
        )?;

        let mut references = run_blob_references(&transaction)?;
        let blob_sizes = blob_sizes(&transaction)?;
        if let Some(max_bytes) = policy.max_bytes {
            let mut reference_counts = HashMap::<String, usize>::new();
            for hashes in references.values() {
                for hash in hashes {
                    *reference_counts.entry(hash.clone()).or_default() += 1;
                }
            }
            let mut referenced_bytes = reference_counts
                .keys()
                .map(|hash| blob_sizes.get(hash).copied().unwrap_or(0))
                .sum::<u64>();
            let candidates = deletion_candidates(&transaction)?;
            for id in candidates {
                if referenced_bytes <= max_bytes {
                    break;
                }
                if let Some(hashes) = references.remove(&id) {
                    for hash in hashes {
                        let count = reference_counts.get_mut(&hash).expect("known reference");
                        *count -= 1;
                        if *count == 0 {
                            referenced_bytes = referenced_bytes
                                .saturating_sub(blob_sizes.get(&hash).copied().unwrap_or(0));
                        }
                    }
                }
                deleted_runs += transaction.execute("DELETE FROM runs WHERE id=?1", [id])?;
            }
        }

        let referenced = references.into_values().flatten().collect::<HashSet<_>>();
        let (mut deleted_blobs, mut reclaimed_bytes) = (0, 0);
        for (hash, bytes) in blob_sizes {
            if !referenced.contains(&hash) {
                deleted_blobs +=
                    transaction.execute("DELETE FROM response_blobs WHERE hash=?1", [hash])?;
                reclaimed_bytes += bytes;
            }
        }
        transaction.commit()?;
        Ok(CleanupResult {
            deleted_runs,
            deleted_blobs,
            reclaimed_bytes,
        })
    }
}

fn run_blob_references(
    connection: &rusqlite::Connection,
) -> CoreResult<HashMap<String, HashSet<String>>> {
    let mut references = HashMap::<String, HashSet<String>>::new();
    for sql in [
        "SELECT id,data FROM runs",
        "SELECT run_id,data FROM run_executions",
    ] {
        let mut statement = connection.prepare(sql)?;
        let values = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (run_id, value) in values {
            collect_hashes(
                &serde_json::from_str(&value)?,
                references.entry(run_id).or_default(),
            );
        }
    }
    Ok(references)
}

fn collect_hashes(value: &serde_json::Value, hashes: &mut HashSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(hash) = map.get("body_hash").and_then(|value| value.as_str()) {
                hashes.insert(hash.to_owned());
            }
            for value in map.values() {
                collect_hashes(value, hashes);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_hashes(value, hashes);
            }
        }
        _ => {}
    }
}

fn blob_sizes(connection: &rusqlite::Connection) -> CoreResult<HashMap<String, u64>> {
    let mut statement = connection.prepare("SELECT hash,length(compressed) FROM response_blobs")?;
    Ok(statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<rusqlite::Result<_>>()?)
}

fn deletion_candidates(connection: &rusqlite::Connection) -> CoreResult<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT id FROM runs WHERE state != 'Running'
         AND COALESCE(json_extract(data, '$.pinned'), 0) != 1 ORDER BY started_at",
    )?;
    Ok(statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::{Collection, ExecutionState, RequestExecution, ResponseSnapshot, Run, RunState};

    #[test]
    fn byte_cleanup_keeps_shared_blob_until_last_referencing_run_is_deleted() {
        let store = Store::open(":memory:").unwrap();
        store
            .save_collection(&Collection {
                id: "collection".into(),
                name: "collection".into(),
                requests: vec![],
                variables: vec![],
                imported_at: Utc::now(),
                import_warnings: vec![],
            })
            .unwrap();
        for id in ["first", "second"] {
            let run = Run {
                id: id.into(),
                collection_id: "collection".into(),
                collection_name: "collection".into(),
                environment_name: None,
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                state: RunState::Completed,
                baseline_run_id: None,
                executions: vec![],
                pinned: false,
            };
            store.begin_run(&run).unwrap();
            store
                .append_execution(
                    id,
                    0,
                    &RequestExecution {
                        id: format!("execution-{id}"),
                        run_id: id.into(),
                        request_id: "request".into(),
                        request_name: "request".into(),
                        state: ExecutionState::Passed,
                        started_at: Utc::now(),
                        response: Some(ResponseSnapshot {
                            status: 200,
                            headers: vec![],
                            content_type: None,
                            body: "shared response".into(),
                            body_hash: None,
                            body_size: 15,
                            duration_ms: 1,
                            truncated: false,
                        }),
                        error: None,
                        comparison: None,
                        assertions: vec![],
                        extractions: vec![],
                    },
                )
                .unwrap();
        }

        let result = store
            .cleanup_history(&RetentionPolicy {
                days: 365,
                max_bytes: Some(0),
            })
            .unwrap();

        assert_eq!(result.deleted_runs, 2);
        assert_eq!(result.deleted_blobs, 1);
        assert!(store.runs(None).unwrap().is_empty());
    }
}
