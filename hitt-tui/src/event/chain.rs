use uuid::Uuid;

use crate::app::{App, AppMode, FocusArea, NotificationKind};

use super::{AppEvent, ChainStepEvent};

pub fn start_chain_execution(
    app: &mut App,
    chain: hitt_core::core::chain::RequestChain,
    coll_idx: usize,
) {
    use hitt_core::core::chain::ChainExecutionState;

    // A new run replaces the previous one: stop it so its events can't leak in.
    if let Some(handle) = app.chain_abort.take() {
        handle.abort();
    }
    let run_id = Uuid::new_v4();
    app.chain_run_id = Some(run_id);
    app.chain_cancelled = false;

    let mut state = ChainExecutionState::new(&chain);
    state.running = true;
    app.active_chain = Some(state);
    app.active_chain_def = Some(chain.clone());
    app.active_chain_coll_idx = Some(coll_idx);
    app.chain_scroll = 0;
    app.mode = AppMode::ChainEditor;
    if app.focus != FocusArea::ChainSteps {
        app.chain_return_focus = app.focus;
    }
    app.focus = FocusArea::ChainSteps;

    // Clone what the async task needs
    let http_client = app.http_client.clone();
    let collection = app.collections[coll_idx].clone();
    let environment = app.active_environment().cloned();
    let event_tx = app.event_tx();
    let chain_def = chain;

    let handle = tokio::spawn(async move {
        run_chain_task(
            http_client,
            collection,
            chain_def,
            environment,
            event_tx,
            run_id,
        )
        .await;
    });
    app.chain_abort = Some(handle.abort_handle());
}

/// Stop the running chain (keeping finished steps). Returns false if no
/// chain is running.
pub(super) fn cancel_chain_run(app: &mut App) -> bool {
    use hitt_core::core::chain::ChainStepStatus;

    let Some(state) = app.active_chain.as_mut().filter(|s| s.running) else {
        return false;
    };
    if let Some(handle) = app.chain_abort.take() {
        handle.abort();
    }
    // Late events of the aborted task are dropped by `handle_chain_step_event`.
    app.chain_run_id = None;
    app.chain_cancelled = true;
    state.running = false;
    for status in &mut state.step_statuses {
        if matches!(status, ChainStepStatus::Running | ChainStepStatus::Pending) {
            *status = ChainStepStatus::Skipped {
                reason: "Cancelled".into(),
            };
        }
    }
    app.notify("Chain cancelled".into(), NotificationKind::Info);
    true
}

async fn run_chain_task(
    http_client: hitt_core::core::client::HttpClient,
    collection: hitt_core::core::collection::Collection,
    chain: hitt_core::core::chain::RequestChain,
    environment: Option<hitt_core::core::environment::Environment>,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    run_id: Uuid,
) {
    use hitt_core::core::chain_executor::{self, StepOutcome};

    let send = |event: ChainStepEvent| {
        let _ = event_tx.send(AppEvent::ChainStepComplete { run_id, event });
    };
    // The executor only reports finished steps; a step starts right when the
    // previous one finishes (or at the beginning), so mark it Running then.
    if !chain.steps.is_empty() {
        send(ChainStepEvent::Running { step_index: 0 });
    }

    chain_executor::execute_chain(
        &http_client,
        &collection,
        &chain,
        environment.as_ref(),
        |outcome| {
            let event = match outcome {
                StepOutcome::Success {
                    step_index,
                    status,
                    duration_ms,
                    extracted,
                    ..
                } => ChainStepEvent::Success {
                    step_index: *step_index,
                    status: *status,
                    duration_ms: *duration_ms,
                    extracted: extracted.clone(),
                },
                StepOutcome::Failed { step_index, error } => {
                    // Replace the core's raw-UUID message with something readable
                    // when the step's request was deleted from the collection.
                    let missing = chain
                        .steps
                        .get(*step_index)
                        .is_some_and(|step| collection.find_request(&step.request_id).is_none());
                    ChainStepEvent::Failed {
                        step_index: *step_index,
                        error: if missing {
                            "request not found in collection (deleted?)".to_string()
                        } else {
                            error.clone()
                        },
                    }
                }
                StepOutcome::Skipped { step_index, reason } => ChainStepEvent::Skipped {
                    step_index: *step_index,
                    reason: reason.clone(),
                },
            };
            // A failed step ends the chain; otherwise the next one starts now.
            let next = match &event {
                ChainStepEvent::Success { step_index, .. }
                | ChainStepEvent::Skipped { step_index, .. } => Some(step_index + 1),
                _ => None,
            };
            send(event);
            if let Some(next) = next.filter(|n| *n < chain.steps.len()) {
                send(ChainStepEvent::Running { step_index: next });
            }
        },
    )
    .await;

    send(ChainStepEvent::Complete);
}

pub(super) fn handle_chain_step_event(app: &mut App, run_id: Uuid, event: ChainStepEvent) {
    use hitt_core::core::chain::ChainStepStatus;

    // Events of a cancelled / replaced run.
    if app.chain_run_id != Some(run_id) {
        return;
    }
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
            app.chain_run_id = None;
            app.chain_abort = None;
            let mut failed = 0;
            if let Some(ref mut state) = app.active_chain {
                state.running = false;
                failed = chain_failed_steps(state);
            }
            if failed == 0 {
                app.notify("Chain execution complete".into(), NotificationKind::Success);
            } else {
                app.notify(
                    format!("Chain finished with {failed} failed step(s)"),
                    NotificationKind::Error,
                );
            }
        }
    }
}

/// Number of steps that ended in `Failed`.
pub fn chain_failed_steps(state: &hitt_core::core::chain::ChainExecutionState) -> usize {
    state
        .step_statuses
        .iter()
        .filter(|s| matches!(s, hitt_core::core::chain::ChainStepStatus::Failed { .. }))
        .count()
}
