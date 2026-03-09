use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Paragraph, Row, Table, Widget, Wrap,
    },
};

use crate::app::{App, AppMode, FocusArea, RequestTabKind};
use crate::core::auth::AuthConfig;
use crate::core::request::{KeyValuePair, RequestBody};
use crate::testing::assertion_engine::Assertion;
use crate::ui::theme::Theme;

/// Top-level widget that renders the entire request editing panel.
///
/// Layout from top to bottom:
///   1. Method selector + URL bar + Send indicator  (3 rows)
///   2. Request sub-tab bar                          (1 row)
///   3. Active sub-tab content                       (remaining space)
pub struct RequestPanel<'a> {
    app: &'a App,
}

impl<'a> RequestPanel<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for RequestPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = &self.app.theme;
        let tab = self.app.active_tab();
        let request = &tab.request;

        // Outer block
        let focused = self.app.focus == FocusArea::UrlBar
            || self.app.focus == FocusArea::RequestTabs
            || self.app.focus == FocusArea::RequestBody;

        let border_style = if focused {
            theme.focused_border_style()
        } else {
            theme.unfocused_border_style()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type_from_theme(theme))
            .border_style(border_style)
            .title(" Request ")
            .title_style(Style::default().fg(theme.colors.foreground));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 4 || inner.width < 10 {
            return;
        }

        let chunks = Layout::vertical([
            Constraint::Length(1), // method + url + send
            Constraint::Length(1), // sub-tab bar
            Constraint::Min(1),   // sub-tab content
        ])
        .split(inner);

        render_url_bar(chunks[0], buf, request, theme, self.app);
        render_request_sub_tabs(chunks[1], buf, tab.request_tab, theme, self.app);
        render_sub_tab_content(chunks[2], buf, tab.request_tab, request, theme, self.app);
    }
}

// ---------------------------------------------------------------------------
// URL bar:  [ METHOD v ]  https://example.com/api           [Send]
// ---------------------------------------------------------------------------

fn render_url_bar(area: Rect, buf: &mut Buffer, request: &crate::core::request::Request, theme: &Theme, app: &App) {
    if area.width < 12 {
        return;
    }

    let method_label = format!(" {} ", request.method.as_str());
    let method_width = method_label.len() as u16 + 2; // padding + dropdown arrow
    let send_width: u16 = 8; // " Send "

    let chunks = Layout::horizontal([
        Constraint::Length(method_width),
        Constraint::Length(1), // spacer
        Constraint::Min(1),   // url
        Constraint::Length(1), // spacer
        Constraint::Length(send_width),
    ])
    .split(area);

    // Method selector
    let method_style = theme.method_style(&request.method);
    let method_span = Span::styled(
        format!("{} \u{25BC}", request.method.as_str()),
        method_style,
    );
    Paragraph::new(Line::from(vec![Span::raw(" "), method_span]))
        .render(chunks[0], buf);

    // URL input
    let url_focused = app.focus == FocusArea::UrlBar;
    let url_style = if url_focused {
        Style::default()
            .fg(theme.colors.foreground)
            .add_modifier(Modifier::UNDERLINED)
    } else {
        Style::default().fg(theme.colors.foreground)
    };

    let url_display = if request.url.is_empty() {
        Span::styled("Enter URL...", theme.muted_style())
    } else {
        Span::styled(&request.url, url_style)
    };

    // Show a cursor indicator when editing the URL
    let url_line = if url_focused && app.mode == AppMode::Insert {
        Line::from(vec![url_display, Span::styled("\u{2588}", Style::default().fg(theme.colors.accent))])
    } else {
        Line::from(vec![url_display])
    };

    Paragraph::new(url_line).render(chunks[2], buf);

    // Send button
    let send_style = Style::default()
        .fg(theme.colors.background)
        .bg(theme.colors.accent)
        .add_modifier(Modifier::BOLD);
    Paragraph::new(Line::from(Span::styled(" Send ", send_style)))
        .alignment(Alignment::Center)
        .render(chunks[4], buf);
}

// ---------------------------------------------------------------------------
// Sub-tab bar:  Params | Auth | Headers | Body | Assertions
// ---------------------------------------------------------------------------

fn render_request_sub_tabs(area: Rect, buf: &mut Buffer, active: RequestTabKind, theme: &Theme, app: &App) {
    let tabs_focused = app.focus == FocusArea::RequestTabs;
    let mut spans: Vec<Span<'_>> = Vec::new();

    for (i, kind) in RequestTabKind::all().iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" \u{2502} ", theme.muted_style()));
        }

        let style = if *kind == active {
            if tabs_focused {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD)
            }
        } else {
            Style::default().fg(theme.colors.muted)
        };

        spans.push(Span::styled(kind.label(), style));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

// ---------------------------------------------------------------------------
// Sub-tab content dispatcher
// ---------------------------------------------------------------------------

fn render_sub_tab_content(
    area: Rect,
    buf: &mut Buffer,
    kind: RequestTabKind,
    request: &crate::core::request::Request,
    theme: &Theme,
    app: &App,
) {
    match kind {
        RequestTabKind::Params => render_params_tab(area, buf, &request.params, theme),
        RequestTabKind::Auth => render_auth_tab(area, buf, &request.auth, theme),
        RequestTabKind::Headers => render_headers_tab(area, buf, &request.headers, theme),
        RequestTabKind::Body => render_body_tab(area, buf, &request.body, theme, app),
        RequestTabKind::Assertions => render_assertions_tab(area, buf, &request.assertions, theme),
    }
}

// ---------------------------------------------------------------------------
// Params tab: key-value table with enable/disable checkboxes
// ---------------------------------------------------------------------------

fn render_params_tab(area: Rect, buf: &mut Buffer, params: &[KeyValuePair], theme: &Theme) {
    if params.is_empty() {
        let placeholder = Paragraph::new(Line::from(Span::styled(
            "No query parameters. Press 'a' to add one.",
            theme.muted_style(),
        )))
        .alignment(Alignment::Center);
        placeholder.render(area, buf);
        return;
    }

    render_kv_table(area, buf, params, "Query Parameters", theme);
}

// ---------------------------------------------------------------------------
// Auth tab: display current auth type and its editable fields
// ---------------------------------------------------------------------------

fn render_auth_tab(area: Rect, buf: &mut Buffer, auth: &Option<AuthConfig>, theme: &Theme) {
    let auth = auth.as_ref();

    let mut lines: Vec<Line<'_>> = Vec::new();

    let auth_name = auth.map(|a| a.display_name()).unwrap_or("No Auth");
    lines.push(Line::from(vec![
        Span::styled("Type: ", Style::default().fg(theme.colors.muted)),
        Span::styled(auth_name, Style::default().fg(theme.colors.accent).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::default());

    match auth {
        Some(AuthConfig::Bearer { token }) => {
            lines.push(field_line("Token", token, theme));
        }
        Some(AuthConfig::Basic { username, password }) => {
            lines.push(field_line("Username", username, theme));
            lines.push(field_line("Password", &mask_string(password), theme));
        }
        Some(AuthConfig::ApiKey { key, value, location }) => {
            let loc_str = match location {
                crate::core::auth::ApiKeyLocation::Header => "Header",
                crate::core::auth::ApiKeyLocation::QueryParam => "Query Param",
            };
            lines.push(field_line("Key", key, theme));
            lines.push(field_line("Value", value, theme));
            lines.push(field_line("Add to", loc_str, theme));
        }
        Some(AuthConfig::OAuth2 {
            grant_type,
            access_token_url,
            client_id,
            client_secret,
            scope,
            token,
        }) => {
            lines.push(field_line("Grant Type", grant_type.as_str(), theme));
            lines.push(field_line("Token URL", access_token_url, theme));
            lines.push(field_line("Client ID", client_id, theme));
            lines.push(field_line("Client Secret", &mask_string(client_secret), theme));
            if let Some(s) = scope {
                lines.push(field_line("Scope", s, theme));
            }
            if let Some(t) = token {
                lines.push(field_line("Token", t, theme));
            }
        }
        Some(AuthConfig::Inherit) => {
            lines.push(Line::from(Span::styled(
                "Inheriting authentication from parent collection.",
                theme.muted_style(),
            )));
        }
        Some(AuthConfig::None) | None => {
            lines.push(Line::from(Span::styled(
                "No authentication configured. Press 'e' to choose an auth type.",
                theme.muted_style(),
            )));
        }
    }

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

// ---------------------------------------------------------------------------
// Headers tab: key-value table
// ---------------------------------------------------------------------------

fn render_headers_tab(area: Rect, buf: &mut Buffer, headers: &[KeyValuePair], theme: &Theme) {
    if headers.is_empty() {
        let placeholder = Paragraph::new(Line::from(Span::styled(
            "No headers. Press 'a' to add one.",
            theme.muted_style(),
        )))
        .alignment(Alignment::Center);
        placeholder.render(area, buf);
        return;
    }

    render_kv_table(area, buf, headers, "Headers", theme);
}

// ---------------------------------------------------------------------------
// Body tab: text editor area for JSON / raw / form data
// ---------------------------------------------------------------------------

fn render_body_tab(
    area: Rect,
    buf: &mut Buffer,
    body: &Option<RequestBody>,
    theme: &Theme,
    app: &App,
) {
    let body_focused = app.focus == FocusArea::RequestBody;

    match body {
        None | Some(RequestBody::None) => {
            Paragraph::new(Line::from(Span::styled(
                "No body. Press 'b' to choose a body type.",
                theme.muted_style(),
            )))
            .alignment(Alignment::Center)
            .render(area, buf);
        }
        Some(RequestBody::Json(content)) => {
            render_body_editor(area, buf, content, "application/json", theme, body_focused);
        }
        Some(RequestBody::Raw { content, content_type }) => {
            render_body_editor(area, buf, content, content_type, theme, body_focused);
        }
        Some(RequestBody::FormData(pairs)) => {
            render_kv_table(area, buf, pairs, "Form Data (multipart)", theme);
        }
        Some(RequestBody::FormUrlEncoded(pairs)) => {
            render_kv_table(area, buf, pairs, "Form URL Encoded", theme);
        }
        Some(RequestBody::Binary(path)) => {
            let path_str = path.display().to_string();
            let lines = vec![
                Line::from(Span::styled("Binary File", Style::default().fg(theme.colors.accent).add_modifier(Modifier::BOLD))),
                Line::default(),
                Line::from(vec![
                    Span::styled("Path: ", theme.muted_style()),
                    Span::styled(&path_str, Style::default().fg(theme.colors.foreground)),
                ]),
            ];
            Paragraph::new(lines).render(area, buf);
        }
        Some(RequestBody::GraphQL { query, variables }) => {
            let split = Layout::vertical([
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(area);

            render_body_editor(split[0], buf, query, "GraphQL Query", theme, body_focused);

            let vars_content = variables.as_deref().unwrap_or("{}");
            render_body_editor(split[1], buf, vars_content, "Variables (JSON)", theme, false);
        }
        Some(RequestBody::Protobuf { message }) => {
            render_body_editor(area, buf, message, "Protobuf Message (JSON)", theme, body_focused);
        }
    }
}

fn render_body_editor(area: Rect, buf: &mut Buffer, content: &str, label: &str, theme: &Theme, focused: bool) {
    if area.height < 2 {
        return;
    }

    // Header line showing content type
    let header_area = Rect { height: 1, ..area };
    let body_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };

    let label_style = Style::default()
        .fg(theme.colors.accent)
        .add_modifier(Modifier::BOLD);
    Paragraph::new(Line::from(Span::styled(label, label_style)))
        .render(header_area, buf);

    // Line numbers + content
    let lines: Vec<Line<'_>> = if content.is_empty() {
        vec![Line::from(Span::styled(
            if focused { "Start typing..." } else { "(empty)" },
            theme.muted_style(),
        ))]
    } else {
        content
            .lines()
            .enumerate()
            .map(|(i, line_text)| {
                let line_num = format!("{:>3} ", i + 1);
                Line::from(vec![
                    Span::styled(line_num, theme.muted_style()),
                    colorize_json_line(line_text, theme),
                ])
            })
            .collect()
    };

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(body_area, buf);
}

// ---------------------------------------------------------------------------
// Assertions tab: list of assertions with add/edit affordance
// ---------------------------------------------------------------------------

fn render_assertions_tab(area: Rect, buf: &mut Buffer, assertions: &[Assertion], theme: &Theme) {
    if assertions.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "No assertions. Press 'a' to add one.",
            theme.muted_style(),
        )))
        .alignment(Alignment::Center)
        .render(area, buf);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("", Style::default().fg(theme.colors.muted))),
        Cell::from(Span::styled("#", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Assertion", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
    ]);

    let rows: Vec<Row> = assertions
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let checkbox = if a.enabled { "\u{2611}" } else { "\u{2610}" };
            let cb_style = if a.enabled {
                Style::default().fg(theme.colors.success)
            } else {
                theme.muted_style()
            };

            let desc_style = if a.enabled {
                Style::default().fg(theme.colors.foreground)
            } else {
                theme.muted_style()
            };

            Row::new(vec![
                Cell::from(Span::styled(checkbox, cb_style)),
                Cell::from(Span::styled(format!("{}", i + 1), theme.muted_style())),
                Cell::from(Span::styled(a.kind.description(), desc_style)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Min(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1);

    Widget::render(table, area, buf);
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Render a key-value table with enable/disable checkboxes.
fn render_kv_table(area: Rect, buf: &mut Buffer, pairs: &[KeyValuePair], _title: &str, theme: &Theme) {
    let header = Row::new(vec![
        Cell::from(Span::styled("", Style::default())),
        Cell::from(Span::styled("Key", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Value", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Description", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
    ]);

    let rows: Vec<Row> = pairs
        .iter()
        .map(|kv| {
            let checkbox = if kv.enabled { "\u{2611}" } else { "\u{2610}" };
            let cb_style = if kv.enabled {
                Style::default().fg(theme.colors.success)
            } else {
                theme.muted_style()
            };

            let text_style = if kv.enabled {
                Style::default().fg(theme.colors.foreground)
            } else {
                theme.muted_style()
            };

            let desc = kv.description.as_deref().unwrap_or("");

            Row::new(vec![
                Cell::from(Span::styled(checkbox, cb_style)),
                Cell::from(Span::styled(&kv.key, text_style)),
                Cell::from(Span::styled(&kv.value, text_style)),
                Cell::from(Span::styled(desc, theme.muted_style())),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Percentage(30),
        Constraint::Percentage(40),
        Constraint::Percentage(25),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1);

    Widget::render(table, area, buf);
}

/// Format a labelled field: "Label: value"
fn field_line(label: &str, value: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {}: ", label),
            Style::default().fg(theme.colors.muted),
        ),
        Span::styled(
            value.to_string(),
            Style::default().fg(theme.colors.foreground),
        ),
    ])
}

/// Mask a secret string, keeping only the first 3 characters visible.
fn mask_string(s: &str) -> String {
    if s.len() <= 3 {
        "*".repeat(s.len().max(1))
    } else {
        let visible: String = s.chars().take(3).collect();
        format!("{}{}", visible, "*".repeat(s.len() - 3))
    }
}

/// Very lightweight JSON line colorizer.  This is intentionally simple --
/// it colours keys, string values, numbers, booleans, and null without
/// pulling in a full syntax-highlighting stack for the *request* editor.
/// (The response panel uses syntect for proper highlighting.)
fn colorize_json_line<'a>(line: &'a str, theme: &Theme) -> Span<'a> {
    let trimmed = line.trim();
    let style = if trimmed.starts_with('"') && trimmed.contains(':') {
        // Looks like a JSON key
        Style::default().fg(theme.colors.syntax.json_key)
    } else if trimmed.starts_with('"') {
        Style::default().fg(theme.colors.syntax.json_string)
    } else if trimmed == "true" || trimmed == "false" || trimmed.trim_end_matches(',') == "true" || trimmed.trim_end_matches(',') == "false" {
        Style::default().fg(theme.colors.syntax.json_boolean)
    } else if trimmed == "null" || trimmed.trim_end_matches(',') == "null" {
        Style::default().fg(theme.colors.syntax.json_null)
    } else if trimmed.trim_end_matches(',').parse::<f64>().is_ok() {
        Style::default().fg(theme.colors.syntax.json_number)
    } else {
        Style::default().fg(theme.colors.foreground)
    };

    Span::styled(line, style)
}

fn border_type_from_theme(theme: &Theme) -> BorderType {
    match theme.border_style {
        crate::ui::theme::BorderStyle::Rounded => BorderType::Rounded,
        crate::ui::theme::BorderStyle::Plain => BorderType::Plain,
        crate::ui::theme::BorderStyle::Double => BorderType::Double,
        crate::ui::theme::BorderStyle::Thick => BorderType::Thick,
    }
}
