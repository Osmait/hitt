use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::app::App;

/// Renders the top tab bar showing all open request tabs.
///
/// Each tab displays the request name (or "METHOD url" when unnamed).
/// The active tab is highlighted with the accent colour and a bold modifier.
/// Modified/unsaved tabs show a dot indicator (\u{25CF}).
/// A trailing [+] button indicates that a new tab can be created.
///
/// ┌─ GET /users ─┬─ POST /login \u{25CF} ─┬─ [+] ─┐
pub struct TabBar<'a> {
    app: &'a App,
}

impl<'a> TabBar<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = &self.app.theme;

        if area.height == 0 || area.width < 6 {
            return;
        }

        // Background
        let bg_style = Style::default()
            .fg(theme.colors.foreground)
            .bg(theme.colors.background);
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_style(bg_style);
            }
        }

        // Build tab labels
        let mut segments: Vec<TabSegment> = Vec::with_capacity(self.app.tabs.len() + 1);

        for (i, tab) in self.app.tabs.iter().enumerate() {
            let is_active = i == self.app.active_tab;

            // Build label text
            let title = tab.title();
            let label = if tab.dirty {
                format!(" {title} \u{25CF} ")
            } else {
                format!(" {title} ")
            };

            let style = if is_active {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.colors.muted)
            };

            segments.push(TabSegment {
                label,
                style,
                is_active,
            });
        }

        // [+] new tab button
        segments.push(TabSegment {
            label: " [+] ".to_string(),
            style: Style::default().fg(theme.colors.muted),
            is_active: false,
        });

        // Render each segment left-to-right, separated by a thin divider
        let mut x = area.x;
        let max_x = area.x + area.width;
        let divider = "\u{2502}";

        for (i, seg) in segments.iter().enumerate() {
            // If this is not the first segment, draw a divider
            if i > 0 && x < max_x {
                let div_style = Style::default().fg(theme.colors.muted);
                buf[(x, area.y)].set_symbol(divider);
                buf[(x, area.y)].set_style(div_style);
                x += 1;
            }

            if x >= max_x {
                break;
            }

            // Determine available width for this tab
            let remaining = (max_x - x) as usize;
            let label_width = seg.label.len().min(remaining);
            if label_width == 0 {
                break;
            }

            let display: String = if label_width < seg.label.len() {
                // Truncate with ellipsis
                let trunc = label_width.saturating_sub(2);
                let truncated: String = seg.label.chars().take(trunc).collect();
                format!("{truncated}\u{2026} ")
            } else {
                seg.label.clone()
            };

            // For active tabs, render with an underline accent bar
            let tab_style = if seg.is_active {
                seg.style.add_modifier(Modifier::UNDERLINED)
            } else {
                seg.style
            };

            // Write characters into buffer
            let mut cx = x;
            for ch in display.chars() {
                if cx >= max_x {
                    break;
                }
                buf[(cx, area.y)].set_symbol(&ch.to_string());
                buf[(cx, area.y)].set_style(tab_style);
                cx += 1;
            }

            // If there is a second row, draw the active indicator line
            if area.height > 1 && seg.is_active {
                let indicator_y = area.y + 1;
                for ix in x..cx.min(max_x) {
                    buf[(ix, indicator_y)].set_symbol("\u{2500}");
                    buf[(ix, indicator_y)].set_style(Style::default().fg(theme.colors.accent));
                }
            }

            x = cx;
        }

        // Fill remaining space on the first line with a thin border
        if area.height > 1 {
            let indicator_y = area.y + 1;
            while x < max_x {
                buf[(x, indicator_y)].set_symbol("\u{2500}");
                buf[(x, indicator_y)].set_style(Style::default().fg(theme.colors.muted));
                x += 1;
            }
        }
    }
}

/// Internal representation of a single tab segment for rendering.
struct TabSegment {
    label: String,
    style: Style,
    is_active: bool,
}
