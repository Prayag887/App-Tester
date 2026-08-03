use std::{sync::Arc, time::Duration};

use chrono::Utc;
use reqwest::Client;
use uuid::Uuid;

use super::{ApiQaEngine, request, variables};
use crate::{Collection, CoreError, CoreResult, ExecutionState, Run, RunOptions, RunState, Store};

impl ApiQaEngine {
    pub async fn run_collection(
        &self,
        collection: &Collection,
        options: RunOptions,
    ) -> CoreResult<Run> {
        let store = Arc::clone(&self.store);
        let collection_id = collection.id.clone();
        let requested = options.baseline_run_id.clone();
        let baseline = tokio::task::spawn_blocking(move || match requested {
            Some(id) => store.run(&id),
            None => store.eligible_baseline(&collection_id),
        })
        .await??;
        validate_baseline(collection, &options, baseline.as_ref())?;

        let mut run = Run {
            id: Uuid::new_v4().to_string(),
            collection_id: collection.id.clone(),
            collection_name: collection.name.clone(),
            environment_name: options.environment.as_ref().map(|value| value.name.clone()),
            started_at: Utc::now(),
            completed_at: None,
            state: RunState::Running,
            baseline_run_id: baseline.as_ref().map(|value| value.id.clone()),
            executions: vec![],
            pinned: false,
        };
        self.blocking({
            let run = run.clone();
            move |store| store.begin_run(&run)
        })
        .await?;

        if let Err(error) = self
            .execute_started_run(&mut run, collection, &options, baseline.as_ref())
            .await
        {
            self.fail_started_run(&mut run).await;
            return Err(error);
        }
        Ok(run)
    }

    async fn execute_started_run(
        &self,
        run: &mut Run,
        collection: &Collection,
        options: &RunOptions,
        baseline: Option<&Run>,
    ) -> CoreResult<()> {
        let mut builder = Client::builder()
            .timeout(Duration::from_millis(options.timeout_ms))
            .danger_accept_invalid_certs(options.accept_invalid_certificates);
        if let Some(proxy) = options.proxy_url.as_deref() {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        let client = builder.build()?;
        let mut variables = variables::resolve(collection, options.environment.as_ref());

        for api_request in collection.requests.iter().filter(|value| !value.disabled) {
            let previous = baseline.and_then(|value| {
                value
                    .executions
                    .iter()
                    .find(|execution| execution.request_id == api_request.id)
            });
            let execution =
                request::execute(&client, &run.id, api_request, &variables, previous).await;
            for value in &execution.extractions {
                variables.insert(value.name.clone(), value.value.clone());
            }
            let failed = matches!(
                execution.state,
                ExecutionState::TransportFailed | ExecutionState::AssertionFailed
            );
            let position = run.executions.len();
            self.blocking({
                let id = run.id.clone();
                let value = execution.clone();
                move |store| store.append_execution(&id, position, &value)
            })
            .await?;
            run.executions.push(execution);
            if failed && options.stop_on_error {
                break;
            }
        }

        run.completed_at = Some(Utc::now());
        run.state = final_state(run);
        self.blocking({
            let run = run.clone();
            move |store| store.finish_run(&run)
        })
        .await?;
        // Retention is maintenance; failure must not invalidate persisted run completion.
        if let Ok(policy) = self.blocking(|store| store.retention_policy()).await {
            let _ = self
                .blocking(move |store| store.cleanup_history(&policy))
                .await;
        }
        Ok(())
    }

    async fn fail_started_run(&self, run: &mut Run) {
        run.completed_at = Some(Utc::now());
        run.state = RunState::Failed;
        let failed = run.clone();
        let _ = self.blocking(move |store| store.finish_run(&failed)).await;
    }

    pub async fn run_saved(
        &self,
        collection_id: String,
        request_id: Option<String>,
        environment_id: Option<String>,
        mut options: RunOptions,
    ) -> CoreResult<Run> {
        let store = Arc::clone(&self.store);
        let mut collection = tokio::task::spawn_blocking(move || store.collection(&collection_id))
            .await??
            .ok_or(CoreError::NotFound("collection"))?;
        if let Some(request_id) = request_id {
            collection
                .requests
                .retain(|request| request.id == request_id);
            if collection.requests.is_empty() {
                return Err(CoreError::NotFound("request"));
            }
        }
        if let Some(environment_id) = environment_id {
            let store = Arc::clone(&self.store);
            options.environment =
                tokio::task::spawn_blocking(move || store.environment(&environment_id)).await??;
            if options.environment.is_none() {
                return Err(CoreError::NotFound("environment"));
            }
        }
        self.run_collection(&collection, options).await
    }

    async fn blocking<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Store) -> CoreResult<T> + Send + 'static,
    ) -> CoreResult<T> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || operation(&store)).await?
    }
}

fn validate_baseline(
    collection: &Collection,
    options: &RunOptions,
    baseline: Option<&Run>,
) -> CoreResult<()> {
    if options.baseline_run_id.is_some() && baseline.is_none() {
        return Err(CoreError::NotFound("baseline run"));
    }
    if options.baseline_run_id.is_some()
        && baseline.is_some_and(|run| {
            !matches!(
                run.state,
                RunState::Completed | RunState::CompletedWithFindings
            )
        })
    {
        return Err(CoreError::InvalidInput(
            "baseline run must be completed".into(),
        ));
    }
    if baseline.is_some_and(|run| run.collection_id != collection.id) {
        return Err(CoreError::InvalidInput(
            "baseline run belongs to another collection".into(),
        ));
    }
    Ok(())
}

fn final_state(run: &Run) -> RunState {
    if run.executions.iter().any(|value| {
        matches!(
            value.state,
            ExecutionState::TransportFailed | ExecutionState::AssertionFailed
        )
    }) {
        RunState::Failed
    } else if run
        .executions
        .iter()
        .any(|value| value.state == ExecutionState::Changed)
    {
        RunState::CompletedWithFindings
    } else {
        RunState::Completed
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
