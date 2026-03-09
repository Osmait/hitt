use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Widget},
};

use crate::app::App;
use crate::testing::assertion_engine::{AssertionEngine, AssertionResult};
use crate::ui::theme::Theme;

/// Renders the assertion results panel for the active tab's response.
///
/// The panel displays:
/// - A summary header showing the number of passed vs total assertions
/// - A list of individual assertions with pass/fail indicators,
///   the assertion description, and (when applicable) the actual value
///
/// Passed assertions are styled in green; failed assertions in red.
pub fn render_assertion_panel(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;
    let tab = app.active_tab();

    let block = Block::default()
        .title(" Assertions ")
        .title_style(Style::default().fg(theme.colors.foreground))
        .borders(Borders::ALL)
        .border_type(border_type_from_set(theme))
        .border_style(theme.unfocused_border_style())
        .style(Style::default().bg(theme.colors.background));

    let inner = block.inner(area);
    block.render(area, buf);

    match &tab.response {
        Some(response) if !response.assertion_results.is_empty() => {
            render_results_content(&response.assertion_results, theme, inner, buf);
        }
        Some(_) => {
            render_empty_state("No assertions defined for this request", theme, inner, buf);
        }
        None => {
            render_empty_state("Send a request to see assertion results", theme, inner, buf);
        }
    }
}

/// Renders the assertion results: a summary line followed by the individual items.
fn render_results_content(
    results: &[AssertionResult],
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let (passed, total) = AssertionEngine::summary(results);

    // Split into summary header and list body
    let chunks = Layout::vertical([
        Constraint::Length(2), // summary + separator
        Constraint::Min(1),   // assertion list
    ])
    .split(area);

    render_summary(passed, total, theme, chunks[0], buf);
    render_assertion_list(results, theme, chunks[1], buf);
}

/// Renders the summary line: "X/Y passed" with a colored progress indicator.
fn render_summary(passed: usize, total: usize, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let all_passed = passed == total;

    let summary_style = if all_passed {
        Style::default()
            .fg(theme.colors.success)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.colors.error)
            .add_modifier(Modifier::BOLD)
    };

    let icon = if all_passed { "+" } else { "!" };
    let status_text = if all_passed {
        "All passed"
    } else {
        "Some failed"
    };

    // Build a simple visual progress bar
    let bar_width = 20usize.min((area.width as usize).saturating_sub(30));
    let filled = if total > 0 {
        (bar_width * passed) / total
    } else {
        0
    };
    let empty = bar_width - filled;

    let bar = format!(
        "[{}{}]",
        "#".repeat(filled),
        "-".repeat(empty),
    );

    let bar_style = if all_passed {
        Style::default().fg(theme.colors.success)
    } else if passed > 0 {
        Style::default().fg(theme.colors.warning)
    } else {
        Style::default().fg(theme.colors.error)
    };

    let line = Line::from(vec![
        Span::styled(format!(" [{}] ", icon), summary_style),
        Span::styled(
            format!("{}/{} passed", passed, total),
            summary_style,
        ),
        Span::styled(format!("  {} ", status_text), summary_style),
        Span::styled(bar, bar_style),
    ]);

    let paragraph = Paragraph::new(line);
    paragraph.render(area, buf);
}

/// Renders the list of individual assertion results.
fn render_assertion_list(
    results: &[AssertionResult],
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let items: Vec<ListItem> = results
        .iter()
        .map(|result| {
            let (icon, icon_style) = if result.passed {
                (
                    " + ",
                    Style::default()
                        .fg(theme.colors.success)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    " x ",
                    Style::default()
                        .fg(theme.colors.error)
                        .add_modifier(Modifier::BOLD),
                )
            };

            let desc = result.assertion.kind.description();

            let desc_style = if result.passed {
                Style::default().fg(theme.colors.success)
            } else {
                Style::default().fg(theme.colors.error)
            };

            let message_style = if result.passed {
                theme.muted_style()
            } else {
                Style::default()
                    .fg(theme.colors.error)
                    .add_modifier(Modifier::DIM)
            };

            // Build spans for the assertion line
            let mut spans = vec![
                Span::styled(icon.to_string(), icon_style),
                Span::styled(desc, desc_style),
            ];

            // Append the actual value if present
            if let Some(actual) = &result.actual_value {
                spans.push(Span::styled(
                    format!("  (actual: {})", actual),
                    message_style,
                ));
            }

            // For failed assertions, show the failure message on a second line
            if !result.passed {
                let lines = vec![
                    Line::from(spans),
                    Line::from(vec![
                        Span::styled("     ", Style::default()),
                        Span::styled(result.message.clone(), message_style),
                    ]),
                ];
                ListItem::new(lines)
            } else {
                ListItem::new(Line::from(spans))
            }
        })
        .collect();

    let list = List::new(items).style(Style::default().bg(theme.colors.background));
    list.render(area, buf);
}

/// Renders an empty state message when there are no results to display.
fn render_empty_state(message: &str, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let line = Line::from(Span::styled(message, theme.muted_style()));
    let paragraph = Paragraph::new(line).style(Style::default().bg(theme.colors.background));
    paragraph.render(area.inner(Margin::new(1, 1)), buf);
}

/// Maps the theme's border style to a ratatui `BorderType`.
fn border_type_from_set(theme: &Theme) -> BorderType {
    use crate::ui::theme::BorderStyle;
    match theme.border_style {
        BorderStyle::Rounded => BorderType::Rounded,
        BorderStyle::Plain => BorderType::Plain,
        BorderStyle::Double => BorderType::Double,
        BorderStyle::Thick => BorderType::Thick,
    }
}
