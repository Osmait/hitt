use crate::app::{App, AppMode, FocusArea, NotificationKind};

use super::{AppEvent, ChainStepEvent};

pub fn start_chain_execution(
    app: &mut App,
    chain: crate::core::chain::RequestChain,
    coll_idx: usize,
) {
    use crate::core::chain::ChainExecutionState;

    let mut state = ChainExecutionState::new(&chain);
    state.running = true;
    app.active_chain = Some(state);
    app.active_chain_def = Some(chain.clone());
    app.active_chain_coll_idx = Some(coll_idx);
    app.chain_scroll = 0;
    app.mode = AppMode::ChainEditor;
    app.focus = FocusArea::ChainSteps;

    // Clone what the async task needs
    let http_client = app.http_client.clone();
    let collection = app.collections[coll_idx].clone();
    let event_tx = app.event_tx();
    let chain_def = chain;

    tokio::spawn(async move {
        run_chain_task(http_client, collection, chain_def, event_tx).await;
    });
}

async fn run_chain_task(
    http_client: crate::core::client::HttpClient,
    collection: crate::core::collection::Collection,
    chain: crate::core::chain::RequestChain,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
) {
    use crate::core::chain_executor::{self, StepOutcome};

    chain_executor::execute_chain(&http_client, &collection, &chain, None, |outcome| {
        let event = match outcome {
            StepOutcome::Success {
                step_index,
                status,
                duration_ms,
                extracted,
                ..
            } => {
                // Send Running first, then Success
                let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Running {
                    step_index: *step_index,
                }));
                ChainStepEvent::Success {
                    step_index: *step_index,
                    status: *status,
                    duration_ms: *duration_ms,
                    extracted: extracted.clone(),
                }
            }
            StepOutcome::Failed { step_index, error } => ChainStepEvent::Failed {
                step_index: *step_index,
                error: error.clone(),
            },
            StepOutcome::Skipped { step_index, reason } => ChainStepEvent::Skipped {
                step_index: *step_index,
                reason: reason.clone(),
            },
        };
        let _ = event_tx.send(AppEvent::ChainStepComplete(event));
    })
    .await;

    let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Complete));
}

pub(super) fn handle_chain_step_event(app: &mut App, event: ChainStepEvent) {
    use crate::core::chain::ChainStepStatus;

    match event {
        ChainStepEvent::Running { step_index } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Running;
                    state.current_step = step_index;
                }
            }
        }
        ChainStepEvent::Success {
            step_index,
            status,
            duration_ms,
            extracted,
        } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Success {
                        status,
                        duration_ms,
                    };
                    state.current_step = step_index;
                    state.extracted_variables.extend(extracted);
                }
            }
        }
        ChainStepEvent::Failed { step_index, error } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Failed {
                        error: error.clone(),
                    };
                    state.current_step = step_index;
                }
            }
            app.notify(
                format!("Chain step {} failed: {}", step_index + 1, error),
                NotificationKind::Error,
            );
        }
        ChainStepEvent::Skipped { step_index, reason } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Skipped { reason };
                    state.current_step = step_index;
                }
            }
        }
        ChainStepEvent::Complete => {
            if let Some(ref mut state) = app.active_chain {
                state.running = false;
            }
            app.notify("Chain execution complete".into(), NotificationKind::Success);
        }
    }
}
