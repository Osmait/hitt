//! Collection runner: `:run`, `:runexport`, sidebar `R` and the runner modal.
//!
//! The run happens on a tokio task; progress comes back as
//! `AppEvent::CollectionRun` and updates `app.runner` (see
//! `ui::widgets::runner`).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use crate::app::{App, AppMode, FocusArea, ModalKind, NotificationKind, SidebarSection};
use crate::ui::widgets::runner::{RunRow, RunView};
use crate::utils::expand_tilde;
use hitt_core::testing::collection_runner::{
    self as core_runner, RequestRunStatus, RunEvent, RunSummary, RunnerOptions,
};

use super::sidebar::{build_sidebar_items, SidebarItem};
use super::{is_text_input, AppEvent};

const DEFAULT_EXPORT_PATH: &str = "hitt-report.json";

/// Progress of a background collection run.
#[derive(Debug, Clone)]
pub enum CollectionRunEvent {
    Progress(RunEvent),
    Finished(Box<RunSummary>),
    Failed(String),
}

/// `:run [collection] [folder] [--bail]`.
///
/// Without a collection, uses the sidebar selection when the sidebar is
/// focused, else the active tab's collection, else the sidebar selection.
pub(super) async fn run_command(app: &mut App, args: Option<&str>) {
    const USAGE: &str = "Usage: :run [collection] [folder] [--bail]";
    let words = match shell_words::split(args.unwrap_or("")) {
        Ok(words) => words,
        Err(e) => {
            app.notify(format!("{USAGE} ({e})"), NotificationKind::Warning);
            return;
        }
    };
    let bail = words.iter().any(|w| w == "--bail");
    let positional: Vec<&String> = words.iter().filter(|w| !w.starts_with("--")).collect();

    let (coll_idx, folder) = if let Some(name) = positional.first() {
        // Unquoted names with spaces: the longest leading run of words that
        // names a collection wins; the remaining words are the folder.
        let found = (1..=positional.len()).rev().find_map(|n| {
            let candidate = positional[..n]
                .iter()
                .map(|w| w.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            app.collections
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(&candidate))
                .map(|idx| (idx, n))
        });
        let Some((idx, used)) = found else {
            app.notify(
                format!("Collection '{name}' not found"),
                NotificationKind::Error,
            );
            return;
        };
        let rest: Vec<&str> = positional[used..].iter().map(|w| w.as_str()).collect();
        (idx, (!rest.is_empty()).then(|| rest.join(" ")))
    } else if let Some(target) = default_target(app) {
        target
    } else if app.runner.run.is_some() {
        // Nothing to infer: reopen the last run.
        app.mode = AppMode::Modal(ModalKind::Runner);
        return;
    } else {
        app.notify(
            format!("No collection to run. {USAGE}"),
            NotificationKind::Warning,
        );
        return;
    };
    start_collection_run(app, coll_idx, folder, bail).await;
}

/// Sidebar `R`: run the selected collection, or folder.
pub(super) async fn run_sidebar_selection(app: &mut App) {
    if app.sidebar_state.section != SidebarSection::Collections {
        return;
    }
    if let Some((coll_idx, folder)) = sidebar_target(app) {
        start_collection_run(app, coll_idx, folder, false).await;
    }
}

fn sidebar_target(app: &App) -> Option<(usize, Option<String>)> {
    if app.sidebar_state.section != SidebarSection::Collections {
        return None;
    }
    match build_sidebar_items(app).get(app.sidebar_state.selected)? {
        SidebarItem::Collection { coll_idx, .. } | SidebarItem::Request { coll_idx, .. } => {
            Some((*coll_idx, None))
        }
        SidebarItem::Folder { coll_idx, id } => Some((*coll_idx, Some(id.to_string()))),
    }
}

fn default_target(app: &App) -> Option<(usize, Option<String>)> {
    if app.focus == FocusArea::Sidebar {
        if let Some(target) = sidebar_target(app) {
            return Some(target);
        }
    }
    if let Some(idx) = app
        .active_tab()
        .collection_index
        .filter(|i| *i < app.collections.len())
    {
        return Some((idx, None));
    }
    sidebar_target(app).or_else(|| (app.collections.len() == 1).then_some((0, None)))
}

/// Start running collection `coll_idx` (or one of its folders) with the
/// active environment and open the runner view.
pub async fn start_collection_run(
    app: &mut App,
    coll_idx: usize,
    folder: Option<String>,
    stop_on_failure: bool,
) {
    if app.runner.run.as_ref().is_some_and(|r| r.running) {
        app.mode = AppMode::Modal(ModalKind::Runner);
        app.notify(
            "A collection run is already in progress".into(),
            NotificationKind::Warning,
        );
        return;
    }
    let Some(collection) = app.collections.get(coll_idx).cloned() else {
        return;
    };
    let plan = match core_runner::plan_requests(&collection, folder.as_deref()) {
        Ok(plan) => plan,
        Err(e) => {
            app.notify(e.to_string(), NotificationKind::Error);
            return;
        }
    };
    if plan.is_empty() {
        app.notify("No requests to run".into(), NotificationKind::Warning);
        return;
    }

    let options = RunnerOptions {
        folder: folder.clone(),
        stop_on_failure,
        ..RunnerOptions::default()
    };
    let environment = app.active_environment().cloned();
    let run_id = Uuid::new_v4();
    let rows = plan
        .iter()
        .map(|p| RunRow {
            request_id: p.request.id,
            name: p.request.name.clone(),
            method: match p.request.protocol {
                hitt_core::core::request::Protocol::Http => p.request.method.as_str().to_string(),
                hitt_core::core::request::Protocol::WebSocket => "WS".into(),
                hitt_core::core::request::Protocol::Sse => "SSE".into(),
                hitt_core::core::request::Protocol::Grpc { .. } => "gRPC".into(),
            },
            folder: p.folder.clone(),
            iteration: 1,
            result: None,
        })
        .collect();
    app.runner.run = Some(RunView {
        id: run_id,
        coll_idx,
        collection: collection.name.clone(),
        folder: folder
            .as_deref()
            .and_then(|f| core_runner::resolve_folder_path(&collection, f)),
        environment: environment.as_ref().map(|e| e.name.clone()),
        rows,
        current: None,
        running: true,
        cancelled: false,
        selected: 0,
        follow: true,
        started: std::time::Instant::now(),
        started_at: chrono::Utc::now(),
        elapsed: None,
        summary: None,
        abort: None,
        export_input: None,
    });
    app.mode = AppMode::Modal(ModalKind::Runner);

    let client = app.http_client.clone();
    if let Some(tx) = app.event_sender.clone() {
        let handle = tokio::spawn(async move {
            let progress_tx = tx.clone();
            let outcome = core_runner::run_collection(
                &client,
                &collection,
                environment.as_ref(),
                &options,
                |event| {
                    let _ = progress_tx.send(AppEvent::CollectionRun {
                        run_id,
                        event: CollectionRunEvent::Progress(event.clone()),
                    });
                },
            )
            .await;
            let event = match outcome {
                Ok(summary) => CollectionRunEvent::Finished(Box::new(summary)),
                Err(e) => CollectionRunEvent::Failed(e.to_string()),
            };
            let _ = tx.send(AppEvent::CollectionRun { run_id, event });
        });
        if let Some(run) = app.runner.run.as_mut() {
            run.abort = Some(handle.abort_handle());
        }
    } else {
        // No event loop attached (tests / headless use): run inline.
        let mut events = Vec::new();
        let outcome = core_runner::run_collection(
            &client,
            &collection,
            environment.as_ref(),
            &options,
            |event| events.push(CollectionRunEvent::Progress(event.clone())),
        )
        .await;
        events.push(match outcome {
            Ok(summary) => CollectionRunEvent::Finished(Box::new(summary)),
            Err(e) => CollectionRunEvent::Failed(e.to_string()),
        });
        for event in events {
            handle_collection_run_event(app, run_id, event);
        }
    }
}

pub(super) fn handle_collection_run_event(app: &mut App, run_id: Uuid, event: CollectionRunEvent) {
    let Some(run) = app.runner.run.as_mut().filter(|r| r.id == run_id) else {
        return; // stale event from a cancelled / replaced run
    };
    if !run.running {
        return;
    }
    match event {
        // Rows are built from the plan before the run starts.
        CollectionRunEvent::Progress(RunEvent::Started { .. }) => {}
        CollectionRunEvent::Progress(RunEvent::RequestStarted { index, .. }) => {
            run.current = Some(index);
            if run.follow && index < run.rows.len() {
                run.selected = index;
            }
        }
        CollectionRunEvent::Progress(RunEvent::RequestFinished(result)) => {
            if let Some(row) = run.rows.get_mut(result.index) {
                row.result = Some(*result);
            }
        }
        CollectionRunEvent::Finished(summary) => {
            run.running = false;
            run.current = None;
            run.abort = None;
            run.elapsed = Some(std::time::Duration::from_millis(summary.duration_ms));
            if run.follow {
                // Land on the first failure so it can be inspected right away.
                run.selected = run
                    .rows
                    .iter()
                    .position(|r| {
                        r.result.as_ref().is_some_and(|r| {
                            matches!(r.status, RequestRunStatus::Failed | RequestRunStatus::Error)
                        })
                    })
                    .unwrap_or(0);
            }
            let (msg, kind) = summary_notification(&summary);
            run.summary = Some(*summary);
            app.notify(msg, kind);
        }
        CollectionRunEvent::Failed(err) => {
            run.running = false;
            run.current = None;
            run.abort = None;
            run.elapsed = Some(run.started.elapsed());
            app.notify(
                format!("Collection run failed: {err}"),
                NotificationKind::Error,
            );
        }
    }
}

fn summary_notification(summary: &RunSummary) -> (String, NotificationKind) {
    let failed = summary.failed + summary.errors;
    if failed == 0 {
        (
            format!(
                "Run complete: {}/{} passed ({} skipped)",
                summary.passed, summary.total, summary.skipped
            ),
            NotificationKind::Success,
        )
    } else {
        (
            format!("Run finished: {failed} of {} failed", summary.total),
            NotificationKind::Error,
        )
    }
}

/// Stop the in-flight run, keeping the results gathered so far.
pub(super) fn cancel_collection_run(app: &mut App) -> bool {
    let Some(run) = app.runner.run.as_mut().filter(|r| r.running) else {
        return false;
    };
    if let Some(handle) = run.abort.take() {
        handle.abort();
    }
    run.running = false;
    run.cancelled = true;
    run.current = None;
    let elapsed = run.started.elapsed();
    run.elapsed = Some(elapsed);
    let mut summary = RunSummary::from_results(
        run.collection.clone(),
        run.folder.clone(),
        run.environment.clone(),
        run.started_at,
        elapsed,
        run.finished_results().cloned().collect(),
    );
    summary.cancelled = true;
    run.summary = Some(summary);
    app.notify("Collection run cancelled".into(), NotificationKind::Info);
    true
}

/// `:runexport <path>` / `x` in the runner: write the last run's report.
/// `.xml` writes JUnit XML, `.json` JSON, `.txt` plain text.
pub(super) fn export_run_report(app: &mut App, path: &str) {
    let path = path.trim();
    if path.is_empty() {
        app.notify(
            "Usage: :runexport <path.json|path.xml>".into(),
            NotificationKind::Warning,
        );
        return;
    }
    let Some(run) = app.runner.run.as_ref() else {
        app.notify(
            "No collection run to export".into(),
            NotificationKind::Warning,
        );
        return;
    };
    let Some(summary) = run.summary.as_ref().filter(|_| !run.running) else {
        app.notify(
            "Wait for the run to finish before exporting".into(),
            NotificationKind::Warning,
        );
        return;
    };
    let expanded = expand_tilde(path);
    let ext = expanded
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let report = match ext.as_deref() {
        Some("xml") => Ok(core_runner::to_junit_xml(summary)),
        Some("json") => core_runner::to_json(summary).map_err(|e| e.to_string()),
        Some("txt") => Ok(core_runner::to_text(summary)),
        _ => Err("use a .json, .xml (JUnit) or .txt path".to_string()),
    };
    match report.and_then(|r| std::fs::write(&expanded, r).map_err(|e| e.to_string())) {
        Ok(()) => app.notify(
            format!("Report exported to {}", expanded.display()),
            NotificationKind::Success,
        ),
        Err(e) => app.notify(format!("Export failed: {e}"), NotificationKind::Error),
    }
}

/// Keys of the runner modal.
pub(super) fn handle_runner_modal(app: &mut App, key: KeyEvent) {
    let Some(run) = app.runner.run.as_mut() else {
        app.mode = AppMode::Normal;
        return;
    };

    // Export path prompt.
    if let Some(input) = run.export_input.as_mut() {
        match key.code {
            KeyCode::Esc => run.export_input = None,
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Enter => {
                let path = run.export_input.take().unwrap_or_default();
                export_run_report(app, &path);
            }
            KeyCode::Char(c) if is_text_input(key.modifiers) => input.push(c),
            _ => {}
        }
        return;
    }

    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Char('q')) => {
            if !cancel_collection_run(app) {
                app.mode = AppMode::Normal;
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('j') | KeyCode::Down) => run.select_next(),
        (KeyModifiers::NONE, KeyCode::Char('k') | KeyCode::Up) => run.select_prev(),
        (KeyModifiers::NONE, KeyCode::Char('g') | KeyCode::Home) => {
            run.follow = false;
            run.selected = 0;
        }
        (KeyModifiers::SHIFT, KeyCode::Char('G')) | (_, KeyCode::End) => {
            run.follow = false;
            run.selected = run.rows.len().saturating_sub(1);
        }
        (KeyModifiers::NONE, KeyCode::Char('x')) => {
            if run.running {
                app.notify(
                    "Wait for the run to finish before exporting".into(),
                    NotificationKind::Warning,
                );
            } else {
                run.export_input = Some(DEFAULT_EXPORT_PATH.to_string());
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => open_selected_row(app),
        _ => {}
    }
}

/// Open the selected row's request in a new tab, with its run response.
fn open_selected_row(app: &mut App) {
    let Some(run) = app.runner.run.as_ref() else {
        return;
    };
    let Some(row) = run.rows.get(run.selected) else {
        return;
    };
    let request_id = row.request_id;
    let response = row.result.as_ref().and_then(|r| r.response.clone());

    // Prefer the run's collection; fall back to a search in case indexes moved.
    let found = app
        .collections
        .get(run.coll_idx)
        .and_then(|c| c.find_request(&request_id))
        .map(|r| (run.coll_idx, r.clone()))
        .or_else(|| {
            app.collections
                .iter()
                .enumerate()
                .find_map(|(ci, c)| c.find_request(&request_id).map(|r| (ci, r.clone())))
        });
    let Some((coll_idx, request)) = found else {
        app.notify(
            "Request no longer exists in the collection".into(),
            NotificationKind::Warning,
        );
        return;
    };

    let has_response = response.is_some();
    let mut tab = crate::app::RequestTab::from_request(request, Some(coll_idx));
    tab.response = response;
    app.tabs.push(tab);
    app.active_tab = app.tabs.len() - 1;
    app.response_scroll = 0;
    app.request_scroll = 0;
    app.mode = AppMode::Normal;
    app.focus = if has_response {
        FocusArea::ResponseBody
    } else {
        FocusArea::UrlBar
    };
}
