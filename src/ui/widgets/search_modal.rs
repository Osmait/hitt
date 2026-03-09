use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Widget,
    },
};

use crate::app::App;
use crate::ui::theme::Theme;

/// The maximum number of visible search results in the list.
const MAX_VISIBLE_RESULTS: usize = 12;

/// Renders the fuzzy search modal (triggered by Ctrl+P).
///
/// The modal is a centered overlay that contains:
/// - A text input at the top for the search query
/// - A results list below showing matching requests with method badge,
///   request name, URL, and collection name
/// - The currently selected result is highlighted
pub fn render_search_modal(app: &App, area: Rect, buf: &mut Buffer) {
    let modal_area = centered_modal(area, 60, 50);

    // Clear the area behind the modal
    Clear.render(modal_area, buf);

    let theme = &app.theme;

    let block = Block::default()
        .title(" Search Requests (Ctrl+P) ")
        .title_style(Style::default().fg(theme.colors.accent).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(border_type_from_set(theme))
        .border_style(theme.focused_border_style())
        .style(Style::default().bg(theme.colors.background));

    let inner = block.inner(modal_area);
    block.render(modal_area, buf);

    // Split inner area into search input and results list
    let chunks = Layout::vertical([
        Constraint::Length(3), // search input
        Constraint::Min(1),   // results list
    ])
    .split(inner);

    render_search_input(app, theme, chunks[0], buf);
    render_search_results(app, theme, chunks[1], buf);
}

/// Renders the search text input field with the current query.
fn render_search_input(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let query = &app.search_query;

    // Build the input line: magnifying glass icon + query + cursor
    let input_spans = vec![
        Span::styled(" / ", Style::default().fg(theme.colors.muted)),
        Span::styled(query.as_str(), Style::default().fg(theme.colors.foreground)),
        Span::styled(
            "_",
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ];

    let input = Paragraph::new(Line::from(input_spans)).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.colors.muted))
            .style(Style::default().bg(theme.colors.background)),
    );

    input.render(area, buf);
}

/// Renders the list of search results below the input.
fn render_search_results(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    if app.search_results.is_empty() {
        let msg = if app.search_query.is_empty() {
            "Type to search requests..."
        } else {
            "No matching requests found"
        };
        let empty = Paragraph::new(Line::from(Span::styled(
            msg,
            theme.muted_style(),
        )))
        .style(Style::default().bg(theme.colors.background));
        empty.render(area.inner(Margin::new(1, 1)), buf);
        return;
    }

    // Determine the selected index (default to 0 if the field is absent).
    let selected = app
        .search_selected
        .min(app.search_results.len().saturating_sub(1));

    // Compute scroll offset to keep selected item in view
    let visible = (area.height as usize).min(MAX_VISIBLE_RESULTS);
    let scroll_offset = if selected >= visible {
        selected - visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible)
        .map(|(idx, result)| {
            let is_selected = idx == selected;

            // Method badge
            let method_str = result
                .method
                .as_ref()
                .map(|m| m.as_str())
                .unwrap_or("???");
            let method_style = result
                .method
                .as_ref()
                .map(|m| theme.method_style(m))
                .unwrap_or_else(|| theme.muted_style())
                .add_modifier(Modifier::BOLD);

            // Pad method to fixed width for alignment
            let method_padded = format!("{:<7}", method_str);

            // Collection name suffix
            let collection_label = result
                .collection_name
                .as_deref()
                .unwrap_or("");

            let base_style = if is_selected {
                theme.selected_style()
            } else {
                Style::default().bg(theme.colors.background)
            };

            let name_style = if is_selected {
                base_style.add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(theme.colors.foreground)
                    .bg(theme.colors.background)
            };

            let url_style = if is_selected {
                base_style
            } else {
                theme.muted_style().bg(theme.colors.background)
            };

            let coll_style = if is_selected {
                base_style
            } else {
                Style::default()
                    .fg(theme.colors.accent)
                    .bg(theme.colors.background)
                    .add_modifier(Modifier::DIM)
            };

            // Truncate URL to fit available space
            let available_width = area.width as usize;
            let url_display = truncate_str(&result.url, available_width.saturating_sub(20));

            let line = Line::from(vec![
                Span::styled(format!(" {} ", method_padded), method_style),
                Span::styled(result.name.clone(), name_style),
                Span::styled("  ", base_style),
                Span::styled(url_display, url_style),
                Span::styled("  ", base_style),
                Span::styled(collection_label.to_string(), coll_style),
            ]);

            ListItem::new(line).style(base_style)
        })
        .collect();

    // Result count header
    let count_line = Line::from(vec![
        Span::styled(
            format!(" {} result{} ",
                app.search_results.len(),
                if app.search_results.len() == 1 { "" } else { "s" }
            ),
            theme.muted_style(),
        ),
    ]);

    let count_paragraph = Paragraph::new(count_line)
        .style(Style::default().bg(theme.colors.background));

    // Split the results area to show count header + list
    let results_chunks = Layout::vertical([
        Constraint::Length(1), // count
        Constraint::Min(1),   // list items
    ])
    .split(area);

    count_paragraph.render(results_chunks[0], buf);

    let list = List::new(items).style(Style::default().bg(theme.colors.background));
    list.render(results_chunks[1], buf);
}

/// Creates a centered rectangle with the given percentage width and height.
fn centered_modal(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
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

/// Truncates a string to at most `max_len` characters, appending an ellipsis
/// if truncation occurs.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}
