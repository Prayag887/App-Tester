use std::{collections::HashMap, time::Instant};

use chrono::Utc;
use reqwest::Client;
use uuid::Uuid;

use super::{evaluation, transport};
use crate::{ApiRequest, ComparisonOptions, ExecutionState, RequestExecution, compare_responses};

pub(super) async fn execute(
    client: &Client,
    run_id: &str,
    request: &ApiRequest,
    variables: &HashMap<String, String>,
    baseline: Option<&RequestExecution>,
) -> RequestExecution {
    let started_at = Utc::now();
    let started = Instant::now();
    match transport::send(client, request, variables).await {
        Ok(mut response) => {
            response.duration_ms = started.elapsed().as_millis() as u64;
            let assertions = evaluation::assertions(request, &response);
            let extractions = evaluation::extractions(request, &response);
            let assertion_failed = assertions.iter().any(|result| !result.passed);
            let comparison = baseline
                .and_then(|execution| execution.response.as_ref())
                .map(|previous| {
                    compare_responses(previous, &response, &ComparisonOptions::default())
                });
            let changed = comparison
                .as_ref()
                .is_some_and(|comparison| comparison.changed);
            RequestExecution {
                id: Uuid::new_v4().to_string(),
                run_id: run_id.to_string(),
                request_id: request.id.clone(),
                request_name: request.name.clone(),
                state: if assertion_failed {
                    ExecutionState::AssertionFailed
                } else if changed {
                    ExecutionState::Changed
                } else {
                    ExecutionState::Passed
                },
                started_at,
                response: Some(response),
                error: None,
                comparison,
                assertions,
                extractions,
            }
        }
        Err(error) => RequestExecution {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.to_string(),
            request_id: request.id.clone(),
            request_name: request.name.clone(),
            state: ExecutionState::TransportFailed,
            started_at,
            response: None,
            error: Some(format!("{error:#}")),
            comparison: None,
            assertions: vec![],
            extractions: vec![],
        },
    }
}
