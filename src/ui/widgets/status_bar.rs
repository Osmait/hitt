use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::app::{App, AppMode, ModalKind, NavMode, NotificationKind};
use crate::ui::theme::Theme;

/// Spinner animation frames used when a request is in flight.
const SPINNER_FRAMES: &[&str] = &["   ", ".  ", ".. ", "...", " ..", "  .", "   "];

/// Renders the bottom status bar.
///
/// The status bar displays:
/// - Current mode indicator (Normal/Insert/Command) on the left
/// - Keybinding hints in the center
/// - In command mode: `:` prefix followed by the command input
/// - A loading spinner when a request is in flight
/// - Notification messages colored by severity (info/success/warning/error)
pub fn render_status_bar(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;

    // In command mode, render the command input line instead of the normal bar
    if app.mode == AppMode::Command {
        render_command_line(app, theme, area, buf);
        return;
    }

    // Split the bar into: [mode] [hints/notification] [loading]
    let chunks = Layout::horizontal([
        Constraint::Length(mode_label_width(&app.mode, app.nav_mode)),
        Constraint::Min(1),
        Constraint::Length(if app.loading { 6 } else { 0 }),
    ])
    .split(area);

    // Mode indicator
    render_mode_indicator(app, theme, chunks[0], buf);

    // Center section: either notification or keybinding hints
    if let Some(notification) = &app.notification {
        render_notification(notification, theme, chunks[1], buf);
    } else {
        render_keybinding_hints(app, theme, chunks[1], buf);
    }

    // Loading spinner
    if app.loading {
        render_spinner(app, theme, chunks[2], buf);
    }
}

/// Renders the mode indicator badge on the left side of the status bar.
fn render_mode_indicator(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let (label, style) = mode_display(&app.mode, app.nav_mode, theme);

    let paragraph = Paragraph::new(Line::from(vec![Span::styled(label, style)]));
    paragraph.render(area, buf);
}

/// Returns the display label and style for the current mode.
fn mode_display(mode: &AppMode, nav_mode: NavMode, theme: &Theme) -> (String, Style) {
    match mode {
        AppMode::Normal => match nav_mode {
            NavMode::Global => (
                " GLOBAL ".to_string(),
                Style::default()
                    .fg(theme.colors.background)
                    .bg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            NavMode::Panel => (
                " PANEL ".to_string(),
                Style::default()
                    .fg(theme.colors.background)
                    .bg(theme.colors.warning)
                    .add_modifier(Modifier::BOLD),
            ),
        },
        AppMode::Insert => (
            " INSERT ".to_string(),
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.success)
                .add_modifier(Modifier::BOLD),
        ),
        AppMode::Command => (
            " COMMAND ".to_string(),
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        AppMode::Modal(kind) => {
            let label = match kind {
                ModalKind::Search => " SEARCH ",
                ModalKind::Help => " HELP ",
                ModalKind::EnvironmentEdit => " ENV EDIT ",
                ModalKind::Confirm(_) => " CONFIRM ",
                ModalKind::Import => " IMPORT ",
                ModalKind::Export => " EXPORT ",
                ModalKind::LoadTestConfig => " LOAD TEST ",
                ModalKind::DiffSelector => " DIFF ",
                ModalKind::CurlImport => " CURL ",
                ModalKind::RenameTab => " RENAME ",
                ModalKind::CollectionPicker => " PICK COLLECTION ",
                ModalKind::RenameCollection(_) => " RENAME COLLECTION ",
                ModalKind::RenameRequest { .. } => " RENAME REQUEST ",
            };
            (
                label.to_string(),
                Style::default()
                    .fg(theme.colors.background)
                    .bg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD),
            )
        }
        AppMode::ChainEditor => (
            " CHAIN ".to_string(),
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        AppMode::ProxyInspector => (
            " PROXY ".to_string(),
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.error)
                .add_modifier(Modifier::BOLD),
        ),
    }
}

/// Returns the width needed for the mode label.
fn mode_label_width(mode: &AppMode, nav_mode: NavMode) -> u16 {
    let (label, ()) = mode_display_label(mode, nav_mode);
    label.len() as u16
}

/// Returns just the label string for width calculation without allocating a Style.
fn mode_display_label(mode: &AppMode, nav_mode: NavMode) -> (&'static str, ()) {
    match mode {
        AppMode::Normal => match nav_mode {
            NavMode::Global => (" GLOBAL ", ()),
            NavMode::Panel => (" PANEL ", ()),
        },
        AppMode::Insert => (" INSERT ", ()),
        AppMode::Command => (" COMMAND ", ()),
        AppMode::Modal(kind) => {
            let label = match kind {
                ModalKind::Search => " SEARCH ",
                ModalKind::Help => " HELP ",
                ModalKind::EnvironmentEdit => " ENV EDIT ",
                ModalKind::Confirm(_) => " CONFIRM ",
                ModalKind::Import => " IMPORT ",
                ModalKind::Export => " EXPORT ",
                ModalKind::LoadTestConfig => " LOAD TEST ",
                ModalKind::DiffSelector => " DIFF ",
                ModalKind::CurlImport => " CURL ",
                ModalKind::RenameTab => " RENAME ",
                ModalKind::CollectionPicker => " PICK COLLECTION ",
                ModalKind::RenameCollection(_) => " RENAME COLLECTION ",
                ModalKind::RenameRequest { .. } => " RENAME REQUEST ",
            };
            (label, ())
        }
        AppMode::ChainEditor => (" CHAIN ", ()),
        AppMode::ProxyInspector => (" PROXY ", ()),
    }
}

/// Renders the keybinding hints in the center of the status bar.
fn render_keybinding_hints(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let hints = build_hints(app, theme);
    let paragraph = Paragraph::new(Line::from(hints));
    paragraph.render(area, buf);
}

/// Builds the keybinding hint spans based on the current mode and focus.
fn build_hints<'a>(app: &App, theme: &'a Theme) -> Vec<Span<'a>> {
    let key_style = Style::default()
        .fg(theme.colors.accent)
        .add_modifier(Modifier::BOLD);
    let sep_style = theme.muted_style();
    let desc_style = Style::default().fg(theme.colors.foreground);

    let mut spans = vec![Span::styled(" ", sep_style)];

    match &app.mode {
        AppMode::Normal => {
            if app.nav_mode == NavMode::Global {
                add_hint(&mut spans, "hjkl", "Nav", key_style, desc_style, sep_style);
                add_hint(
                    &mut spans, "Enter", "Focus", key_style, desc_style, sep_style,
                );
                add_hint(&mut spans, "Tab", "Cycle", key_style, desc_style, sep_style);
                add_hint(&mut spans, "q", "Quit", key_style, desc_style, sep_style);
                add_hint(&mut spans, "?", "Help", key_style, desc_style, sep_style);
            } else {
                add_hint(
                    &mut spans, "Esc", "Global", key_style, desc_style, sep_style,
                );
                if app.focus == crate::app::FocusArea::Sidebar {
                    add_hint(
                        &mut spans, "Enter/l", "Open", key_style, desc_style, sep_style,
                    );
                    add_hint(
                        &mut spans, "h", "Collapse", key_style, desc_style, sep_style,
                    );
                    add_hint(&mut spans, "j/k", "Nav", key_style, desc_style, sep_style);
                    add_hint(&mut spans, "a", "Add", key_style, desc_style, sep_style);
                    add_hint(&mut spans, "x", "Del", key_style, desc_style, sep_style);
                    add_hint(&mut spans, "r", "Rename", key_style, desc_style, sep_style);
                } else {
                    add_hint(
                        &mut spans, "Enter", "Send", key_style, desc_style, sep_style,
                    );
                    add_hint(&mut spans, "/", "Search", key_style, desc_style, sep_style);
                    add_hint(&mut spans, "e", "Env", key_style, desc_style, sep_style);
                    add_hint(&mut spans, "i", "Insert", key_style, desc_style, sep_style);
                    add_hint(&mut spans, ":", "Cmd", key_style, desc_style, sep_style);
                }
                add_hint(&mut spans, "?", "Help", key_style, desc_style, sep_style);
            }
        }
        AppMode::Insert => {
            add_hint(
                &mut spans, "Esc", "Normal", key_style, desc_style, sep_style,
            );
            add_hint(
                &mut spans, "Enter", "Send", key_style, desc_style, sep_style,
            );
            add_hint(&mut spans, "Tab", "Next", key_style, desc_style, sep_style);
        }
        AppMode::Modal(ModalKind::Search) => {
            add_hint(
                &mut spans, "Enter", "Select", key_style, desc_style, sep_style,
            );
            add_hint(
                &mut spans, "Up/Down", "Navigate", key_style, desc_style, sep_style,
            );
            add_hint(&mut spans, "Esc", "Close", key_style, desc_style, sep_style);
        }
        AppMode::Modal(ModalKind::Help) => {
            add_hint(
                &mut spans, "j/k", "Scroll", key_style, desc_style, sep_style,
            );
            add_hint(
                &mut spans, "Esc/q", "Close", key_style, desc_style, sep_style,
            );
        }
        AppMode::Modal(_) => {
            add_hint(&mut spans, "Esc", "Close", key_style, desc_style, sep_style);
        }
        _ => {
            add_hint(&mut spans, "Esc", "Back", key_style, desc_style, sep_style);
        }
    }

    // Right side: focus area + tab count + collection count + env
    let focus_label = match app.focus {
        crate::app::FocusArea::Sidebar => "Sidebar",
        crate::app::FocusArea::UrlBar => "URL",
        crate::app::FocusArea::RequestTabs => "ReqTabs",
        crate::app::FocusArea::RequestBody => "ReqBody",
        crate::app::FocusArea::ResponseBody => "RespBody",
        crate::app::FocusArea::ResponseTabs => "RespTabs",
        crate::app::FocusArea::ChainSteps => "Chain",
        crate::app::FocusArea::ProxyList => "Proxy",
    };
    spans.push(Span::styled(
        format!("  {focus_label} "),
        Style::default().fg(theme.colors.foreground),
    ));

    let tab_count = app.tabs.len();
    let coll_count = app.collections.len();
    if tab_count > 0 || coll_count > 0 {
        spans.push(Span::styled(
            format!(
                " {} tab{}",
                tab_count,
                if tab_count == 1 { "" } else { "s" }
            ),
            sep_style,
        ));
    }
    if coll_count > 0 {
        spans.push(Span::styled(
            format!(
                "  {} collection{}",
                coll_count,
                if coll_count == 1 { "" } else { "s" }
            ),
            sep_style,
        ));
    }

    if let Some(env) = app.active_environment() {
        spans.push(Span::styled("  ", sep_style));
        spans.push(Span::styled(
            format!("env:{}", env.name),
            Style::default()
                .fg(theme.colors.success)
                .add_modifier(Modifier::DIM),
        ));
    }

    spans
}

/// Appends a single keybinding hint: `key desc | `
fn add_hint<'a>(
    spans: &mut Vec<Span<'a>>,
    key: &'a str,
    desc: &'a str,
    key_style: Style,
    desc_style: Style,
    sep_style: Style,
) {
    spans.push(Span::styled(key, key_style));
    spans.push(Span::styled(" ", sep_style));
    spans.push(Span::styled(desc, desc_style));
    spans.push(Span::styled(" | ", sep_style));
}

/// Renders a notification message with color based on severity.
fn render_notification(
    notification: &crate::app::Notification,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let (icon, style) = match &notification.kind {
        NotificationKind::Info => (
            "i",
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        NotificationKind::Success => (
            "+",
            Style::default()
                .fg(theme.colors.success)
                .add_modifier(Modifier::BOLD),
        ),
        NotificationKind::Warning => (
            "!",
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        NotificationKind::Error => (
            "x",
            Style::default()
                .fg(theme.colors.error)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let msg_style = match &notification.kind {
        NotificationKind::Info => Style::default().fg(theme.colors.accent),
        NotificationKind::Success => Style::default().fg(theme.colors.success),
        NotificationKind::Warning => Style::default().fg(theme.colors.warning),
        NotificationKind::Error => Style::default().fg(theme.colors.error),
    };

    let line = Line::from(vec![
        Span::styled(format!(" [{icon}] "), style),
        Span::styled(notification.message.clone(), msg_style),
    ]);

    let paragraph = Paragraph::new(line);
    paragraph.render(area, buf);
}

/// Renders the command mode input line showing `:` prefix + current input.
fn render_command_line(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let line = Line::from(vec![
        Span::styled(
            ":",
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.command_input.as_str(),
            Style::default().fg(theme.colors.foreground),
        ),
        Span::styled(
            "_",
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);

    let paragraph = Paragraph::new(line).style(Style::default().bg(theme.colors.background));
    paragraph.render(area, buf);
}

/// Renders a simple text-based loading spinner.
fn render_spinner(_app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    // Use the notification creation time or a simple tick counter
    // to animate the spinner. We derive the frame index from the
    // elapsed time since the app started loading.
    let frame_idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        / 200) as usize
        % SPINNER_FRAMES.len();

    let spinner_char = SPINNER_FRAMES[frame_idx];

    let line = Line::from(Span::styled(
        format!(" {spinner_char} "),
        Style::default()
            .fg(theme.colors.accent)
            .add_modifier(Modifier::BOLD),
    ));

    let paragraph = Paragraph::new(line);
    paragraph.render(area, buf);
}
