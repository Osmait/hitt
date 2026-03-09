use std::collections::HashMap;

use super::chain::{evaluate_condition, extract_values, RequestChain};
use super::client::HttpClient;
use super::collection::Collection;
use super::environment::Environment;
use super::response::Response;
use super::variables::VariableResolver;

/// Outcome of a single chain step execution.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum StepOutcome {
    Success {
        step_index: usize,
        request_name: String,
        status: u16,
        duration_ms: u64,
        extracted: HashMap<String, String>,
        response: Response,
    },
    Failed {
        step_index: usize,
        error: String,
    },
    Skipped {
        step_index: usize,
        reason: String,
    },
}

/// Execute a request chain step by step, calling `on_step` after each step.
///
/// Returns the final set of extracted variables accumulated across all steps.
pub async fn execute_chain(
    client: &HttpClient,
    collection: &Collection,
    chain: &RequestChain,
    environment: Option<&Environment>,
    mut on_step: impl FnMut(&StepOutcome),
) -> HashMap<String, String> {
    let mut extracted_variables: HashMap<String, String> = HashMap::new();
    let mut last_response: Option<Response> = None;

    for (step_index, step) in chain.steps.iter().enumerate() {
        // Check condition
        if let Some(ref condition) = step.condition {
            if !evaluate_condition(condition, last_response.as_ref(), &extracted_variables) {
                let outcome = StepOutcome::Skipped {
                    step_index,
                    reason: "Condition not met".into(),
                };
                on_step(&outcome);
                continue;
            }
        }

        // Find request
        let Some(request) = collection.find_request(&step.request_id) else {
            let outcome = StepOutcome::Failed {
                step_index,
                error: format!("Request {} not found in collection", step.request_id),
            };
            on_step(&outcome);
            break;
        };

        let request_name = request.name.clone();

        // Build resolver with current extracted variables
        let resolver = VariableResolver::from_context(
            Some(&extracted_variables),
            &collection.variables,
            environment,
            None,
            None,
        );

        // Send request
        match client.send(request, &resolver).await {
            Ok(response) => {
                let status = response.status;
                let duration_ms = response.timing.total.as_millis() as u64;

                // Extract values
                let new_vars = extract_values(&step.extractions, &response);

                let outcome = StepOutcome::Success {
                    step_index,
                    request_name,
                    status,
                    duration_ms,
                    extracted: new_vars.clone(),
                    response: response.clone(),
                };
                on_step(&outcome);

                extracted_variables.extend(new_vars);
                last_response = Some(response);
            }
            Err(e) => {
                let outcome = StepOutcome::Failed {
                    step_index,
                    error: e.to_string(),
                };
                on_step(&outcome);
                break;
            }
        }

        // Apply delay
        if let Some(delay) = step.delay_ms {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
    }

    extracted_variables
}
