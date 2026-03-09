use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Paragraph, Row, Table, Widget, Wrap,
    },
};
use std::time::Duration;

use crate::app::{App, FocusArea, ResponseTabKind};
use crate::core::request::KeyValuePair;
use crate::core::response::{Cookie, RequestTiming, Response, ResponseBody};
use crate::testing::assertion_engine::{AssertionEngine, AssertionResult};
use crate::ui::theme::Theme;

/// Top-level widget that renders the entire response viewing panel.
///
/// Layout from top to bottom:
///   1. Status line: code, time, size, content-type  (1 row)
///   2. Response sub-tab bar                          (1 row)
///   3. Active sub-tab content                        (remaining space)
///
/// When there is no response yet a full-area placeholder is shown instead.
pub struct ResponsePanel<'a> {
    app: &'a App,
}

impl<'a> ResponsePanel<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for ResponsePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = &self.app.theme;
        let tab = self.app.active_tab();

        let focused = self.app.focus == FocusArea::ResponseBody
            || self.app.focus == FocusArea::ResponseTabs;

        let border_style = if focused {
            theme.focused_border_style()
        } else {
            theme.unfocused_border_style()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type_from_theme(theme))
            .border_style(border_style)
            .title(" Response ")
            .title_style(Style::default().fg(theme.colors.foreground));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        match &tab.response {
            None => render_placeholder(inner, buf, theme, self.app.loading),
            Some(response) => {
                let chunks = Layout::vertical([
                    Constraint::Length(1), // status line
                    Constraint::Length(1), // sub-tab bar
                    Constraint::Min(1),   // sub-tab content
                ])
                .split(inner);

                render_status_line(chunks[0], buf, response, theme);
                render_response_sub_tabs(chunks[1], buf, tab.response_tab, theme, self.app);
                render_sub_tab_content(chunks[2], buf, tab.response_tab, response, theme);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Placeholder when no response is available
// ---------------------------------------------------------------------------

fn render_placeholder(area: Rect, buf: &mut Buffer, theme: &Theme, loading: bool) {
    let message = if loading {
        "Sending request..."
    } else {
        "Send a request to see results"
    };

    let icon = if loading { "\u{23F3} " } else { "\u{2192} " };

    let lines = vec![
        Line::default(),
        Line::default(),
        Line::from(Span::styled(
            format!("{}{}", icon, message),
            Style::default()
                .fg(theme.colors.muted)
                .add_modifier(Modifier::ITALIC),
        )),
        Line::default(),
        if !loading {
            Line::from(Span::styled(
                "Press Ctrl+Enter or click Send",
                theme.muted_style(),
            ))
        } else {
            Line::default()
        },
    ];

    Paragraph::new(lines)
        .alignment(Alignment::Center)
        .render(area, buf);
}

// ---------------------------------------------------------------------------
// Status line:  200 OK  |  142ms  |  3.2 KB  |  application/json
// ---------------------------------------------------------------------------

fn render_status_line(area: Rect, buf: &mut Buffer, response: &Response, theme: &Theme) {
    let status_str = format!(" {} {} ", response.status, &response.status_text);
    let status_style = theme.status_style(response.status);

    let time_str = format!(" {} ", response.timing.format_total());
    let size_str = format!(" {} ", response.size.format());

    let content_type = response
        .content_type()
        .unwrap_or("unknown")
        .to_string();

    let sep = Span::styled(" \u{2502} ", theme.muted_style());

    let spans = vec![
        Span::styled(status_str, status_style),
        sep.clone(),
        Span::styled(time_str, Style::default().fg(theme.colors.foreground)),
        sep.clone(),
        Span::styled(size_str, Style::default().fg(theme.colors.foreground)),
        sep,
        Span::styled(content_type, theme.muted_style()),
    ];

    Paragraph::new(Line::from(spans)).render(area, buf);
}

// ---------------------------------------------------------------------------
// Response sub-tab bar: Body | Headers | Cookies | Timing | Assertions
// ---------------------------------------------------------------------------

fn render_response_sub_tabs(
    area: Rect,
    buf: &mut Buffer,
    active: ResponseTabKind,
    theme: &Theme,
    app: &App,
) {
    let tabs_focused = app.focus == FocusArea::ResponseTabs;
    let mut spans: Vec<Span<'_>> = Vec::new();

    for (i, kind) in ResponseTabKind::all().iter().enumerate() {
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
    kind: ResponseTabKind,
    response: &Response,
    theme: &Theme,
) {
    match kind {
        ResponseTabKind::Body => render_body_tab(area, buf, response, theme),
        ResponseTabKind::Headers => render_headers_tab(area, buf, &response.headers, theme),
        ResponseTabKind::Cookies => render_cookies_tab(area, buf, &response.cookies, theme),
        ResponseTabKind::Timing => render_timing_tab(area, buf, &response.timing, theme),
        ResponseTabKind::Assertions => {
            render_assertions_tab(area, buf, &response.assertion_results, theme)
        }
        // WS/SSE tabs are rendered by their own functions in layout.rs
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Body tab: syntax-highlighted body with line numbers
// ---------------------------------------------------------------------------

fn render_body_tab(area: Rect, buf: &mut Buffer, response: &Response, theme: &Theme) {
    match &response.body {
        ResponseBody::Empty => {
            Paragraph::new(Line::from(Span::styled(
                "(empty response body)",
                theme.muted_style(),
            )))
            .alignment(Alignment::Center)
            .render(area, buf);
        }
        ResponseBody::Binary(data) => {
            let lines = vec![
                Line::from(Span::styled(
                    "Binary data",
                    Style::default()
                        .fg(theme.colors.accent)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::default(),
                Line::from(Span::styled(
                    format!("{} bytes", data.len()),
                    Style::default().fg(theme.colors.foreground),
                )),
                Line::default(),
                Line::from(Span::styled(
                    hex_preview(data, 16),
                    theme.muted_style(),
                )),
            ];
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .render(area, buf);
        }
        ResponseBody::Json(text) => {
            render_highlighted_body(area, buf, text, BodyLanguage::Json, theme);
        }
        ResponseBody::Xml(text) => {
            render_highlighted_body(area, buf, text, BodyLanguage::Xml, theme);
        }
        ResponseBody::Html(text) => {
            render_highlighted_body(area, buf, text, BodyLanguage::Html, theme);
        }
        ResponseBody::Text(text) => {
            render_highlighted_body(area, buf, text, BodyLanguage::Plain, theme);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BodyLanguage {
    Json,
    Xml,
    Html,
    Plain,
}

fn render_highlighted_body(
    area: Rect,
    buf: &mut Buffer,
    text: &str,
    lang: BodyLanguage,
    theme: &Theme,
) {
    if area.height == 0 {
        return;
    }

    let gutter_width: u16 = 5; // "1234 "
    if area.width <= gutter_width + 1 {
        return;
    }

    let gutter_area = Rect {
        width: gutter_width,
        ..area
    };
    let body_area = Rect {
        x: area.x + gutter_width,
        width: area.width.saturating_sub(gutter_width),
        ..area
    };

    let total_lines = text.lines().count().max(1);
    let visible_lines = area.height as usize;

    // Line numbers
    let gutter_lines: Vec<Line<'_>> = (1..=total_lines)
        .take(visible_lines)
        .map(|n| {
            Line::from(Span::styled(
                format!("{:>4} ", n),
                theme.muted_style(),
            ))
        })
        .collect();
    Paragraph::new(gutter_lines).render(gutter_area, buf);

    // Body content with syntax colouring
    let body_lines: Vec<Line<'_>> = text
        .lines()
        .take(visible_lines)
        .map(|line| highlight_line(line, lang, theme))
        .collect();

    Paragraph::new(body_lines).render(body_area, buf);
}

/// Per-line syntax highlighting.
///
/// For JSON we do token-level colouring.  For XML/HTML we distinguish tags
/// from content.  For plain text we use the default foreground.
fn highlight_line<'a>(line: &'a str, lang: BodyLanguage, theme: &Theme) -> Line<'a> {
    match lang {
        BodyLanguage::Json => highlight_json_line(line, theme),
        BodyLanguage::Xml | BodyLanguage::Html => highlight_markup_line(line, theme),
        BodyLanguage::Plain => Line::from(Span::styled(
            line,
            Style::default().fg(theme.colors.foreground),
        )),
    }
}

/// Syntax-colour a single JSON line by splitting it into key / value spans.
fn highlight_json_line<'a>(line: &'a str, theme: &Theme) -> Line<'a> {
    let trimmed = line.trim();

    // Structural characters only (braces, brackets, commas)
    if trimmed.is_empty()
        || trimmed == "{"
        || trimmed == "}"
        || trimmed == "},"
        || trimmed == "["
        || trimmed == "]"
        || trimmed == "],"
    {
        return Line::from(Span::styled(
            line,
            Style::default().fg(theme.colors.foreground),
        ));
    }

    // Try to split "key": value
    if let Some(colon_pos) = find_json_colon(trimmed) {
        let leading_ws = &line[..line.len() - trimmed.len()];
        let key_part = &trimmed[..colon_pos];
        let rest = &trimmed[colon_pos..];

        // Separate ": " from value
        let after_colon = rest.trim_start_matches(':').trim_start();

        let mut spans: Vec<Span<'a>> = Vec::with_capacity(4);

        if !leading_ws.is_empty() {
            spans.push(Span::raw(leading_ws));
        }

        spans.push(Span::styled(
            key_part,
            Style::default().fg(theme.colors.syntax.json_key),
        ));

        spans.push(Span::styled(
            ": ",
            Style::default().fg(theme.colors.foreground),
        ));

        spans.push(colorize_json_value(after_colon, theme));

        // Rebuild the line from the original string is impossible with lifetimes
        // so we work with the trimmed substrings, which borrow from `line` already
        // through `trimmed`. Since `trimmed` is a sub-slice of `line`, the borrows
        // are valid. However `leading_ws` references `line` directly. So we can
        // build spans referencing different parts of the same &'a str.
        //
        // Actually: `trimmed` borrows from `line`, and `key_part` / `rest` borrow
        // from `trimmed`, which is fine.
        return Line::from(spans);
    }

    // Bare value line (e.g. inside an array)
    Line::from(colorize_json_value(trimmed, theme))
}

/// Find the colon separating a JSON key from its value, respecting quoted strings.
fn find_json_colon(s: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escape = false;
    let mut saw_key_end = false;
    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            if in_string {
                in_string = false;
                saw_key_end = true;
            } else {
                in_string = true;
            }
            continue;
        }
        if !in_string && saw_key_end && c == ':' {
            return Some(i);
        }
    }
    None
}

fn colorize_json_value<'a>(value: &'a str, theme: &Theme) -> Span<'a> {
    let v = value.trim().trim_end_matches(',');
    let style = if v.starts_with('"') {
        Style::default().fg(theme.colors.syntax.json_string)
    } else if v == "true" || v == "false" {
        Style::default().fg(theme.colors.syntax.json_boolean)
    } else if v == "null" {
        Style::default().fg(theme.colors.syntax.json_null)
    } else if v.parse::<f64>().is_ok() {
        Style::default().fg(theme.colors.syntax.json_number)
    } else {
        Style::default().fg(theme.colors.foreground)
    };

    Span::styled(value, style)
}

/// Very simple markup highlighter: colour `<tags>` with accent,
/// everything else as foreground text.
fn highlight_markup_line<'a>(line: &'a str, theme: &Theme) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut pos = 0;
    let bytes = line.as_bytes();

    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            // find matching >
            if let Some(end) = line[pos..].find('>') {
                let tag = &line[pos..pos + end + 1];
                spans.push(Span::styled(
                    tag,
                    Style::default().fg(theme.colors.accent),
                ));
                pos += end + 1;
            } else {
                spans.push(Span::styled(
                    &line[pos..],
                    Style::default().fg(theme.colors.foreground),
                ));
                break;
            }
        } else {
            // find next <
            let next_tag = line[pos..].find('<').unwrap_or(line.len() - pos);
            let text = &line[pos..pos + next_tag];
            if !text.is_empty() {
                spans.push(Span::styled(
                    text,
                    Style::default().fg(theme.colors.foreground),
                ));
            }
            pos += next_tag;
        }
    }

    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Headers tab
// ---------------------------------------------------------------------------

fn render_headers_tab(area: Rect, buf: &mut Buffer, headers: &[KeyValuePair], theme: &Theme) {
    if headers.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "(no headers)",
            theme.muted_style(),
        )))
        .alignment(Alignment::Center)
        .render(area, buf);
        return;
    }

    let rows: Vec<Row> = headers
        .iter()
        .map(|kv| {
            Row::new(vec![
                Cell::from(Span::styled(
                    &kv.key,
                    Style::default()
                        .fg(theme.colors.accent)
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    &kv.value,
                    Style::default().fg(theme.colors.foreground),
                )),
            ])
        })
        .collect();

    let widths = [Constraint::Percentage(35), Constraint::Percentage(65)];

    let header = Row::new(vec![
        Cell::from(Span::styled(
            "Name",
            Style::default()
                .fg(theme.colors.muted)
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Value",
            Style::default()
                .fg(theme.colors.muted)
                .add_modifier(Modifier::BOLD),
        )),
    ]);

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(2);

    Widget::render(table, area, buf);
}

// ---------------------------------------------------------------------------
// Cookies tab
// ---------------------------------------------------------------------------

fn render_cookies_tab(area: Rect, buf: &mut Buffer, cookies: &[Cookie], theme: &Theme) {
    if cookies.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "(no cookies)",
            theme.muted_style(),
        )))
        .alignment(Alignment::Center)
        .render(area, buf);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("Name", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Value", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Domain", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Path", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Flags", Style::default().fg(theme.colors.muted).add_modifier(Modifier::BOLD))),
    ]);

    let rows: Vec<Row> = cookies
        .iter()
        .map(|c| {
            let mut flags = Vec::new();
            if c.http_only {
                flags.push("HttpOnly");
            }
            if c.secure {
                flags.push("Secure");
            }
            let flags_str = flags.join(", ");

            Row::new(vec![
                Cell::from(Span::styled(&c.name, Style::default().fg(theme.colors.accent))),
                Cell::from(Span::styled(&c.value, Style::default().fg(theme.colors.foreground))),
                Cell::from(Span::styled(
                    c.domain.as_deref().unwrap_or("-"),
                    theme.muted_style(),
                )),
                Cell::from(Span::styled(
                    c.path.as_deref().unwrap_or("/"),
                    theme.muted_style(),
                )),
                Cell::from(Span::styled(flags_str, theme.muted_style())),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(18),
        Constraint::Percentage(30),
        Constraint::Percentage(20),
        Constraint::Percentage(12),
        Constraint::Percentage(20),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1);

    Widget::render(table, area, buf);
}

// ---------------------------------------------------------------------------
// Timing tab: request timing breakdown
// ---------------------------------------------------------------------------

fn render_timing_tab(area: Rect, buf: &mut Buffer, timing: &RequestTiming, theme: &Theme) {
    let total_ms = timing.total.as_millis() as f64;

    let phases: Vec<(&str, Duration, Style)> = vec![
        (
            "DNS Lookup",
            timing.dns_lookup,
            Style::default().fg(theme.colors.syntax.json_string),
        ),
        (
            "TCP Connect",
            timing.tcp_connect,
            Style::default().fg(theme.colors.syntax.json_number),
        ),
        (
            "TLS Handshake",
            timing.tls_handshake.unwrap_or_default(),
            Style::default().fg(theme.colors.syntax.json_boolean),
        ),
        (
            "Time to First Byte",
            timing.first_byte,
            Style::default().fg(theme.colors.accent),
        ),
        (
            "Content Download",
            timing.content_download,
            Style::default().fg(theme.colors.syntax.json_key),
        ),
    ];

    // We need enough width for the bar chart
    let bar_max_width = area.width.saturating_sub(30) as f64;

    let mut lines: Vec<Line<'_>> = Vec::new();

    // Title
    lines.push(Line::from(Span::styled(
        format!("Total: {}", timing.format_total()),
        Style::default()
            .fg(theme.colors.foreground)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::default());

    for (label, duration, style) in &phases {
        let ms = duration.as_millis();
        let bar_len = if total_ms > 0.0 {
            ((ms as f64 / total_ms) * bar_max_width).round() as usize
        } else {
            0
        };
        let bar = "\u{2588}".repeat(bar_len.max(if ms > 0 { 1 } else { 0 }));

        let label_span = Span::styled(
            format!("{:<20}", label),
            Style::default().fg(theme.colors.muted),
        );
        let time_span = Span::styled(
            format!("{:>6}ms ", ms),
            Style::default().fg(theme.colors.foreground),
        );
        let bar_span = Span::styled(bar, *style);

        lines.push(Line::from(vec![label_span, time_span, bar_span]));
    }

    lines.push(Line::default());

    // Summary bar
    let summary_bar_len = bar_max_width as usize;
    if summary_bar_len > 0 && total_ms > 0.0 {
        let mut bar_spans: Vec<Span<'_>> = Vec::new();
        for (_, duration, style) in &phases {
            let ms = duration.as_millis() as f64;
            let segment_len = ((ms / total_ms) * summary_bar_len as f64).round() as usize;
            if segment_len > 0 {
                bar_spans.push(Span::styled("\u{2588}".repeat(segment_len), *style));
            }
        }
        lines.push(Line::from(bar_spans));
    }

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

// ---------------------------------------------------------------------------
// Assertions tab: test results with pass/fail
// ---------------------------------------------------------------------------

fn render_assertions_tab(area: Rect, buf: &mut Buffer, results: &[AssertionResult], theme: &Theme) {
    if results.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "No assertions were run.",
            theme.muted_style(),
        )))
        .alignment(Alignment::Center)
        .render(area, buf);
        return;
    }

    let (passed, total) = AssertionEngine::summary(results);

    // Summary header
    let summary_style = if passed == total {
        theme.success_style().add_modifier(Modifier::BOLD)
    } else {
        theme.error_style().add_modifier(Modifier::BOLD)
    };

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" {}/{} assertions passed", passed, total),
        summary_style,
    )));
    lines.push(Line::default());

    for result in results {
        let (icon, icon_style) = if result.passed {
            ("\u{2713}", theme.success_style())
        } else {
            ("\u{2717}", theme.error_style())
        };

        let desc = result.assertion.kind.description();
        let msg_style = if result.passed {
            Style::default().fg(theme.colors.foreground)
        } else {
            Style::default().fg(theme.colors.error)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", icon), icon_style),
            Span::styled(desc, msg_style),
        ]));

        // Show actual value or failure detail on the next line
        if !result.passed {
            lines.push(Line::from(Span::styled(
                format!("      {}", result.message),
                theme.muted_style(),
            )));
            if let Some(actual) = &result.actual_value {
                lines.push(Line::from(Span::styled(
                    format!("      actual: {}", actual),
                    theme.muted_style(),
                )));
            }
        }
    }

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a short hex preview of binary data (like a hex dump first line).
fn hex_preview(data: &[u8], max_bytes: usize) -> String {
    let take = data.len().min(max_bytes);
    let hex: Vec<String> = data[..take].iter().map(|b| format!("{:02X}", b)).collect();
    let ascii: String = data[..take]
        .iter()
        .map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '.' })
        .collect();
    format!("{} | {}", hex.join(" "), ascii)
}

fn border_type_from_theme(theme: &Theme) -> BorderType {
    match theme.border_style {
        crate::ui::theme::BorderStyle::Rounded => BorderType::Rounded,
        crate::ui::theme::BorderStyle::Plain => BorderType::Plain,
        crate::ui::theme::BorderStyle::Double => BorderType::Double,
        crate::ui::theme::BorderStyle::Thick => BorderType::Thick,
    }
}
