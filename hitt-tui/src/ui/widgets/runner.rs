//! Collection runner view (`ModalKind::Runner`, opened by `:run` / sidebar `R`).
//!
//! Shows one row per scheduled request with live progress while the run is
//! in flight, then the final summary. The run itself lives in
//! `hitt_core::testing::collection_runner`; `event::runner` drives it.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use uuid::Uuid;

use crate::app::App;
use crate::ui::theme::Theme;
use hitt_core::testing::collection_runner::{RequestRunResult, RequestRunStatus, RunSummary};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Runner state on `App`: the current (or last finished) run, if any.
#[derive(Debug, Default)]
pub struct RunnerState {
    pub run: Option<RunView>,
    /// First visible row of the request list, kept between frames so the
    /// list only scrolls when the selection leaves the view.
    pub list_offset: std::cell::Cell<usize>,
}

/// One scheduled request of a run.
#[derive(Debug, Clone)]
pub struct RunRow {
    pub request_id: Uuid,
    pub name: String,
    pub method: String,
    pub folder: Option<String>,
    pub iteration: u32,
    /// `None` until the request finishes.
    pub result: Option<RequestRunResult>,
}

#[derive(Debug)]
pub struct RunView {
    /// Identifies the run so events of a cancelled run are ignored.
    pub id: Uuid,
    pub coll_idx: usize,
    pub collection: String,
    pub folder: Option<String>,
    pub environment: Option<String>,
    pub rows: Vec<RunRow>,
    /// Row currently being sent.
    pub current: Option<usize>,
    pub running: bool,
    pub cancelled: bool,
    pub selected: usize,
    /// Selection follows the running request until the user moves it.
    pub follow: bool,
    pub started: Instant,
    pub started_at: DateTime<Utc>,
    /// Final duration once the run is over.
    pub elapsed: Option<Duration>,
    pub summary: Option<RunSummary>,
    pub abort: Option<tokio::task::AbortHandle>,
    /// Path being typed after `x` (export prompt open).
    pub export_input: Option<String>,
}

impl RunView {
    pub fn finished_results(&self) -> impl Iterator<Item = &RequestRunResult> {
        self.rows.iter().filter_map(|r| r.result.as_ref())
    }

    pub fn count(&self, status: RequestRunStatus) -> usize {
        self.finished_results()
            .filter(|r| r.status == status)
            .count()
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed.unwrap_or_else(|| self.started.elapsed())
    }

    pub fn select_next(&mut self) {
        self.follow = false;
        if self.selected + 1 < self.rows.len() {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        self.follow = false;
        self.selected = self.selected.saturating_sub(1);
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Returns the rect of each visible request row with its index.
pub fn render(app: &App, frame: &mut Frame, area: Rect) -> Vec<(Rect, usize)> {
    let theme = &app.theme;
    let Some(run) = app.runner.run.as_ref() else {
        return Vec::new();
    };

    let title = match &run.folder {
        Some(folder) => format!(" Collection Runner: {} / {folder} ", run.collection),
        None => format!(" Collection Runner: {} ", run.collection),
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(theme.focused_border_style())
        .style(Style::default().bg(theme.colors.background));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(header_lines(run, theme)), chunks[0]);
    let rows = render_rows(run, &app.runner.list_offset, theme, frame, chunks[1]);
    frame.render_widget(
        Paragraph::new(detail_lines(run, theme)).wrap(Wrap { trim: false }),
        chunks[2],
    );
    frame.render_widget(Paragraph::new(footer_line(run, theme)), chunks[3]);
    rows
}

fn header_lines(run: &RunView, theme: &Theme) -> Vec<Line<'static>> {
    let done = run.finished_results().count();
    let total = run.rows.len();
    let state = if run.running {
        let current = run
            .current
            .and_then(|i| run.rows.get(i))
            .map(|r| r.name.clone())
            .unwrap_or_default();
        Span::styled(
            format!(
                "{} Running {}/{total}  {current}",
                crate::ui::layout::spinner_frame(),
                (done + 1).min(total)
            ),
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        )
    } else if run.cancelled {
        Span::styled(
            format!("Cancelled after {done}/{total}"),
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        let ok = run.count(RequestRunStatus::Failed) + run.count(RequestRunStatus::Error) == 0;
        let bailed = run.summary.as_ref().is_some_and(|s| s.bailed);
        let text = match (ok, bailed) {
            (true, _) => "✓ All requests passed".to_string(),
            (false, true) => "✗ Stopped on first failure".to_string(),
            (false, false) => "✗ Some requests failed".to_string(),
        };
        let color = if ok {
            theme.colors.success
        } else {
            theme.colors.error
        };
        Span::styled(
            text,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )
    };
    let env = run.environment.as_deref().unwrap_or("no environment");

    let (a_pass, a_fail) = run.finished_results().fold((0, 0), |(p, f), r| {
        (p + r.assertions_passed(), f + r.assertions_failed())
    });
    let counts = Line::from(vec![
        Span::styled(
            format!("{} passed", run.count(RequestRunStatus::Passed)),
            theme.success_style(),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} failed", run.count(RequestRunStatus::Failed)),
            theme.error_style(),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} errors", run.count(RequestRunStatus::Error)),
            theme.error_style(),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} skipped", run.count(RequestRunStatus::Skipped)),
            theme.muted_style(),
        ),
        Span::styled(
            format!(
                "   assertions {a_pass}/{}   {:.2}s",
                a_pass + a_fail,
                run.elapsed().as_secs_f64()
            ),
            theme.muted_style(),
        ),
    ]);
    vec![
        Line::from(vec![
            state,
            Span::styled(format!("   env: {env}"), theme.muted_style()),
        ]),
        counts,
    ]
}

fn render_rows(
    run: &RunView,
    list_offset: &std::cell::Cell<usize>,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) -> Vec<(Rect, usize)> {
    let name_width = usize::from(area.width).saturating_sub(40).max(10);
    let selected = run.selected.min(run.rows.len().saturating_sub(1));
    let items: Vec<ListItem> = run
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let name = {
                let full = match &row.folder {
                    Some(folder) => format!("{folder}/{}", row.name),
                    None => row.name.clone(),
                };
                let mut name: String = full.chars().take(name_width).collect();
                if row.iteration > 1 {
                    name = format!("{name} #{}", row.iteration);
                }
                name
            };
            let (icon, icon_style) = match &row.result {
                None if run.running && run.current == Some(i) => {
                    (crate::ui::layout::spinner_frame(), theme.warning_style())
                }
                None => ("·", theme.muted_style()),
                Some(r) => match r.status {
                    RequestRunStatus::Passed => ("✓", theme.success_style()),
                    RequestRunStatus::Failed | RequestRunStatus::Error => {
                        ("✗", theme.error_style())
                    }
                    RequestRunStatus::Skipped => ("-", theme.muted_style()),
                },
            };
            let method_style = row
                .method
                .parse::<hitt_core::core::request::HttpMethod>()
                .map_or_else(|_| theme.accent_style(), |m| theme.method_style(&m));
            let mut spans = vec![
                Span::styled(format!(" {icon} "), icon_style),
                Span::styled(format!("{:<7} ", row.method), method_style),
                Span::styled(
                    format!("{name:<name_width$} "),
                    Style::default().fg(theme.colors.foreground),
                ),
            ];
            match &row.result {
                Some(r) if r.http_status.is_some() => {
                    let status = r.http_status.unwrap_or_default();
                    spans.push(Span::styled(
                        format!("{status:>3} "),
                        theme.status_style(status),
                    ));
                    spans.push(Span::styled(
                        format!("{:>6}ms ", r.duration_ms),
                        theme.muted_style(),
                    ));
                    if !r.assertions.is_empty() {
                        let style = if r.assertions_failed() == 0 {
                            theme.success_style()
                        } else {
                            theme.error_style()
                        };
                        spans.push(Span::styled(
                            format!("{}/{} tests", r.assertions_passed(), r.assertions.len()),
                            style,
                        ));
                    }
                }
                Some(r) if r.status == RequestRunStatus::Skipped => {
                    spans.push(Span::styled("skipped", theme.muted_style()));
                }
                Some(_) => spans.push(Span::styled("error", theme.error_style())),
                None => {}
            }
            let line = Line::from(spans);
            if i == selected {
                ListItem::new(theme.selected_line(line))
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_set(theme.border_set())
        .border_style(theme.unfocused_border_style());
    let inner = block.inner(area);
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_style());
    // Start from the previous frame's offset: `List` only scrolls when the
    // selection would leave the view (clicking a visible row keeps it still).
    let height = usize::from(inner.height);
    let mut state = ListState::default()
        .with_offset(list_offset.get().min(run.rows.len().saturating_sub(height)));
    if !run.rows.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
    list_offset.set(state.offset());

    // The list scrolls to keep the selection visible (single-line items).
    (state.offset()..run.rows.len())
        .take(usize::from(inner.height))
        .enumerate()
        .map(|(row, i)| (Rect::new(inner.x, inner.y + row as u16, inner.width, 1), i))
        .collect()
}

/// Details of the selected row: failed assertions or the error.
fn detail_lines(run: &RunView, theme: &Theme) -> Vec<Line<'static>> {
    let Some(row) = run.rows.get(run.selected) else {
        return Vec::new();
    };
    let Some(result) = &row.result else {
        return vec![Line::from(Span::styled(
            if run.running { "Pending..." } else { "Not run" },
            theme.muted_style(),
        ))];
    };
    let mut lines = vec![Line::from(Span::styled(
        format!("{} {}", result.method, result.url),
        theme.muted_style(),
    ))];
    if let Some(err) = &result.error {
        let style = if result.status == RequestRunStatus::Skipped {
            theme.muted_style()
        } else {
            theme.error_style()
        };
        lines.push(Line::from(Span::styled(err.clone(), style)));
    }
    for a in &result.assertions {
        let (icon, style) = if a.passed {
            ("✓", theme.success_style())
        } else {
            ("✗", theme.error_style())
        };
        lines.push(Line::from(Span::styled(
            format!("{icon} {}  {}", a.description, a.message),
            style,
        )));
    }
    lines
}

fn footer_line(run: &RunView, theme: &Theme) -> Line<'static> {
    if let Some(input) = &run.export_input {
        return Line::from(vec![
            Span::styled("Export to: ", theme.accent_style()),
            Span::styled(
                format!("{input}_"),
                Style::default().fg(theme.colors.foreground),
            ),
            Span::styled(
                "   (.json / .xml)  Enter:save  Esc:cancel",
                theme.muted_style(),
            ),
        ]);
    }
    let hint = if run.running {
        " j/k:select  Esc/q:cancel run"
    } else {
        " j/k:select  Enter:open in tab  x:export report  Esc/q:close"
    };
    Line::from(Span::styled(hint, theme.muted_style()))
}
