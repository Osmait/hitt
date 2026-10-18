use uuid::Uuid;

use crate::core::chain_executor::{execute_chain, StepOutcome};
use crate::core::request::Request;
use crate::testing::{diff, load_test};
use crate::{ChainRunResult, ChainStepResult, ChainStepStatusTag, Core, CoreEvent};

pub(crate) async fn run_load_test(
    _core: &mut Core,
    request: Request,
    total: u32,
    concurrency: u32,
    timeout_ms: u64,
) -> Vec<CoreEvent> {
    let config = load_test::LoadTestConfig {
        request_id: request.id,
        total_requests: total,
        concurrency,
        ramp_up_seconds: None,
        timeout_ms,
    };
    let resolver = crate::core::variables::VariableResolver::new();
    match load_test::run_load_test(&config, &request, &resolver).await {
        Ok(result) => vec![CoreEvent::LoadTestCompleted(Box::new(result))],
        Err(e) => vec![CoreEvent::Error(e.to_string())],
    }
}

pub(crate) fn run_diff(left: String, right: String) -> Vec<CoreEvent> {
    let result = diff::diff_text(&left, &right);
    vec![CoreEvent::DiffCompleted(Box::new(result))]
}

pub(crate) async fn run_chain(
    core: &mut Core,
    collection_id: Uuid,
    chain_name: String,
) -> Vec<CoreEvent> {
    let Some(collection) = core
        .collections
        .iter()
        .find(|c| c.id == collection_id)
        .cloned()
    else {
        return vec![CoreEvent::Error(format!(
            "Collection {collection_id} not found"
        ))];
    };
    let Some(chain) = collection
        .chains
        .iter()
        .find(|c| c.name == chain_name)
        .cloned()
    else {
        return vec![CoreEvent::Error(format!("Chain '{chain_name}' not found"))];
    };
    let env = core
        .active_environment
        .as_ref()
        .and_then(|n| core.environments.iter().find(|e| &e.name == n))
        .cloned();

    let mut step_results: Vec<ChainStepResult> = Vec::new();
    let final_vars = execute_chain(&core.client, &collection, &chain, env.as_ref(), |outcome| {
        step_results.push(convert_outcome(outcome));
    })
    .await;

    let result = ChainRunResult {
        steps: step_results,
        final_vars,
    };
    vec![CoreEvent::ChainCompleted(Box::new(result))]
}

fn convert_outcome(outcome: &StepOutcome) -> ChainStepResult {
    match outcome {
        StepOutcome::Success {
            step_index,
            request_name,
            status,
            duration_ms,
            extracted,
            ..
        } => ChainStepResult {
            step_index: *step_index as u32,
            status: ChainStepStatusTag::Success,
            request_name: Some(request_name.clone()),
            http_status: Some(*status),
            duration_ms: Some(*duration_ms),
            extracted: extracted.clone(),
            error: None,
        },
        StepOutcome::Failed { step_index, error } => ChainStepResult {
            step_index: *step_index as u32,
            status: ChainStepStatusTag::Failed,
            request_name: None,
            http_status: None,
            duration_ms: None,
            extracted: Default::default(),
            error: Some(error.clone()),
        },
        StepOutcome::Skipped { step_index, reason } => ChainStepResult {
            step_index: *step_index as u32,
            status: ChainStepStatusTag::Skipped,
            request_name: None,
            http_status: None,
            duration_ms: None,
            extracted: Default::default(),
            error: Some(reason.clone()),
        },
    }
}
