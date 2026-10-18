//! Response diff selector and viewer (`ModalKind::DiffSelector`).
//!
//! The modal has two phases:
//!   1. Selection: lists every response that has a text body (open tabs first,
//!      then history, newest first) and lets the user mark two of them.
//!   2. Diff view: renders the line diff computed by
//!      `hitt_core::testing::diff::diff_text`, side-by-side or unified.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::cell::Cell;
use uuid::Uuid;

use crate::app::App;
use crate::ui::theme::Theme;
use hitt_core::core::request::HttpMethod;
use hitt_core::testing::diff::{DiffResult, DiffTag};

// ---------------------------------------------------------------------------
// State / candidates
// ---------------------------------------------------------------------------

/// Stable identity of a diffable response.  Indices are avoided on purpose:
/// a request finishing while the modal is open prepends a history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSource {
    Tab(Uuid),
    History(Uuid),
}

/// A response that can be diffed, borrowed from the `App`.
pub struct DiffCandidate<'a> {
    pub source: DiffSource,
    pub origin: &'static str,
    pub method: HttpMethod,
    pub label: String,
    pub status: Option<u16>,
    pub body: &'a str,
}

/// A computed diff ready to be displayed.
#[derive(Debug, Clone)]
pub struct DiffView {
    pub result: DiffResult,
    pub left_label: String,
    pub right_label: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiffSelectorState {
    /// Cursor position in the candidate list.
    pub selected: usize,
    /// Marked responses, in marking order (first = left/old, second = right/new).
    pub marked: Vec<DiffSource>,
    /// `Some` once two responses have been compared.
    pub view: Option<DiffView>,
    pub scroll: usize,
    pub unified: bool,
    /// First visible candidate row, kept between frames so the list only
    /// scrolls when the cursor leaves the view.
    pub list_offset: Cell<usize>,
    /// Rows of the diff body shown by the last frame (bounds `scroll`).
    pub view_height: Cell<usize>,
}

impl DiffSelectorState {
    /// Largest useful `scroll` of the diff view: the last page stays full.
    pub fn max_scroll(&self) -> usize {
        self.view.as_ref().map_or(0, |v| {
            v.result
                .lines
                .len()
                .saturating_sub(self.view_height.get().max(1))
        })
    }
}

/// Collects all responses with a text body: open tabs first, then history.
pub fn collect_candidates(app: &App) -> Vec<DiffCandidate<'_>> {
    let mut out = Vec::new();

    for tab in &app.tabs {
        if let Some(resp) = &tab.response {
            if let Some(body) = resp.body_text() {
                let name = if tab.request.name.is_empty() {
                    tab.request.url.clone()
                } else {
                    tab.request.name.clone()
                };
                out.push(DiffCandidate {
                    source: DiffSource::Tab(tab.id),
                    origin: "tab",
                    method: tab.request.method,
                    label: name,
                    status: Some(resp.status),
                    body,
                });
            }
        }
    }

    for entry in app.history.entries() {
        if let Some(body) = entry.response_body.as_deref() {
            let time = entry
                .timestamp
                .with_timezone(&chrono::Local)
                .format("%H:%M:%S");
            out.push(DiffCandidate {
                source: DiffSource::History(entry.id),
                origin: "history",
                method: entry.method,
                label: format!("{}  {time}", entry.short_url(50)),
                status: entry.status,
                body,
            });
        }
    }

    out
}

fn candidate_title(c: &DiffCandidate<'_>) -> String {
    let status = c.status.map(|s| format!(" [{s}]")).unwrap_or_default();
    format!("{} {}{status} ({})", c.method, c.label, c.origin)
}

/// Computes the diff between the two marked candidates, if both still exist.
pub fn build_view(app: &App) -> Option<DiffView> {
    let state = &app.diff_state;
    let [left_src, right_src] = state.marked.as_slice() else {
        return None;
    };
    let candidates = collect_candidates(app);
    let left = candidates.iter().find(|c| c.source == *left_src)?;
    let right = candidates.iter().find(|c| c.source == *right_src)?;
    Some(DiffView {
        result: hitt_core::testing::diff::diff_text(left.body, right.body),
        left_label: candidate_title(left),
        right_label: candidate_title(right),
    })
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Renders the diff modal (selector or diff view) into `area`. Returns the
/// rect of each visible candidate row with its index (selector phase only).
pub fn render(app: &App, frame: &mut Frame, area: Rect) -> Vec<(Rect, usize)> {
    match &app.diff_state.view {
        Some(view) => {
            render_diff_view(app, view, frame, area);
            Vec::new()
        }
        None => render_selector(app, frame, area),
    }
}

fn modal_block<'a>(theme: &Theme, title: String) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(theme.focused_border_style())
        .style(Style::default().bg(theme.colors.background))
}

fn render_selector(app: &App, frame: &mut Frame, area: Rect) -> Vec<(Rect, usize)> {
    let theme = &app.theme;
    let state = &app.diff_state;
    let candidates = collect_candidates(app);

    let block = modal_block(theme, " Select Responses to Diff ".into());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    if candidates.len() < 2 {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Not enough responses to compare.",
                Style::default().fg(theme.colors.warning),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    "  Found {} response(s) with a text body; at least 2 are needed.",
                    candidates.len()
                ),
                theme.muted_style(),
            )),
            Line::from(Span::styled(
                "  Send a few requests (they are kept in history), then press d again.",
                theme.muted_style(),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), chunks[1]);
        let hint = Paragraph::new(Span::styled("  Esc:close", theme.muted_style()));
        frame.render_widget(hint, chunks[2]);
        return Vec::new();
    }

    let label = Paragraph::new(Span::styled(
        format!("  Mark two responses ({}/2 marked):", state.marked.len()),
        theme.muted_style(),
    ));
    frame.render_widget(label, chunks[0]);

    let selected = state.selected.min(candidates.len() - 1);
    let height = chunks[1].height as usize;
    // Keep the cursor visible, scrolling only when it leaves the window.
    let offset = crate::ui::widgets::request_editors::sticky_offset(
        &state.list_offset,
        selected,
        height,
        candidates.len(),
    );

    let items: Vec<ListItem> = candidates
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(i, c)| {
            let is_selected = i == selected;
            let cursor = if is_selected { "> " } else { "  " };
            let mark = match state.marked.iter().position(|m| *m == c.source) {
                Some(0) => "[1] ",
                Some(_) => "[2] ",
                None => "[ ] ",
            };
            let base = if is_selected {
                theme.selected_style()
            } else {
                Style::default().fg(theme.colors.foreground)
            };
            let mut spans = vec![
                Span::styled(cursor, base),
                Span::styled(
                    mark,
                    if is_selected {
                        base
                    } else {
                        Style::default().fg(theme.colors.accent)
                    },
                ),
                Span::styled(
                    format!("{:<7}", c.method.to_string()),
                    if is_selected {
                        base
                    } else {
                        theme.method_style(&c.method)
                    },
                ),
                Span::styled(c.label.clone(), base),
            ];
            if let Some(status) = c.status {
                spans.push(Span::styled(
                    format!("  {status}"),
                    if is_selected {
                        base
                    } else {
                        theme.status_style(status)
                    },
                ));
            }
            spans.push(Span::styled(
                format!("  ({})", c.origin),
                if is_selected {
                    base
                } else {
                    theme.muted_style()
                },
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();
    frame.render_widget(List::new(items), chunks[1]);

    let hint = if state.marked.len() == 2 {
        "  j/k:move  Space:mark/unmark  Enter:show diff  Esc:cancel"
    } else {
        "  j/k:move  Space/Enter:mark  Esc:cancel"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, theme.muted_style())),
        chunks[2],
    );

    let list = chunks[1];
    (offset..candidates.len())
        .take(height)
        .enumerate()
        .map(|(row, i)| (Rect::new(list.x, list.y + row as u16, list.width, 1), i))
        .collect()
}

fn render_diff_view(app: &App, view: &DiffView, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let state = &app.diff_state;
    let diff = &view.result;

    let mode = if state.unified {
        "unified"
    } else {
        "side-by-side"
    };
    let block = modal_block(
        theme,
        format!(
            " Diff ({mode}): +{} -{} ={} ",
            diff.additions, diff.deletions, diff.unchanged
        ),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let del_style = Style::default().fg(theme.colors.error);
    let ins_style = Style::default().fg(theme.colors.success);

    let header = vec![
        Line::from(vec![
            Span::styled(" - old: ", del_style.add_modifier(Modifier::BOLD)),
            Span::styled(view.left_label.clone(), theme.muted_style()),
        ]),
        Line::from(vec![
            Span::styled(" + new: ", ins_style.add_modifier(Modifier::BOLD)),
            Span::styled(view.right_label.clone(), theme.muted_style()),
        ]),
    ];
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let body_area = chunks[1];
    state.view_height.set(usize::from(body_area.height));
    let scroll = u16::try_from(state.scroll.min(state.max_scroll())).unwrap_or(u16::MAX);

    if diff.lines.is_empty() || (diff.additions == 0 && diff.deletions == 0) {
        let msg = if diff.lines.is_empty() {
            "  Both responses are empty."
        } else {
            "  Responses are identical."
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(theme.colors.success))),
            body_area,
        );
    } else if state.unified {
        let lines: Vec<Line> = diff
            .lines
            .iter()
            .map(|line| {
                let (prefix, style) = match line.tag {
                    DiffTag::Equal => (' ', Style::default().fg(theme.colors.foreground)),
                    DiffTag::Delete => ('-', del_style),
                    DiffTag::Insert => ('+', ins_style),
                };
                let old = line
                    .old_line
                    .map_or_else(|| "    ".into(), |n| format!("{n:4}"));
                let new = line
                    .new_line
                    .map_or_else(|| "    ".into(), |n| format!("{n:4}"));
                Line::from(vec![
                    Span::styled(format!("{old} {new} "), theme.muted_style()),
                    Span::styled(format!("{prefix} {}", line.content), style),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), body_area);
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(body_area);

        let left_block = Block::default()
            .borders(Borders::RIGHT)
            .border_set(theme.border_set())
            .border_style(theme.unfocused_border_style());
        let left_inner = left_block.inner(columns[0]);
        frame.render_widget(left_block, columns[0]);

        // Deletions and insertions are padded with a blank line on the
        // opposite side so both columns stay row-aligned while scrolling.
        let mut left_lines = Vec::with_capacity(diff.lines.len());
        let mut right_lines = Vec::with_capacity(diff.lines.len());
        for line in &diff.lines {
            match line.tag {
                DiffTag::Equal => {
                    let style = Style::default().fg(theme.colors.foreground);
                    left_lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:4}  ", line.old_line.unwrap_or(0)),
                            theme.muted_style(),
                        ),
                        Span::styled(line.content.as_str(), style),
                    ]));
                    right_lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:4}  ", line.new_line.unwrap_or(0)),
                            theme.muted_style(),
                        ),
                        Span::styled(line.content.as_str(), style),
                    ]));
                }
                DiffTag::Delete => {
                    left_lines.push(Line::from(vec![
                        Span::styled(format!("{:4} -", line.old_line.unwrap_or(0)), del_style),
                        Span::styled(line.content.as_str(), del_style),
                    ]));
                    right_lines.push(Line::raw(""));
                }
                DiffTag::Insert => {
                    left_lines.push(Line::raw(""));
                    right_lines.push(Line::from(vec![
                        Span::styled(format!("{:4} +", line.new_line.unwrap_or(0)), ins_style),
                        Span::styled(line.content.as_str(), ins_style),
                    ]));
                }
            }
        }

        frame.render_widget(Paragraph::new(left_lines).scroll((scroll, 0)), left_inner);
        frame.render_widget(Paragraph::new(right_lines).scroll((scroll, 0)), columns[1]);
    }

    let hint = Paragraph::new(Span::styled(
        "  j/k:scroll  J/K:page  g/G:top/end  u:unified/side-by-side  Backspace:back  Esc:close",
        theme.muted_style(),
    ));
    frame.render_widget(hint, chunks[2]);
}
