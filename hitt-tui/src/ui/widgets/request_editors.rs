//! Rendering for the request sub-tab editors and the variables modal.
//!
//! State lives in `crate::editor`; keys are handled in `event/editors.rs`.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::cell::Cell;

use crate::app::{App, AppMode, FocusArea, NavMode, RequestTabKind};
use crate::editor::{
    auth_inherits, auth_rows, auth_type_name, body_type_label, json_error, mask_secret,
    BodyTextTarget, EditTarget, FieldForm, FieldKind, KvRow, TextInput, VarScope,
};
use crate::ui::layout::{clamp_scroll, wrapped_row_count};
use crate::ui::theme::Theme;
use hitt_core::core::request::RequestBody;

/// Render the active request sub-tab into `area` (inside the panel border).
/// `scroll` is the request-area scroll used by read-only text views.
/// Returns the screen rows of the visible table entries (for mouse clicks).
pub fn render(app: &App, frame: &mut Frame, area: Rect, scroll: &mut usize) -> Vec<(Rect, usize)> {
    let tab = app.active_tab();
    let form = active_request_form(app);
    let selectable = app.focus == FocusArea::RequestBody
        && (app.nav_mode == NavMode::Panel || app.mode == AppMode::Insert);
    let theme = &app.theme;

    match tab.request_tab {
        RequestTabKind::Params => render_kv_table(
            frame,
            area,
            theme,
            &tab.request.params,
            selectable.then_some(app.editor.params_selected),
            form.filter(|f| f.target == EditTarget::Param),
            "No query parameters. Press a to add one (enabled params are appended to the URL)",
            false,
            &app.editor.table_scroll,
        ),
        RequestTabKind::Headers => render_kv_table(
            frame,
            area,
            theme,
            &tab.request.headers,
            selectable.then_some(app.editor.headers_selected),
            form.filter(|f| f.target == EditTarget::Header),
            "No headers. Press a to add one",
            false,
            &app.editor.table_scroll,
        ),
        RequestTabKind::Auth => {
            render_auth(app, frame, area, selectable, form);
            Vec::new()
        }
        RequestTabKind::Body => render_body(app, frame, area, scroll, selectable, form),
        RequestTabKind::Assertions => render_assertions(app, frame, area, selectable, form),
    }
}

/// The open request-panel form, if it belongs to the active tab.
fn active_request_form(app: &App) -> Option<&FieldForm> {
    if app.mode != AppMode::Insert {
        return None;
    }
    app.editor
        .request_form()
        .filter(|f| f.tab_id == app.active_tab().id)
}

// ---------------------------------------------------------------------------
// Shared pieces
// ---------------------------------------------------------------------------

/// Spans for a text input; the focused one shows a block cursor.
fn input_spans(input: &TextInput, style: Style, focused: bool) -> Vec<Span<'static>> {
    if !focused {
        return vec![Span::styled(input.value.clone(), style)];
    }
    let cursor = input.cursor();
    let before: String = input.value.chars().take(cursor).collect();
    let at: String = input
        .value
        .chars()
        .nth(cursor)
        .map_or(" ".into(), String::from);
    let after: String = input.value.chars().skip(cursor + 1).collect();
    vec![
        Span::styled(before, style),
        Span::styled(at, style.add_modifier(Modifier::REVERSED)),
        Span::styled(after, style),
    ]
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

/// First row index to draw so `selected` stays within `height` rows.
fn scroll_offset(selected: usize, height: usize) -> usize {
    if height == 0 || selected < height {
        0
    } else {
        selected + 1 - height
    }
}

/// Like `scroll_offset`, but starts from the previous frame's offset and only
/// moves it when `selected` would leave the view, so the rows don't shift
/// under the mouse.
pub(crate) fn sticky_offset(
    prev: &Cell<usize>,
    selected: usize,
    height: usize,
    len: usize,
) -> usize {
    if height == 0 {
        return 0;
    }
    let mut offset = prev.get().min(len.saturating_sub(height));
    if selected < offset {
        offset = selected;
    } else if selected >= offset + height {
        offset = selected + 1 - height;
    }
    prev.set(offset);
    offset
}

/// Split off a one-line footer for form errors / warnings.
fn split_footer(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    (chunks[0], chunks[1])
}

fn render_error(frame: &mut Frame, area: Rect, theme: &Theme, error: &str) {
    frame.render_widget(
        Paragraph::new(Span::styled(format!(" ! {error}"), theme.error_style())),
        area,
    );
}

// ---------------------------------------------------------------------------
// Key/value table (params, headers, form bodies, variables)
// ---------------------------------------------------------------------------

/// Draw `rows` as `[x] key  value` lines. `form` (if editing a row) replaces
/// that row with inline inputs. Secret rows are masked unless `reveal`.
/// Returns the rect of each visible row with its index.
#[allow(clippy::too_many_arguments)]
fn render_kv_table<R: KvRow>(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    rows: &[R],
    selected: Option<usize>,
    form: Option<&FieldForm>,
    empty_msg: &str,
    reveal: bool,
    scroll: &Cell<usize>,
) -> Vec<(Rect, usize)> {
    let editing_row = form.and_then(|f| f.row);
    if rows.is_empty() && editing_row.is_none() {
        let p = Paragraph::new(empty_msg)
            .style(theme.muted_style())
            .wrap(Wrap { trim: true });
        frame.render_widget(p, area);
        return Vec::new();
    }

    let (table_area, footer) = match form.and_then(|f| f.error.as_ref()) {
        Some(_) => split_footer(area),
        None => (area, Rect::default()),
    };

    let key_w = rows
        .iter()
        .map(|r| r.row_key().chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, (usize::from(area.width) / 3).max(8))
        .min(32);
    let selected = selected.map(|s| s.min(rows.len().saturating_sub(1)));
    let focus_row = editing_row.or(selected).unwrap_or(0);
    let height = usize::from(table_area.height);
    let total = rows.len().max(editing_row.map_or(0, |r| r + 1));
    let offset = sticky_offset(scroll, focus_row, height, total);

    let key_style = Style::default()
        .fg(theme.colors.accent)
        .add_modifier(Modifier::BOLD);
    let val_style = Style::default().fg(theme.colors.foreground);

    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(i, r)| {
            let check = if r.row_enabled() { "[x] " } else { "[ ] " };
            if let (Some(form), Some(row)) = (form, editing_row) {
                if row == i {
                    // Same width as the `[x] ` checkbox so columns stay aligned.
                    let mut spans = vec![Span::styled(" >  ", theme.accent_style())];
                    let key = input_spans(&form.fields[0].input, val_style, form.focused == 0);
                    let pad = key_w.saturating_sub(spans_width(&key)) + 2;
                    spans.extend(key);
                    spans.push(Span::raw(" ".repeat(pad)));
                    spans.extend(input_spans(
                        &form.fields[1].input,
                        val_style,
                        form.focused == 1,
                    ));
                    return Line::from(spans);
                }
            }
            let value = if r.is_secret() && !reveal {
                mask_secret(r.row_value())
            } else {
                r.row_value().to_string()
            };
            let key = format!("{:<key_w$}  ", r.row_key());
            let secret_tag = if r.is_secret() { "  (secret)" } else { "" };
            if selected == Some(i) {
                let style = theme.selected_style().add_modifier(Modifier::BOLD);
                Line::from(vec![
                    Span::styled(check, style),
                    Span::styled(key, style),
                    Span::styled(value, style),
                    Span::styled(secret_tag, style),
                ])
            } else if r.row_enabled() {
                Line::from(vec![
                    Span::styled(check, val_style),
                    Span::styled(key, key_style),
                    Span::styled(value, val_style),
                    Span::styled(secret_tag, theme.muted_style()),
                ])
            } else {
                let dim = theme.muted_style().add_modifier(Modifier::DIM);
                Line::from(vec![
                    Span::styled(check, dim),
                    Span::styled(key, dim),
                    Span::styled(value, dim),
                    Span::styled(secret_tag, dim),
                ])
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), table_area);

    if let Some(error) = form.and_then(|f| f.error.as_deref()) {
        render_error(frame, footer, theme, error);
    }
    row_rects(table_area, offset, rows.len())
}

/// One rect per visible row of a list drawn from `offset` into `area`.
fn row_rects(area: Rect, offset: usize, len: usize) -> Vec<(Rect, usize)> {
    (offset..len)
        .take(usize::from(area.height))
        .enumerate()
        .map(|(row, i)| (Rect::new(area.x, area.y + row as u16, area.width, 1), i))
        .collect()
}

// ---------------------------------------------------------------------------
// Generic field form (auth, assertions, single fields)
// ---------------------------------------------------------------------------

/// `label: value` lines for a form, the focused field showing its cursor.
fn form_lines(form: &FieldForm, theme: &Theme) -> Vec<Line<'static>> {
    let label_w = form
        .fields
        .iter()
        .map(|f| f.label.chars().count())
        .max()
        .unwrap_or(0);
    form.fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let focused = i == form.focused;
            let label_style = if focused {
                theme.accent_style().add_modifier(Modifier::BOLD)
            } else {
                theme.muted_style()
            };
            let marker = if focused { "> " } else { "  " };
            let mut spans = vec![
                Span::styled(marker, theme.accent_style()),
                Span::styled(format!("{:<label_w$}  ", field.label), label_style),
            ];
            let val_style = Style::default().fg(theme.colors.foreground);
            match &field.kind {
                FieldKind::Text => spans.extend(input_spans(&field.input, val_style, focused)),
                FieldKind::Choice { options, selected } => {
                    let style = if focused {
                        val_style.add_modifier(Modifier::REVERSED)
                    } else {
                        val_style
                    };
                    spans.push(Span::styled(format!("< {} >", options[*selected]), style));
                }
            }
            Line::from(spans)
        })
        .collect()
}

/// Title (with key hints), an error/blank line, then the fields. The error
/// sits at the top so it stays visible in short panels; the view scrolls to
/// keep the focused field on screen.
fn render_form(frame: &mut Frame, area: Rect, theme: &Theme, title: &str, form: &FieldForm) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(theme.colors.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   Tab/Up/Down:field  Left/Right:option  Enter:save  Esc:cancel",
                theme.muted_style(),
            ),
        ]),
        match &form.error {
            Some(error) => Line::from(Span::styled(format!(" ! {error}"), theme.error_style())),
            None => Line::from(""),
        },
    ];
    lines.extend(form_lines(form, theme));
    let offset = scroll_offset(form.focused + 2, usize::from(area.height));
    let offset = u16::try_from(offset).unwrap_or(u16::MAX);
    frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), area);
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

fn render_auth(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    selectable: bool,
    form: Option<&FieldForm>,
) {
    let theme = &app.theme;
    let auth = app.active_tab().request.auth.as_ref();
    let name = auth_type_name(auth);

    if let Some(form) = form.filter(|f| f.target == EditTarget::Auth) {
        render_form(frame, area, theme, &format!("Edit {name}"), form);
        return;
    }

    let mut lines = vec![Line::from(vec![
        Span::styled("Type: ", theme.muted_style()),
        Span::styled(
            name,
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "   t:type  i/Enter:edit  v:show secrets",
            theme.muted_style(),
        ),
    ])];
    lines.push(Line::from(""));

    let rows = auth_rows(auth);
    if auth_inherits(auth) {
        lines.push(Line::from(Span::styled(
            "Uses the auth of the parent folder / collection",
            theme.muted_style(),
        )));
        lines.push(inherited_auth_line(app, theme));
    } else if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "No authentication is sent (even if the collection sets one). Press t to change",
            theme.muted_style(),
        )));
    }

    let label_w = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    let selected = app.editor.auth_selected.min(rows.len().saturating_sub(1));
    for (i, r) in rows.iter().enumerate() {
        let value = if r.secret && !app.editor.show_secrets {
            mask_secret(&r.value)
        } else {
            r.value.clone()
        };
        let label = format!("  {:<label_w$}  ", r.label);
        if selectable && i == selected {
            let style = theme.selected_style().add_modifier(Modifier::BOLD);
            lines.push(Line::from(vec![
                Span::styled(label, style),
                Span::styled(value, style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(label, theme.muted_style()),
                Span::styled(value, Style::default().fg(theme.colors.foreground)),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// What an inheriting request will actually send, from the tab's collection.
fn inherited_auth_line(app: &App, theme: &Theme) -> Line<'static> {
    let tab = app.active_tab();
    let inherited = tab
        .collection_index
        .and_then(|ci| app.collections.get(ci))
        .and_then(|coll| {
            hitt_core::testing::collection_runner::effective_auth_in(coll, &tab.request)
        })
        .filter(|a| !matches!(a, hitt_core::core::auth::AuthConfig::None));
    match inherited {
        Some(auth) => Line::from(vec![
            Span::styled("Effective: ", theme.muted_style()),
            Span::styled(
                auth.display_name(),
                Style::default().fg(theme.colors.foreground),
            ),
        ]),
        None => Line::from(Span::styled(
            "Effective: nothing to inherit, no auth is sent",
            theme.muted_style(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

fn render_assertions(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    selectable: bool,
    form: Option<&FieldForm>,
) -> Vec<(Rect, usize)> {
    let theme = &app.theme;
    let assertions = &app.active_tab().request.assertions;

    if let Some(form) = form.filter(|f| f.target == EditTarget::Assertion) {
        let title = match form.row {
            Some(row) => format!("Edit assertion #{}", row + 1),
            None => "New assertion".to_string(),
        };
        render_form(frame, area, theme, &title, form);
        return Vec::new();
    }

    if assertions.is_empty() {
        let p = Paragraph::new("No assertions. Press a to add one")
            .style(theme.muted_style())
            .wrap(Wrap { trim: true });
        frame.render_widget(p, area);
        return Vec::new();
    }

    let selected = selectable.then(|| app.editor.assertions_selected.min(assertions.len() - 1));
    let height = usize::from(area.height);
    let offset = sticky_offset(
        &app.editor.table_scroll,
        selected.unwrap_or(0),
        height,
        assertions.len(),
    );
    let lines: Vec<Line> = assertions
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(i, a)| {
            let check = if a.enabled { "[x] " } else { "[ ] " };
            let text = format!("{check}{}. {}", i + 1, a.kind.description());
            let style = if selected == Some(i) {
                theme.selected_style().add_modifier(Modifier::BOLD)
            } else if a.enabled {
                Style::default().fg(theme.colors.foreground)
            } else {
                theme.muted_style().add_modifier(Modifier::DIM)
            };
            Line::from(Span::styled(text, style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
    row_rects(area, offset, assertions.len())
}

// ---------------------------------------------------------------------------
// Body
// ---------------------------------------------------------------------------

fn render_body(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    scroll: &mut usize,
    selectable: bool,
    form: Option<&FieldForm>,
) -> Vec<(Rect, usize)> {
    let theme = &app.theme;
    let tab = app.active_tab();
    let body = tab.request.body.as_ref();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Header: body type + the keys that apply to it.
    let hints = match body {
        Some(RequestBody::Json(_)) => "   t:type  i/Enter:edit  f:format",
        Some(RequestBody::Raw { .. }) => "   t:type  i/Enter:edit  c:content-type",
        Some(RequestBody::GraphQL { .. }) => "   t:type  i:edit query  I:edit variables",
        Some(RequestBody::FormUrlEncoded(_) | RequestBody::FormData(_)) => {
            "   t:type  a:add  i/Enter:edit  x:delete  Space:toggle"
        }
        _ => "   t:type",
    };
    let mut header = vec![
        Span::styled("Type: ", theme.muted_style()),
        Span::styled(
            body_type_label(body),
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(RequestBody::Raw { content_type, .. }) = body {
        header.push(Span::styled(
            format!(" ({content_type})"),
            theme.muted_style(),
        ));
    }
    header.push(Span::styled(hints, theme.muted_style()));
    frame.render_widget(Paragraph::new(Line::from(header)), chunks[0]);
    let content = chunks[1];

    // Multi-line editor open for this tab.
    if let Some(edit) = app
        .editor
        .body
        .as_ref()
        .filter(|e| e.tab_id == tab.id && app.mode == AppMode::Insert)
    {
        let label = match (body, edit.target) {
            (Some(RequestBody::GraphQL { .. }), BodyTextTarget::Content) => {
                Some(" Editing query (Esc: done) ")
            }
            (_, BodyTextTarget::GraphqlVariables) => Some(" Editing variables (Esc: done) "),
            _ => None,
        };
        let block = Block::default()
            .borders(Borders::TOP)
            .border_set(theme.border_set())
            .border_style(theme.focused_border_style())
            .title(Span::styled(
                label.unwrap_or(" Editing (Esc: done) "),
                theme.warning_style(),
            ));
        let inner = block.inner(content);
        frame.render_widget(block, content);
        frame.render_widget(&edit.textarea, inner);
        return Vec::new();
    }

    match body {
        None | Some(RequestBody::None) => {
            let p = Paragraph::new("No request body. Press t to choose a type")
                .style(theme.muted_style());
            frame.render_widget(p, content);
            Vec::new()
        }
        Some(RequestBody::FormUrlEncoded(pairs) | RequestBody::FormData(pairs)) => {
            let (table, note) = if matches!(body, Some(RequestBody::FormData(_))) {
                split_footer(content)
            } else {
                (content, Rect::default())
            };
            let rows = render_kv_table(
                frame,
                table,
                theme,
                pairs,
                selectable.then_some(app.editor.form_body_selected),
                form.filter(|f| f.target == EditTarget::FormBody),
                "No fields. Press a to add one",
                false,
                &app.editor.table_scroll,
            );
            if note.height > 0 {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        "Tip: a value starting with @ sends a file (e.g. @./photo.png)",
                        theme.muted_style(),
                    )),
                    note,
                );
            }
            rows
        }
        Some(RequestBody::Raw { .. })
            if form.is_some_and(|f| f.target == EditTarget::RawContentType) =>
        {
            if let Some(form) = form {
                render_form(frame, content, theme, "Raw body content type", form);
            }
            Vec::new()
        }
        Some(body) => {
            render_body_text(app, frame, content, scroll, body);
            Vec::new()
        }
    }
}

/// Read-only view of a text body (scrolls with the request-area scroll).
fn render_body_text(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    scroll: &mut usize,
    body: &RequestBody,
) {
    let theme = &app.theme;
    let fg = Style::default().fg(theme.colors.foreground);
    let (text, warning) = match body {
        RequestBody::Json(s) => (Text::styled(s.clone(), fg), json_error(s)),
        RequestBody::Raw { content, .. } => (Text::styled(content.clone(), fg), None),
        RequestBody::GraphQL { query, variables } => {
            let mut lines = vec![Line::from(Span::styled("Query:", theme.muted_style()))];
            lines.extend(query.lines().map(|l| Line::styled(l.to_string(), fg)));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Variables:", theme.muted_style())));
            let vars = variables.as_deref().unwrap_or("");
            lines.extend(vars.lines().map(|l| Line::styled(l.to_string(), fg)));
            let warning = json_error(vars).map(|e| format!("variables: {e}"));
            (Text::from(lines), warning)
        }
        RequestBody::Protobuf { message } => (Text::styled(message.clone(), fg), None),
        RequestBody::Binary(path) => (
            Text::styled(format!("[Binary: {}]", path.display()), fg),
            None,
        ),
        RequestBody::FormData(_) | RequestBody::FormUrlEncoded(_) | RequestBody::None => {
            (Text::default(), None)
        }
    };

    let (text_area, footer) = if warning.is_some() {
        split_footer(area)
    } else {
        (area, Rect::default())
    };
    let rows = wrapped_row_count(&text.lines, text_area.width);
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((clamp_scroll(scroll, rows, text_area.height), 0));
    frame.render_widget(paragraph, text_area);

    if let Some(warning) = warning {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" ! Invalid JSON: {warning}"),
                theme.warning_style(),
            )),
            footer,
        );
    }
}

// ---------------------------------------------------------------------------
// Variables modal
// ---------------------------------------------------------------------------

/// Returns the rect of each visible variable row with its index.
pub fn render_env_editor_modal(app: &App, frame: &mut Frame, area: Rect) -> Vec<(Rect, usize)> {
    let theme = &app.theme;
    let scope = app.editor.env_scope;
    let collection = app
        .active_tab()
        .collection_index
        .and_then(|ci| app.collections.get(ci));

    let title = match scope {
        VarScope::Environment => match app.active_environment() {
            Some(env) if crate::event::active_env_is_session_only(app) => {
                format!(" Environment: {} (session-only, not saved) ", env.name)
            }
            Some(env) => format!(" Environment: {} ", env.name),
            None => " Environment ".to_string(),
        },
        VarScope::Collection => format!(
            " Collection variables: {} ",
            collection.map_or("-", |c| c.name.as_str())
        ),
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
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner);

    let form = app.editor.env_form();
    let hint = if form.is_some() {
        " Tab:key/value  Enter:save  Esc:cancel"
    } else {
        match scope {
            VarScope::Environment => {
                " a:add  i/Enter:edit  x:del  Space:on/off  s:secret  v:reveal  e:next env  n:new env  R:rename env  D:delete env  c:collection vars  Esc:close"
            }
            VarScope::Collection => {
                " a:add  i/Enter:edit  x:del  Space:on/off  c:environment  Esc:close"
            }
        }
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, theme.muted_style())).wrap(Wrap { trim: true }),
        chunks[1],
    );

    if let Some(form) = form.filter(|f| f.target == EditTarget::NewEnvironment) {
        render_form(frame, chunks[0], theme, "New environment", form);
        return Vec::new();
    }
    if let Some(form) = form.filter(|f| f.target == EditTarget::RenameEnvironment) {
        render_form(frame, chunks[0], theme, "Rename environment", form);
        return Vec::new();
    }

    let selected = Some(app.editor.env_selected);
    let reveal = app.editor.show_secrets;
    match scope {
        VarScope::Environment => {
            if let Some(env) = app.active_environment() {
                return render_kv_table(
                    frame,
                    chunks[0],
                    theme,
                    &env.values,
                    selected,
                    form,
                    "No variables. Press a to add one",
                    reveal,
                    &app.editor.env_table_scroll,
                );
            }
            frame.render_widget(
                Paragraph::new(
                    "No environment selected. Press n to create one (or use :newenv <name>), e to pick one",
                )
                .style(theme.muted_style())
                .wrap(Wrap { trim: true }),
                chunks[0],
            );
            Vec::new()
        }
        VarScope::Collection => {
            if let Some(coll) = collection {
                return render_kv_table(
                    frame,
                    chunks[0],
                    theme,
                    &coll.variables,
                    selected,
                    form,
                    "No collection variables. Press a to add one",
                    reveal,
                    &app.editor.env_table_scroll,
                );
            }
            Vec::new()
        }
    }
}
