use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::app::App;
use crate::ui::theme::Theme;

/// Renders the help modal showing a keybinding reference.
///
/// The modal is a centered overlay with two columns:
/// - Left column: Normal mode keybindings grouped by category
/// - Right column: Command mode commands
///
/// The content scrolls if it exceeds the viewport height.
pub fn render_help_modal(app: &App, area: Rect, buf: &mut Buffer) {
    let modal_area = centered_modal(area, 80, 85);

    Clear.render(modal_area, buf);

    let theme = &app.theme;

    let block = Block::default()
        .title(" Help - Keybinding Reference ")
        .title_style(
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(border_type_from_set(theme))
        .border_style(theme.focused_border_style())
        .style(Style::default().bg(theme.colors.background));

    let inner = block.inner(modal_area);
    block.render(modal_area, buf);

    // Split into two columns
    let columns = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(inner);

    let scroll_offset = app.help_scroll;

    render_normal_mode_bindings(theme, columns[0], buf, scroll_offset);
    render_command_mode_bindings(theme, columns[1], buf, scroll_offset);
}

/// Renders the left column: Normal mode keybindings.
fn render_normal_mode_bindings(theme: &Theme, area: Rect, buf: &mut Buffer, scroll: usize) {
    let content_area = area.inner(Margin::new(1, 0));

    let mut lines = Vec::new();

    // Title
    lines.push(Line::from(Span::styled(
        "Normal Mode",
        Style::default()
            .fg(theme.colors.accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )));
    lines.push(Line::from(""));

    // Navigation
    add_section_header(&mut lines, "Navigation", theme);
    add_binding(&mut lines, "j / Down", "Move down", theme);
    add_binding(&mut lines, "k / Up", "Move up", theme);
    add_binding(&mut lines, "h / Left", "Collapse / Move left", theme);
    add_binding(&mut lines, "l / Right", "Expand / Move right", theme);
    add_binding(&mut lines, "J (Shift)", "Page down", theme);
    add_binding(&mut lines, "K (Shift)", "Page up", theme);
    add_binding(&mut lines, "g", "Scroll to top", theme);
    add_binding(&mut lines, "G (Shift)", "Scroll to bottom", theme);
    add_binding(&mut lines, "Tab", "Next focus area", theme);
    add_binding(&mut lines, "Shift+Tab", "Previous focus area", theme);
    lines.push(Line::from(""));

    // Requests
    add_section_header(&mut lines, "Requests", theme);
    add_binding(&mut lines, "Ctrl+R / Enter", "Send request", theme);
    add_binding(&mut lines, "Ctrl+N / t", "New tab", theme);
    add_binding(&mut lines, "w", "Close tab", theme);
    add_binding(&mut lines, "n", "Next tab", theme);
    add_binding(&mut lines, "b", "Previous tab", theme);
    add_binding(&mut lines, "Alt+1..9", "Switch to tab N", theme);
    add_binding(&mut lines, "Ctrl+S / s", "Save request", theme);
    add_binding(&mut lines, "m", "Cycle method/protocol", theme);
    add_binding(&mut lines, "y", "Copy response body", theme);
    add_binding(&mut lines, "1-5", "Switch sub-tab", theme);
    add_binding(&mut lines, "F2", "Rename tab", theme);
    lines.push(Line::from(""));

    // Sidebar
    add_section_header(&mut lines, "Sidebar", theme);
    add_binding(&mut lines, "Enter / l", "Open / Expand", theme);
    add_binding(&mut lines, "h", "Collapse", theme);
    add_binding(&mut lines, "a", "Add request to collection", theme);
    add_binding(&mut lines, "x", "Delete request", theme);
    add_binding(&mut lines, "r", "Rename collection/request", theme);
    lines.push(Line::from(""));

    // Modes
    add_section_header(&mut lines, "Modes", theme);
    add_binding(&mut lines, "i", "Enter Insert mode", theme);
    add_binding(&mut lines, ":", "Enter Command mode", theme);
    add_binding(&mut lines, "/ or p", "Search requests", theme);
    add_binding(&mut lines, "Esc", "Return to Normal mode", theme);
    lines.push(Line::from(""));

    // Tools
    add_section_header(&mut lines, "Tools", theme);
    add_binding(&mut lines, "Ctrl+P", "Fuzzy search", theme);
    add_binding(&mut lines, "e / Ctrl+E", "Cycle environment", theme);
    add_binding(&mut lines, "Ctrl+I", "Import", theme);
    add_binding(&mut lines, "Ctrl+X", "Export", theme);
    add_binding(&mut lines, "d", "Diff selector", theme);
    add_binding(&mut lines, "?", "Show this help", theme);
    lines.push(Line::from(""));

    // Real-time Protocols
    add_section_header(&mut lines, "Real-time Protocols (WS/SSE)", theme);
    add_binding(&mut lines, "m", "Cycle to WS/SSE protocol", theme);
    add_binding(&mut lines, "Enter", "Connect / disconnect toggle", theme);
    add_binding(&mut lines, "i", "WS: type message (Insert)", theme);
    add_binding(&mut lines, "j/k", "Scroll messages / events", theme);
    add_binding(&mut lines, "a", "SSE: toggle accumulated view", theme);
    add_binding(&mut lines, "q", "Disconnect (on response)", theme);
    lines.push(Line::from(""));

    // Global
    add_section_header(&mut lines, "Global", theme);
    add_binding(&mut lines, "q", "Quit (Normal mode)", theme);
    add_binding(&mut lines, "Ctrl+C", "Force quit", theme);

    let paragraph = Paragraph::new(lines)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(theme.colors.background));

    paragraph.render(content_area, buf);
}

/// Renders the right column: Command mode commands.
fn render_command_mode_bindings(theme: &Theme, area: Rect, buf: &mut Buffer, scroll: usize) {
    let content_area = area.inner(Margin::new(1, 0));

    let mut lines = Vec::new();

    // Title
    lines.push(Line::from(Span::styled(
        "Command Mode (:)",
        Style::default()
            .fg(theme.colors.accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )));
    lines.push(Line::from(""));

    // General
    add_section_header(&mut lines, "General", theme);
    add_binding(&mut lines, ":q / :quit", "Quit application", theme);
    add_binding(&mut lines, ":help", "Show this help", theme);
    add_binding(&mut lines, ":theme <name>", "Switch theme", theme);
    add_binding(&mut lines, ":set <key> <val>", "Change setting", theme);
    add_binding(&mut lines, ":rename <name>", "Rename current request", theme);
    lines.push(Line::from(""));

    // Collections
    add_section_header(&mut lines, "Collections", theme);
    add_binding(&mut lines, ":newcol <name>", "Create collection", theme);
    add_binding(&mut lines, ":delcol [name]", "Delete collection", theme);
    add_binding(&mut lines, ":save", "Save request to collection", theme);
    add_binding(&mut lines, ":delreq", "Delete selected request", theme);
    add_binding(&mut lines, ":addvar <k> <v>", "Add collection variable", theme);
    lines.push(Line::from(""));

    // Environment
    add_section_header(&mut lines, "Environment", theme);
    add_binding(&mut lines, ":env <name>", "Set active environment", theme);
    add_binding(&mut lines, ":newenv <name>", "Create environment", theme);
    add_binding(&mut lines, ":dupenv [name]", "Duplicate environment", theme);
    add_binding(&mut lines, ":env-file <path>", "Load .env file", theme);
    lines.push(Line::from(""));

    // Import / Export
    add_section_header(&mut lines, "Import / Export", theme);
    add_binding(&mut lines, ":import <path>", "Import file (auto-detect)", theme);
    add_binding(&mut lines, ":export <path>", "Export (.json/.md/.sh)", theme);
    add_binding(&mut lines, ":curl", "Copy as cURL", theme);
    add_binding(&mut lines, ":paste-curl", "Import cURL from clipboard", theme);
    add_binding(&mut lines, ":docs", "Copy docs to clipboard", theme);
    lines.push(Line::from(""));

    // WebSocket / SSE
    add_section_header(&mut lines, "WebSocket / SSE", theme);
    add_binding(&mut lines, ":ws <url>", "Connect WebSocket", theme);
    add_binding(&mut lines, ":ws-disconnect", "Disconnect WebSocket", theme);
    add_binding(&mut lines, ":sse <url>", "Connect SSE stream", theme);
    add_binding(&mut lines, ":sse-disconnect", "Disconnect SSE stream", theme);
    lines.push(Line::from(""));

    // Testing
    add_section_header(&mut lines, "Testing", theme);
    add_binding(&mut lines, ":loadtest <n> <c>", "Run load test", theme);
    add_binding(&mut lines, ":diff", "Open diff selector", theme);
    lines.push(Line::from(""));

    // Settings keys
    add_section_header(&mut lines, "Settings (:set)", theme);
    add_binding(&mut lines, "timeout <ms>", "Request timeout", theme);
    add_binding(&mut lines, "follow_redirects", "true/false", theme);
    add_binding(&mut lines, "verify_ssl", "true/false", theme);
    add_binding(&mut lines, "vim_mode", "true/false", theme);
    add_binding(&mut lines, "history_limit <n>", "Max history entries", theme);
    add_binding(&mut lines, "theme <name>", "Set theme", theme);
    lines.push(Line::from(""));

    // Misc
    add_section_header(&mut lines, "Other", theme);
    add_binding(&mut lines, ":clearhistory", "Clear request history", theme);
    lines.push(Line::from(""));

    // Available themes
    add_section_header(&mut lines, "Available Themes", theme);
    add_theme_entry(&mut lines, "catppuccin", "(default)", theme);
    add_theme_entry(&mut lines, "dracula", "", theme);
    add_theme_entry(&mut lines, "gruvbox", "", theme);
    add_theme_entry(&mut lines, "tokyo-night", "", theme);
    lines.push(Line::from(""));

    // Footer hint
    lines.push(Line::from(Span::styled(
        "j/k scroll | J/K page | g/G top/bottom | Esc/q close",
        theme.muted_style(),
    )));

    let paragraph = Paragraph::new(lines)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(theme.colors.background));

    paragraph.render(content_area, buf);
}

/// Adds a section header line to the lines vector.
fn add_section_header(lines: &mut Vec<Line<'static>>, title: &str, theme: &Theme) {
    lines.push(Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme.colors.warning)
            .add_modifier(Modifier::BOLD),
    )));
}

/// Adds a keybinding entry line with key and description.
fn add_binding(lines: &mut Vec<Line<'static>>, key: &str, desc: &str, theme: &Theme) {
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {:<20}", key),
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            desc.to_string(),
            Style::default().fg(theme.colors.foreground),
        ),
    ]));
}

/// Adds a theme entry with optional annotation.
fn add_theme_entry(lines: &mut Vec<Line<'static>>, name: &str, note: &str, theme: &Theme) {
    let mut spans = vec![
        Span::styled(
            format!("  {}", name),
            Style::default().fg(theme.colors.foreground),
        ),
    ];
    if !note.is_empty() {
        spans.push(Span::styled(
            format!(" {}", note),
            theme.muted_style(),
        ));
    }
    lines.push(Line::from(spans));
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
