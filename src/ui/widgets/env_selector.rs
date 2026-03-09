use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::app::App;
use crate::ui::theme::Theme;

/// Renders the environment selector widget.
///
/// This widget displays the currently active environment name in a compact
/// inline format suitable for embedding in a header bar or sidebar.
/// When no environment is selected, it shows a muted "No Environment" label.
///
/// The display uses a dropdown-style appearance with a chevron indicator
/// to suggest that the environment can be changed (via `Ctrl+E`).
pub fn render_env_selector(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;

    let line = build_env_selector_line(app, theme);
    let paragraph = Paragraph::new(line);
    paragraph.render(area, buf);
}

/// Builds the styled line for the environment selector.
fn build_env_selector_line<'a>(app: &App, theme: &'a Theme) -> Line<'a> {
    let env_icon_style = Style::default()
        .fg(theme.colors.accent)
        .add_modifier(Modifier::BOLD);
    let chevron_style = theme.muted_style();

    match app.active_environment() {
        Some(env) => {
            let env_count = app.environments.len();
            let active_idx = app.active_env.unwrap_or(0) + 1;

            Line::from(vec![
                Span::styled(" ENV ", env_icon_style),
                Span::styled(
                    env.name.clone(),
                    Style::default()
                        .fg(theme.colors.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" ({}/{})", active_idx, env_count),
                    theme.muted_style(),
                ),
                Span::styled(" v ", chevron_style),
            ])
        }
        None => Line::from(vec![
            Span::styled(" ENV ", env_icon_style),
            Span::styled("No Environment", theme.muted_style()),
            Span::styled(" v ", chevron_style),
        ]),
    }
}

/// Renders an expanded environment dropdown list.
///
/// This function renders all available environments as a vertical list,
/// highlighting the currently active one. It is intended to be rendered
/// as an overlay or within a dedicated panel area when the user triggers
/// the environment selector dropdown.
///
/// Each environment entry shows:
/// - A selection indicator for the active environment
/// - The environment name
/// - The count of variables defined in that environment
pub fn render_env_dropdown(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = &app.theme;

    if app.environments.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            " No environments configured",
            theme.muted_style(),
        )));
        empty.render(area, buf);
        return;
    }

    let active_idx = app.active_env;

    // Render each environment as a line
    let lines: Vec<Line> = app
        .environments
        .iter()
        .enumerate()
        .take(area.height as usize)
        .map(|(idx, env)| {
            let is_active = active_idx == Some(idx);

            let indicator = if is_active { " > " } else { "   " };
            let indicator_style = if is_active {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.muted_style()
            };

            let name_style = if is_active {
                Style::default()
                    .fg(theme.colors.success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.colors.foreground)
            };

            let var_count = env.values.iter().filter(|v| v.enabled).count();
            let var_label = format!(" ({} vars)", var_count);

            Line::from(vec![
                Span::styled(indicator.to_string(), indicator_style),
                Span::styled(env.name.clone(), name_style),
                Span::styled(var_label, theme.muted_style()),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).style(Style::default().bg(theme.colors.background));
    paragraph.render(area, buf);
}
