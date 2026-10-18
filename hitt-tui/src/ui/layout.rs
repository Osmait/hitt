use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind, RequestTabKind, ResponseTabKind,
    SidebarSection,
};
use crate::editor::EditTarget;
use crate::ui::theme::Theme;

/// Returns the appropriate border style for a panel based on focus and nav mode.
fn border_style_for(app: &App, focused: bool) -> Style {
    if !focused {
        app.theme.unfocused_border_style()
    } else if app.nav_mode == NavMode::Panel && app.mode == AppMode::Normal {
        app.theme.panel_focused_border_style()
    } else {
        app.theme.focused_border_style()
    }
}

/// Clamp `scroll` so the last page of `content_rows` stays in view, store it
/// back, and return it as a `Paragraph::scroll` offset (saturating to u16).
pub(crate) fn clamp_scroll(scroll: &mut usize, content_rows: usize, visible_rows: u16) -> u16 {
    let max = content_rows.saturating_sub(visible_rows as usize);
    *scroll = (*scroll).min(max);
    u16::try_from(*scroll).unwrap_or(u16::MAX)
}

/// Approximate row count of `lines` rendered with `Wrap` into `width` columns
/// (greedy word wrap, long words broken), used to bound scrolling.
pub(crate) fn wrapped_row_count(lines: &[Line], width: u16) -> usize {
    lines
        .iter()
        .map(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            wrapped_rows(&text, width)
        })
        .sum()
}

fn wrapped_rows(text: &str, width: u16) -> usize {
    let width = usize::from(width.max(1));
    let mut rows = 1;
    let mut col = 0;
    for word in text.split(' ') {
        let len = word.chars().count();
        let needed = if col == 0 { len } else { col + 1 + len };
        if needed <= width {
            col = needed;
            continue;
        }
        if col > 0 {
            rows += 1;
        }
        rows += len.saturating_sub(1) / width;
        col = len % width;
    }
    rows
}

/// Truncate to at most `max` characters (char-boundary safe), adding `…` if cut.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Width of a ratatui `Tabs` row: each title gets 1 column of padding on both
/// sides, and `divider_w` columns between titles.
fn tabs_width(labels: &[&str], divider_w: u16) -> u16 {
    let titles: u16 = labels.iter().map(|l| l.chars().count() as u16 + 2).sum();
    titles + divider_w * labels.len().saturating_sub(1) as u16
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Renders the entire application UI.  Called once per tick from the main loop.
pub fn render(app: &mut App, frame: &mut Frame) {
    let size = frame.area();

    // Clear all clickable regions at the start of each frame.
    app.regions.clear();

    // Top-level vertical split:
    //   [header_bar]  (3 rows)
    //   [body]        (fill)
    //   [status_bar]  (1 row)
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(size);

    render_header_bar(app, frame, root_chunks[0]);
    render_body(app, frame, root_chunks[1]);

    app.regions.status_bar = Some(root_chunks[2]);
    render_status_bar(app, frame, root_chunks[2]);

    // URL preview tooltip (overlaid above status bar when sidebar is focused).
    render_url_preview_tooltip(app, frame, size);

    // Keep the help scroll within its content before drawing the modal.
    if app.mode == AppMode::Modal(ModalKind::Help) {
        clamp_help_scroll(app, size);
    }

    // Modal dialogs (search, help, etc.) are drawn last so they sit on top.
    if let AppMode::Modal(kind) = app.mode.clone() {
        let (modal_area, items) = render_modal(app, frame, &kind, size);
        app.regions.overlay = Some(modal_area);
        app.regions.modal_items = items;
        if matches!(kind, ModalKind::Confirm(_)) {
            let buf = frame.buffer_mut();
            app.regions.confirm_yes = find_text(buf, modal_area, "[Y]es");
            app.regions.confirm_no = find_text(buf, modal_area, "[N]o");
        }
    }

    // Command palette.
    if app.mode == AppMode::Command {
        app.regions.overlay = Some(render_command_palette(app, frame, size));
    }

    // Notification toast last, so errors stay visible above modals (top-right corner).
    if let Some(ref notification) = app.notification {
        render_notification(app, frame, notification, size);
    }
}

/// Rect of the first occurrence of `needle` rendered inside `area`, widened
/// by the two blank columns on each side (button-style click target).
fn find_text(buf: &ratatui::buffer::Buffer, area: Rect, needle: &str) -> Option<Rect> {
    let area = area.intersection(buf.area);
    let width = needle.chars().count() as u16;
    for y in area.y..area.y + area.height {
        let row: String = (area.x..area.x + area.width)
            .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect();
        if let Some(byte_idx) = row.find(needle) {
            let col = row[..byte_idx].chars().count() as u16;
            let x = (area.x + col).saturating_sub(2).max(area.x);
            let right = (area.x + col + width + 2).min(area.x + area.width);
            return Some(Rect::new(x, y, right - x, 1));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Header bar
// ---------------------------------------------------------------------------

fn render_header_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    // Split header into: title | tabs | env selector
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12),
            Constraint::Min(0),
            Constraint::Length(20),
        ])
        .split(area);

    // Tab labels (with Alt+N number hints), truncated char-safely.
    let displays: Vec<String> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let truncated = truncate_chars(&tab.title(), 18);
            // Unsaved edits: the marker sits outside the truncated title.
            let marker = if tab.dirty {
                crate::app::RequestTab::DIRTY_MARKER
            } else {
                ""
            };
            let num = i + 1;
            if num <= 9 {
                format!("{num} {marker}{truncated}")
            } else {
                format!("{marker}{truncated}")
            }
        })
        .collect();

    // Scroll the tab row so the active tab is always visible: pick the window
    // [start, end) of tabs that fits the bar and contains the active tab.
    let tab_area = header_chunks[1];
    let widths: Vec<u16> = displays
        .iter()
        .map(|d| d.chars().count() as u16 + 2)
        .collect();
    let window_w = |from: usize, to: usize| -> u16 {
        widths[from..to].iter().sum::<u16>() + 3 * (to - from).saturating_sub(1) as u16
    };
    let active = app.active_tab.min(app.tabs.len().saturating_sub(1));
    // Start from the previous frame's window so selecting a tab that is
    // already visible doesn't scroll the bar; only scroll when needed.
    let mut start = app.regions.header_tab_start.min(active);
    while start < active && window_w(start, active + 1) > tab_area.width {
        start += 1;
    }
    let mut end = active + 1;
    while end < displays.len() && window_w(start, end + 1) <= tab_area.width {
        end += 1;
    }
    // Fill free space on the right by showing earlier tabs again.
    while start > 0 && window_w(start - 1, end) <= tab_area.width {
        start -= 1;
    }
    app.regions.header_tab_start = start;

    // Record the whole header tab bar plus one region per visible tab label
    // (padding included, divider excluded) on the label row only.
    app.regions.header_tab_bar = Some(tab_area);
    let right = tab_area.x + tab_area.width;
    let mut x = tab_area.x;
    for (i, tab_w) in widths.iter().enumerate().take(end).skip(start) {
        let w = (*tab_w).min(right.saturating_sub(x));
        app.regions
            .header_tabs
            .push((Rect::new(x, tab_area.y, w, 1), i));
        x = x.saturating_add(tab_w + 3);
    }

    let theme = &app.theme;

    // --- App title ---
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " hitt ",
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("v0.1", theme.muted_style()),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_set(theme.border_set())
            .border_style(theme.unfocused_border_style()),
    );
    frame.render_widget(title, header_chunks[0]);

    // --- Request tabs (only the visible window) ---
    let tab_titles: Vec<Line> = displays
        .into_iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .map(|(i, display)| {
            let style = if i == app.active_tab {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.colors.foreground)
            };
            Line::from(Span::styled(display, style))
        })
        .collect();

    let tabs = Tabs::new(tab_titles)
        .select(active - start)
        .highlight_style(
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(Span::styled(" | ", theme.muted_style()))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_set(theme.border_set())
                .border_style(theme.unfocused_border_style()),
        );

    frame.render_widget(tabs, header_chunks[1]);

    // --- Environment selector ---
    let env_label = match app.active_env {
        Some(idx) => app.environments.get(idx).map_or("???", |e| e.name.as_str()),
        None => "No Env",
    };
    let env_style = if app.active_env.is_some() {
        Style::default()
            .fg(theme.colors.success)
            .add_modifier(Modifier::BOLD)
    } else {
        theme.muted_style()
    };
    // Only the right-aligned ` ENV: <name>` text is clickable. Long names
    // are cut with `…` so the trailing space stays.
    let env_area = header_chunks[2];
    let env_label = truncate_chars(env_label, usize::from(env_area.width.saturating_sub(7)));
    let env_text_w = (6 + env_label.chars().count() as u16).min(env_area.width.saturating_sub(1));
    let env_rect = Rect::new(
        env_area.x + env_area.width.saturating_sub(env_text_w + 1),
        env_area.y,
        env_text_w,
        1,
    );
    let env_widget = Paragraph::new(Line::from(vec![
        Span::styled(" ENV: ", theme.muted_style()),
        Span::styled(env_label, env_style),
        Span::raw(" "),
    ]))
    .alignment(Alignment::Right)
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_set(theme.border_set())
            .border_style(theme.unfocused_border_style()),
    );
    frame.render_widget(env_widget, header_chunks[2]);
    app.regions.env_selector = Some(env_rect);
}

// ---------------------------------------------------------------------------
// Body: sidebar + main panel
// ---------------------------------------------------------------------------

fn render_body(app: &mut App, frame: &mut Frame, area: Rect) {
    let sidebar_width = app.theme.sidebar_width;

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_width), Constraint::Min(0)])
        .split(area);

    render_sidebar_panel(app, frame, body_chunks[0]);

    if app.mode == AppMode::ChainEditor {
        // The chain runner replaces the main panel; clicks elsewhere close it.
        app.regions.overlay = Some(body_chunks[1]);
        render_chain_runner(app, frame, body_chunks[1]);
    } else {
        render_main_panel(app, frame, body_chunks[1]);
    }
}

// ---------------------------------------------------------------------------
// Sidebar panel (delegates to widget module)
// ---------------------------------------------------------------------------

fn render_sidebar_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    app.regions.sidebar = Some(area);

    let theme = &app.theme;
    let focused = app.focus == FocusArea::Sidebar;
    let border_style = border_style_for(app, focused);

    let block = Block::default()
        .title(Span::styled(
            " Collections ",
            Style::default()
                .fg(theme.colors.foreground)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Section tabs at the top of the sidebar.
    let sidebar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    render_sidebar_section_tabs(app, frame, sidebar_chunks[0]);

    // Keep the selection in view in every section, then record one region
    // per visible entry. The widget draws from the same clamped offset.
    let content_area = sidebar_chunks[1];
    let rows_per_item = if app.sidebar_state.section == SidebarSection::History {
        crate::ui::widgets::sidebar::HISTORY_ROWS_PER_ENTRY
    } else {
        1
    };
    // While the sidebar is focused the URL preview tooltip covers the two
    // bottom content rows; keep the selection (and click targets) above it.
    let tooltip_rows =
        if app.focus == FocusArea::Sidebar && app.sidebar_state.section != SidebarSection::Chains {
            2
        } else {
            0
        };
    let visible = content_area.height.saturating_sub(tooltip_rows) as usize / rows_per_item;
    let len = sidebar_entry_count(app);
    let state = &mut app.sidebar_state;
    state.selected = state.selected.min(len.saturating_sub(1));
    if state.selected < state.scroll_offset {
        state.scroll_offset = state.selected;
    } else if visible > 0 && state.selected >= state.scroll_offset + visible {
        state.scroll_offset = state.selected + 1 - visible;
    }
    state.scroll_offset = state.scroll_offset.min(len.saturating_sub(visible.max(1)));
    let offset = state.scroll_offset;
    for (row, idx) in (offset..len).take(visible).enumerate() {
        app.regions.sidebar_items.push((
            Rect::new(
                content_area.x,
                content_area.y + (row * rows_per_item) as u16,
                content_area.width,
                rows_per_item as u16,
            ),
            idx,
        ));
    }

    // Delegate actual content rendering to the sidebar widget module.
    crate::ui::widgets::sidebar::render(app, frame, sidebar_chunks[1]);
}

/// Number of entries in the active sidebar section (same as the key handlers).
fn sidebar_entry_count(app: &App) -> usize {
    match app.sidebar_state.section {
        SidebarSection::Collections => crate::event::build_sidebar_items(app).len(),
        SidebarSection::History => app.history.entries().len(),
        SidebarSection::Chains => app.collections.iter().map(|c| c.chains.len()).sum(),
    }
}

/// Resolves the URL and method of the currently selected sidebar item, if it's a request.
fn resolve_sidebar_preview(app: &App) -> Option<(String, String)> {
    if app.sidebar_state.section == SidebarSection::Collections {
        let items = crate::event::build_sidebar_items(app);
        if let Some(crate::event::SidebarItem::Request {
            coll_idx,
            request_id,
        }) = items.get(app.sidebar_state.selected)
        {
            if let Some(coll) = app.collections.get(*coll_idx) {
                if let Some(req) = coll.find_request(request_id) {
                    let label = match &req.protocol {
                        hitt_core::core::request::Protocol::WebSocket => "WS".to_string(),
                        hitt_core::core::request::Protocol::Sse => "SSE".to_string(),
                        hitt_core::core::request::Protocol::Grpc { .. } => "gRPC".to_string(),
                        hitt_core::core::request::Protocol::Http => req.method.as_str().to_string(),
                    };
                    return Some((label, req.url.clone()));
                }
            }
        }
    } else if app.sidebar_state.section == SidebarSection::History {
        let entries = app.history.entries();
        if let Some(entry) = entries.get(app.sidebar_state.selected) {
            return Some((entry.method.as_str().to_string(), entry.url.clone()));
        }
    }
    None
}

/// Renders a floating URL preview tooltip above the status bar when a request is
/// selected in the sidebar.  The tooltip spans enough width for the full URL.
fn render_url_preview_tooltip(app: &App, frame: &mut Frame, screen: Rect) {
    // Only show when sidebar is focused.
    if app.focus != FocusArea::Sidebar {
        return;
    }

    let Some((method, url)) = resolve_sidebar_preview(app) else {
        return;
    };

    let theme = &app.theme;
    let method_enum: hitt_core::core::request::HttpMethod = method
        .parse()
        .unwrap_or(hitt_core::core::request::HttpMethod::GET);

    let content = format!("{method} {url}");
    // Tooltip width: content + 2 padding + 2 border, capped to screen width.
    let tooltip_width = (content.len() as u16 + 4).min(screen.width.saturating_sub(2));
    let tooltip_height = 3_u16;

    // Position: bottom-left, just above the status bar (last row).
    let tooltip_area = Rect::new(
        0,
        screen.height.saturating_sub(tooltip_height + 1),
        tooltip_width,
        tooltip_height,
    );

    frame.render_widget(Clear, tooltip_area);

    let prefix = format!("{method} ");
    let inner_width = (tooltip_width.saturating_sub(4)) as usize;
    let url_space = inner_width.saturating_sub(prefix.len());
    let url_display = truncate_chars(&url, url_space);

    let line = Line::from(vec![
        Span::styled(prefix, theme.method_style(&method_enum)),
        Span::styled(url_display, Style::default().fg(theme.colors.foreground)),
    ]);

    let tooltip = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(theme.border_set())
            .border_style(Style::default().fg(theme.colors.muted))
            .style(Style::default().bg(theme.colors.background)),
    );

    frame.render_widget(tooltip, tooltip_area);
}

fn render_sidebar_section_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    // Use the longest label set that fits so "History" is never cut off.
    let label_sets: [[&str; 3]; 3] = [
        ["Collections", "Chains", "History"],
        ["Coll", "Chains", "History"],
        ["Col", "Chn", "Hist"],
    ];
    let labels = label_sets
        .iter()
        .find(|set| set.iter().map(|l| l.len() as u16).sum::<u16>() + 6 <= area.width)
        .unwrap_or(&label_sets[2]);
    let sections = [
        (SidebarSection::Collections, labels[0]),
        (SidebarSection::Chains, labels[1]),
        (SidebarSection::History, labels[2]),
    ];

    // Record clickable regions for each sidebar section tab first (before immutable borrows).
    let total_len: u16 =
        sections.iter().map(|(_, l)| l.len() as u16).sum::<u16>() + (sections.len() as u16 - 1) * 3; // 3 for " | "
    let start_x = area.x + area.width.saturating_sub(total_len) / 2;
    let mut x = start_x;
    for (section, label) in &sections {
        let w = label.len() as u16;
        app.regions
            .sidebar_section_tabs
            .push((Rect::new(x, area.y, w, 1), *section));
        x += w + 3;
    }

    let theme = &app.theme;
    let spans: Vec<Span> = sections
        .iter()
        .enumerate()
        .flat_map(|(i, (section, label))| {
            let style = if app.sidebar_state.section == *section {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                theme.muted_style()
            };
            let mut parts = vec![Span::styled(*label, style)];
            if i < sections.len() - 1 {
                parts.push(Span::styled(" | ", theme.muted_style()));
            }
            parts
        })
        .collect();

    let tabs_line = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
    frame.render_widget(tabs_line, area);
}

// ---------------------------------------------------------------------------
// Main panel: URL bar + request tabs + response area
// ---------------------------------------------------------------------------

fn render_main_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    use hitt_core::core::request::Protocol;
    // Rows of the URL bar and the two tab selectors.
    const FIXED_ROWS: u16 = 3 + 2 + 2;
    // WebSocket / SSE responses are live lists: keep at least this many rows
    // for them (at 80x24 the WS message list shows ~4 lines plus its input).
    const MIN_STREAM_ROWS: u16 = 9;

    let request_constraint = if matches!(
        app.active_tab().request.protocol,
        Protocol::WebSocket | Protocol::Sse
    ) {
        let cap = area.height.saturating_sub(FIXED_ROWS + MIN_STREAM_ROWS);
        Constraint::Length((area.height * 2 / 5).min(cap.max(3)))
    } else {
        Constraint::Percentage(40)
    };
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // URL bar
            Constraint::Length(2), // Request tab selector
            request_constraint,    // Request body / params area
            Constraint::Length(2), // Response tab selector
            Constraint::Min(0),    // Response body area
        ])
        .split(area);

    render_url_bar(app, frame, main_chunks[0]);
    render_request_tab_bar(app, frame, main_chunks[1]);
    render_request_area(app, frame, main_chunks[2]);
    render_response_tab_bar(app, frame, main_chunks[3]);
    render_response_area(app, frame, main_chunks[4]);
}

// ---------------------------------------------------------------------------
// URL bar
// ---------------------------------------------------------------------------

fn render_url_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    use hitt_core::core::request::Protocol;
    const INSERT_HINT: &str = " [INSERT] ";

    let tab = app.active_tab();
    // Compute method/protocol label.
    let (method_label, method_style_val) = match tab.request.protocol {
        Protocol::WebSocket => (" WS ".to_string(), app.theme.protocol_style_ws()),
        Protocol::Sse => (" SSE ".to_string(), app.theme.protocol_style_sse()),
        _ => (
            format!(" {} ", tab.request.method),
            app.theme.method_style(&tab.request.method),
        ),
    };

    // Record method selector region (first ~8 chars inside the box) and URL bar.
    let method_width = method_label.len() as u16 + 1;
    app.regions.method_selector = Some(Rect::new(area.x + 1, area.y + 1, method_width, 1));
    app.regions.url_bar = Some(area);

    let focused = app.focus == FocusArea::UrlBar;
    let border_style = border_style_for(app, focused);
    let editing = focused && app.mode == AppMode::Insert;
    let url = app.active_tab().request.url.clone();
    let url_chars: Vec<char> = url.chars().collect();
    let prefix_w = method_label.chars().count() + 1;

    // While typing, scroll the URL horizontally so the cursor stays visible.
    let inner_w = usize::from(area.width.saturating_sub(2));
    let (visible_url, cursor_col) = if editing {
        let cursor = app
            .editor
            .url_cursor
            .unwrap_or(url_chars.len())
            .min(url_chars.len());
        // Room for the URL: the cursor needs one cell past the last char.
        let avail = inner_w.saturating_sub(prefix_w + INSERT_HINT.len()).max(1);
        let mut scroll = app.editor.url_scroll.min(url_chars.len());
        if cursor < scroll {
            scroll = cursor;
        } else if cursor >= scroll + avail {
            scroll = cursor + 1 - avail;
        }
        app.editor.url_scroll = scroll;
        let end = (scroll + avail).min(url_chars.len());
        let visible: String = url_chars[scroll..end].iter().collect();
        (visible, Some(prefix_w + cursor - scroll))
    } else {
        (url, None)
    };

    let theme = &app.theme;
    let method_span = Span::styled(method_label, method_style_val);
    let url_span = Span::styled(visible_url, Style::default().fg(theme.colors.foreground));
    let cursor_hint = if editing {
        Span::styled(INSERT_HINT, Style::default().fg(theme.colors.warning))
    } else {
        Span::raw("")
    };

    let url_bar = Paragraph::new(Line::from(vec![
        method_span,
        Span::raw(" "),
        url_span,
        cursor_hint,
    ]))
    .block(
        Block::default()
            .title(Span::styled(
                " URL ",
                Style::default().fg(theme.colors.foreground),
            ))
            .borders(Borders::ALL)
            .border_set(theme.border_set())
            .border_style(border_style),
    );

    frame.render_widget(url_bar, area);
    if let Some(col) = cursor_col.filter(|c| *c < inner_w) {
        let x = area.x + 1 + u16::try_from(col).unwrap_or(u16::MAX);
        frame.set_cursor_position((x, area.y + 1));
    }
}

// ---------------------------------------------------------------------------
// Request tab bar
// ---------------------------------------------------------------------------

fn render_request_tab_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    app.regions.request_tab_bar = Some(area);

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::RequestTabs;

    // Labels with item count badges; short names when the row is too narrow
    // (like the response tab strip).
    // Level 0: full labels, 1: short names, 2: tiny names and tight counts.
    let labels = |level: u8| -> Vec<String> {
        RequestTabKind::all()
            .iter()
            .map(|kind| {
                let name = match (kind, level) {
                    (RequestTabKind::Params, 2) => "Par",
                    (RequestTabKind::Headers, 1) => "Hdrs",
                    (RequestTabKind::Headers, 2) => "Hdr",
                    (RequestTabKind::Assertions, 1 | 2) => "Tests",
                    _ => kind.label(),
                };
                let count = match kind {
                    RequestTabKind::Params => tab.request.params.len(),
                    RequestTabKind::Headers => tab.request.headers.len(),
                    RequestTabKind::Assertions => tab.request.assertions.len(),
                    _ => 0,
                };
                if count > 0 && level == 2 {
                    format!("{name}({count})")
                } else if count > 0 {
                    format!("{name} ({count})")
                } else {
                    name.to_string()
                }
            })
            .collect()
    };
    let fits = |texts: &[String]| {
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        tabs_width(&refs, 3) <= area.width
    };
    let label_texts = (0..2)
        .map(labels)
        .find(|texts| fits(texts))
        .unwrap_or_else(|| labels(2));

    let tab_titles: Vec<Line> = RequestTabKind::all()
        .iter()
        .zip(label_texts)
        .map(|(kind, label)| {
            let style = if tab.request_tab == *kind {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else if focused {
                Style::default().fg(theme.colors.foreground)
            } else {
                theme.muted_style()
            };
            Line::from(Span::styled(label, style))
        })
        .collect();

    let selected = RequestTabKind::all()
        .iter()
        .position(|k| *k == tab.request_tab)
        .unwrap_or(0);

    // `Tabs` packs titles to the left: 1 column of padding on each side and a
    // 3-column divider between titles. One click rect per title (label row).
    let title_widths: Vec<u16> = tab_titles.iter().map(|l| l.width() as u16 + 2).collect();

    let tabs_widget = Tabs::new(tab_titles)
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" | ", theme.muted_style()));

    frame.render_widget(tabs_widget, area);

    let right = area.x + area.width;
    let mut x = area.x;
    for (kind, w) in RequestTabKind::all().iter().zip(title_widths) {
        let w = w.min(right.saturating_sub(x));
        if w > 0 {
            app.regions
                .request_tabs
                .push((Rect::new(x, area.y, w, 1), *kind));
        }
        x = x.saturating_add(w + 3);
    }
}

// ---------------------------------------------------------------------------
// Request area (body / params / headers / auth / assertions)
// ---------------------------------------------------------------------------

fn render_request_area(app: &mut App, frame: &mut Frame, area: Rect) {
    app.regions.request_body = Some(area);

    let theme = &app.theme;
    let focused = app.focus == FocusArea::RequestBody;

    let border_style = border_style_for(app, focused);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Clamped by the text views against the rendered content, then stored back.
    let mut scroll = app.request_scroll;
    let rows = crate::ui::widgets::request_editors::render(app, frame, inner, &mut scroll);
    app.request_scroll = scroll;
    app.regions.request_rows = rows;
}

// ---------------------------------------------------------------------------
// Response tab bar
// ---------------------------------------------------------------------------

/// Renders the inline filter bar at the top of the response body area.
fn render_response_filter_bar(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let query = &app.active_tab().response_filter;
    let is_typing = app.response_filter_active;

    let cursor = if is_typing { "_" } else { "" };
    let label = format!("/{query}{cursor}");

    let style = if is_typing {
        Style::default()
            .fg(theme.colors.foreground)
            .bg(theme.colors.accent)
    } else {
        Style::default()
            .fg(theme.colors.foreground)
            .bg(theme.colors.muted)
    };

    let bar = Paragraph::new(Span::styled(label, style));
    frame.render_widget(bar, area);
}

fn render_response_tab_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let protocol_tabs = ResponseTabKind::for_protocol(&app.active_tab().request.protocol);
    let (mut status, optional) = response_status_spans(app);

    // The status summary is laid out first: pick the widest tab strip that
    // leaves room for it (plus a one-column gap); optional trailing status
    // parts (the size) only count if the full labels fit anyway.
    let full: Vec<&str> = protocol_tabs.iter().map(ResponseTabKind::label).collect();
    let short: Vec<&str> = protocol_tabs
        .iter()
        .map(ResponseTabKind::short_label)
        .collect();
    let levels = [(&full, " | "), (&short, " | "), (&short, "|")];
    let status_w = |spans: &[Span]| spans.iter().map(Span::width).sum::<usize>() as u16;
    let required_w = status_w(&status[..status.len() - optional]);
    let strip_w =
        |(labels, divider): (&Vec<&str>, &str)| tabs_width(labels, divider.chars().count() as u16);
    let (labels, divider) = levels
        .iter()
        .copied()
        .find(|level| strip_w(*level) + 1 + required_w <= area.width)
        .unwrap_or(levels[2]);
    let labels = labels.clone();
    let divider_w = divider.chars().count() as u16;
    let tabs_w = tabs_width(&labels, divider_w).min(area.width);
    let bar_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(tabs_w)])
        .split(area);

    // Clickable regions follow the actual title positions (label row only).
    app.regions.response_tab_bar = Some(bar_chunks[1]);
    let mut x = bar_chunks[1].x;
    let right = bar_chunks[1].x + bar_chunks[1].width;
    for (kind, label) in protocol_tabs.iter().zip(&labels) {
        let title_w = label.chars().count() as u16 + 2;
        let w = title_w.min(right.saturating_sub(x));
        app.regions
            .response_tabs
            .push((Rect::new(x, bar_chunks[1].y, w, 1), *kind));
        x = x.saturating_add(title_w + divider_w);
    }

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::ResponseTabs;

    // Status summary: drop the optional parts, then cut with `…`, so it
    // never runs into the tabs.
    let room = bar_chunks[0].width.saturating_sub(1);
    let keep = status.len() - optional;
    while status.len() > keep && status_w(&status) > room {
        status.pop();
    }
    let status_line = fit_spans(status, usize::from(room));
    let status_area = Rect {
        width: room,
        ..bar_chunks[0]
    };
    frame.render_widget(Paragraph::new(status_line), status_area);

    // Response tabs — protocol-aware.
    let tab_titles: Vec<Line> = protocol_tabs
        .iter()
        .zip(labels)
        .map(|(kind, label)| {
            let style = if tab.response_tab == *kind {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else if focused {
                Style::default().fg(theme.colors.foreground)
            } else {
                theme.muted_style()
            };
            Line::from(Span::styled(label, style))
        })
        .collect();

    let selected = protocol_tabs
        .iter()
        .position(|k| *k == tab.response_tab)
        .unwrap_or(0);

    let tabs_widget = Tabs::new(tab_titles)
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(divider, theme.muted_style()));

    frame.render_widget(tabs_widget, bar_chunks[1]);
}

/// Status summary of the response tab bar (protocol-aware) and how many of
/// its trailing spans are optional (dropped first when space is short).
fn response_status_spans(app: &App) -> (Vec<Span<'static>>, usize) {
    use hitt_core::core::request::Protocol;
    use hitt_core::protocols::sse::SseStatus;
    use hitt_core::protocols::websocket::WsStatus;

    let theme = &app.theme;
    let tab = app.active_tab();
    let fg = Style::default().fg(theme.colors.foreground);
    match tab.request.protocol {
        Protocol::WebSocket => {
            let Some(ref session) = tab.ws_session else {
                return (
                    vec![Span::styled(
                        " WS — Press Enter to connect",
                        theme.muted_style(),
                    )],
                    0,
                );
            };
            let (dot, dot_style) = match &session.status {
                WsStatus::Connected { .. } => ("●", Style::default().fg(theme.colors.success)),
                WsStatus::Connecting | WsStatus::Reconnecting { .. } => {
                    ("◌", Style::default().fg(theme.colors.warning))
                }
                WsStatus::Disconnected => ("○", Style::default().fg(theme.colors.muted)),
                WsStatus::Error(_) => ("✕", Style::default().fg(theme.colors.error)),
            };
            let label = match &session.status {
                WsStatus::Connected { connected_at } => {
                    let dur = chrono::Utc::now() - connected_at;
                    format!(
                        " Connected ({}s) {} msgs",
                        dur.num_seconds(),
                        session.messages.len()
                    )
                }
                WsStatus::Connecting => " Connecting...".to_string(),
                WsStatus::Error(e) => format!(" Error: {e}"),
                _ => " Disconnected".to_string(),
            };
            (
                vec![
                    Span::styled(format!(" {dot} "), dot_style),
                    Span::styled("WS", theme.protocol_style_ws()),
                    Span::styled(label, fg),
                ],
                0,
            )
        }
        Protocol::Sse => {
            let Some(ref session) = tab.sse_session else {
                return (
                    vec![Span::styled(
                        " SSE — Press Enter to connect",
                        theme.muted_style(),
                    )],
                    0,
                );
            };
            let (dot, dot_style) = match &session.status {
                SseStatus::Connected => ("●", Style::default().fg(theme.colors.success)),
                SseStatus::Connecting => ("◌", Style::default().fg(theme.colors.warning)),
                SseStatus::Disconnected => ("○", Style::default().fg(theme.colors.muted)),
                SseStatus::Error(_) => ("✕", Style::default().fg(theme.colors.error)),
            };
            let label = match &session.status {
                SseStatus::Connected => format!(" Connected ({} events)", session.events.len()),
                SseStatus::Connecting => " Connecting...".to_string(),
                SseStatus::Error(e) => format!(" Error: {e}"),
                SseStatus::Disconnected => " Disconnected".to_string(),
            };
            (
                vec![
                    Span::styled(format!(" {dot} "), dot_style),
                    Span::styled("SSE", theme.protocol_style_sse()),
                    Span::styled(label, fg),
                ],
                0,
            )
        }
        _ => {
            if let Some(ref pending) = tab.pending_request {
                (
                    vec![Span::styled(
                        format!(
                            " {} Sending... {:.1}s",
                            spinner_frame(),
                            pending.started_at.elapsed().as_secs_f64()
                        ),
                        Style::default().fg(theme.colors.warning),
                    )],
                    0,
                )
            } else if let Some(ref resp) = tab.response {
                (
                    vec![
                        Span::styled(
                            format!(" {} {}", resp.status, resp.status_text),
                            theme.status_style(resp.status),
                        ),
                        Span::styled(format!("  {}", format_timing_total(&resp.timing)), fg),
                        Span::styled(format!("  {}", resp.size.format()), theme.muted_style()),
                    ],
                    1,
                )
            } else if tab.last_error.is_some() {
                (
                    vec![Span::styled(" ✕ Request failed", theme.error_style())],
                    0,
                )
            } else {
                (
                    vec![Span::styled(" No response yet", theme.muted_style())],
                    0,
                )
            }
        }
    }
}

/// Total request time with the same precision as the phases (0.1 ms), never
/// less than the sum of the phases as displayed.
pub(crate) fn format_timing_total(timing: &hitt_core::core::response::RequestTiming) -> String {
    // Tenths of a millisecond, rounded like `RequestTiming::format_phase`.
    let tenths = |d: std::time::Duration| (d.as_micros() + 50) / 100;
    let phases: u128 = [
        timing.dns_lookup,
        timing.tcp_connect,
        timing.tls_handshake,
        timing.first_byte,
        timing.content_download,
    ]
    .into_iter()
    .flatten()
    .map(tenths)
    .sum();
    let total = tenths(timing.total).max(phases);
    format!("{}.{}ms", total / 10, total % 10)
}

/// Cut `spans` to at most `max` columns, ending with `…` when cut.
fn fit_spans(spans: Vec<Span<'static>>, max: usize) -> Line<'static> {
    let total: usize = spans.iter().map(Span::width).sum();
    if total <= max {
        return Line::from(spans);
    }
    let mut out = Vec::new();
    let mut left = max.saturating_sub(1);
    for span in spans {
        if left == 0 {
            break;
        }
        let w = span.width();
        if w <= left {
            left -= w;
            out.push(span);
            continue;
        }
        let mut cut = String::new();
        for c in span.content.chars() {
            let cw = Span::raw(c.to_string()).width();
            if cw > left {
                break;
            }
            left -= cw;
            cut.push(c);
        }
        out.push(Span::styled(cut, span.style));
        break;
    }
    if max > 0 {
        let style = out.last().map_or_else(Style::default, |s: &Span| s.style);
        out.push(Span::styled("…", style));
    }
    Line::from(out)
}

// ---------------------------------------------------------------------------
// Response area
// ---------------------------------------------------------------------------

fn render_response_area(app: &mut App, frame: &mut Frame, area: Rect) {
    use hitt_core::core::request::Protocol;

    app.regions.response_body = Some(area);

    let protocol = app.active_tab().request.protocol.clone();
    match protocol {
        Protocol::WebSocket => render_ws_response(app, frame, area),
        Protocol::Sse => render_sse_response(app, frame, area),
        _ => render_http_response(app, frame, area),
    }
}

fn render_http_response(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == FocusArea::ResponseBody;
    let border_style = border_style_for(app, focused);
    let theme = app.theme.clone();

    let title = if focused {
        " Response [j/k scroll, / filter] "
    } else {
        " Response "
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(theme.colors.foreground),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ref pending) = app.active_tab().pending_request {
        render_loading_indicator(app, frame, inner, pending.started_at.elapsed());
        return;
    }

    let has_response = app.active_tab().response.is_some();
    if let (false, Some(error)) = (has_response, app.active_tab().last_error.as_deref()) {
        let lines = vec![
            Line::from(Span::styled(
                "✕ Request failed",
                theme.error_style().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                error.to_string(),
                Style::default().fg(theme.colors.foreground),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        return;
    }
    if !has_response {
        let empty = Paragraph::new("Send a request to see the response here.")
            .style(theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    }

    // Filter bar: show when filter is active (typing) or has text (browsing matches).
    let filter_query = app.active_tab().response_filter.clone();
    let show_filter = app.response_filter_active || !filter_query.is_empty();
    let filter_height = u16::from(show_filter);

    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(filter_height), Constraint::Min(0)])
        .split(inner);

    if show_filter {
        render_response_filter_bar(app, frame, content_chunks[0]);
    }

    let content_area = content_chunks[1];
    let response_tab = app.active_tab().response_tab;
    // Clamped against each tab's content below, then stored back.
    let mut scroll = app.response_scroll;

    match response_tab {
        ResponseTabKind::Body => {
            // Build rendered lines first, then compute filter matches on the rendered text.
            let (body, is_json) = {
                let response = app.active_tab().response.as_ref().unwrap();
                let body_str = response.body_text().unwrap_or("[binary data]").to_string();
                let ct = response
                    .header_value("content-type")
                    .unwrap_or("")
                    .to_string();
                let is_json = ct.contains("json")
                    || body_str.trim_start().starts_with('{')
                    || body_str.trim_start().starts_with('[');
                (body_str, is_json)
            };

            let mut lines: Vec<Line> = if is_json {
                crate::utils::pretty_print::highlight_json(
                    &body,
                    &theme.colors.syntax,
                    theme.colors.foreground,
                )
            } else {
                crate::utils::pretty_print::pretty_xml(&body)
                    .lines()
                    .map(|l| {
                        Line::from(Span::styled(
                            l.to_string(),
                            Style::default().fg(theme.colors.foreground),
                        ))
                    })
                    .collect()
            };

            // Compute filter matches on the actual rendered lines.
            let filter_lower = filter_query.to_lowercase();
            let has_filter = !filter_lower.is_empty();

            let match_lines: Vec<usize> = if has_filter {
                lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| {
                        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                        text.to_lowercase().contains(&filter_lower)
                    })
                    .map(|(i, _)| i)
                    .collect()
            } else {
                Vec::new()
            };

            // Clamp match index and auto-scroll to current match.
            if has_filter && !match_lines.is_empty() {
                let match_idx = app
                    .active_tab()
                    .response_filter_match_idx
                    .min(match_lines.len().saturating_sub(1));
                app.active_tab_mut().response_filter_match_idx = match_idx;
                let target_line = match_lines[match_idx];
                let visible_height = content_area.height as usize;
                if target_line < app.response_scroll
                    || target_line >= app.response_scroll + visible_height
                {
                    app.response_scroll = target_line.saturating_sub(visible_height / 4);
                }
            }
            scroll = app.response_scroll;

            // Highlight matching lines with a background tint.
            if has_filter {
                let current_match = if match_lines.is_empty() {
                    None
                } else {
                    Some(match_lines[app.active_tab().response_filter_match_idx])
                };
                for &line_idx in &match_lines {
                    if let Some(line) = lines.get_mut(line_idx) {
                        let bg = if Some(line_idx) == current_match {
                            theme.colors.accent
                        } else {
                            theme.colors.muted
                        };
                        let spans: Vec<Span> = line
                            .spans
                            .iter()
                            .map(|s| Span::styled(s.content.clone(), s.style.bg(bg)))
                            .collect();
                        *line = Line::from(spans);
                    }
                }
            }

            let offset = clamp_scroll(&mut scroll, lines.len(), content_area.height);
            let paragraph = Paragraph::new(lines).scroll((offset, 0));
            frame.render_widget(paragraph, content_area);
        }
        ResponseTabKind::Headers => {
            let response = app.active_tab().response.as_ref().unwrap();
            let lines: Vec<Line> = response
                .headers
                .iter()
                .map(|kv| {
                    Line::from(vec![
                        Span::styled(
                            &kv.key,
                            Style::default()
                                .fg(theme.colors.accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(": ", theme.muted_style()),
                        Span::styled(&kv.value, Style::default().fg(theme.colors.foreground)),
                    ])
                })
                .collect();
            if lines.is_empty() {
                let p = Paragraph::new("No headers").style(theme.muted_style());
                frame.render_widget(p, content_area);
            } else {
                let offset = clamp_scroll(&mut scroll, lines.len(), content_area.height);
                let p = Paragraph::new(lines).scroll((offset, 0));
                frame.render_widget(p, content_area);
            }
        }
        ResponseTabKind::Cookies => {
            let response = app.active_tab().response.as_ref().unwrap();
            if response.cookies.is_empty() {
                let p = Paragraph::new("No cookies").style(theme.muted_style());
                frame.render_widget(p, content_area);
            } else {
                let lines: Vec<Line> = response
                    .cookies
                    .iter()
                    .map(|c| {
                        Line::from(vec![
                            Span::styled(
                                &c.name,
                                Style::default()
                                    .fg(theme.colors.accent)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" = ", theme.muted_style()),
                            Span::styled(&c.value, Style::default().fg(theme.colors.foreground)),
                        ])
                    })
                    .collect();
                let offset = clamp_scroll(&mut scroll, lines.len(), content_area.height);
                frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), content_area);
            }
        }
        ResponseTabKind::Timing => {
            use hitt_core::core::response::RequestTiming;
            let response = app.active_tab().response.as_ref().unwrap();
            let timing = &response.timing;
            let lines = vec![
                Line::from(vec![
                    Span::styled("DNS Lookup:       ", theme.muted_style()),
                    Span::styled(
                        RequestTiming::format_phase(timing.dns_lookup),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("TCP Connect:      ", theme.muted_style()),
                    Span::styled(
                        RequestTiming::format_phase(timing.tcp_connect),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("TLS Handshake:    ", theme.muted_style()),
                    Span::styled(
                        RequestTiming::format_phase(timing.tls_handshake),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("First Byte:       ", theme.muted_style()),
                    Span::styled(
                        RequestTiming::format_phase(timing.first_byte),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Content Download: ", theme.muted_style()),
                    Span::styled(
                        RequestTiming::format_phase(timing.content_download),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "Total:            ",
                        Style::default()
                            .fg(theme.colors.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format_timing_total(timing),
                        Style::default()
                            .fg(theme.colors.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];
            let offset = clamp_scroll(&mut scroll, lines.len(), content_area.height);
            frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), content_area);
        }
        ResponseTabKind::Assertions => {
            let response = app.active_tab().response.as_ref().unwrap();
            if response.assertion_results.is_empty() {
                let p = Paragraph::new("No assertion results").style(theme.muted_style());
                frame.render_widget(p, content_area);
            } else {
                let lines: Vec<Line> = response
                    .assertion_results
                    .iter()
                    .map(|r| {
                        let (icon, style) = if r.passed {
                            ("PASS", theme.success_style())
                        } else {
                            ("FAIL", theme.error_style())
                        };
                        Line::from(vec![
                            Span::styled(format!(" [{icon}] "), style),
                            Span::styled(&r.message, Style::default().fg(theme.colors.foreground)),
                        ])
                    })
                    .collect();
                let offset = clamp_scroll(&mut scroll, lines.len(), content_area.height);
                frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), content_area);
            }
        }
        // WS/SSE tabs handled by their own renderers; should not reach here.
        _ => {}
    }
    app.response_scroll = scroll;
}

// ---------------------------------------------------------------------------
// WebSocket response area
// ---------------------------------------------------------------------------

fn render_ws_response(app: &mut App, frame: &mut Frame, area: Rect) {
    use hitt_core::protocols::websocket::{MessageDirection, WsContent};

    // Clamped list scroll to store back once the borrows below end.
    let mut scroll_update = None;
    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::ResponseBody;

    match tab.response_tab {
        ResponseTabKind::WsMessages => {
            // Split into message list + input bar. On short terminals (e.g. 80x24)
            // drop the side/bottom borders so the message list stays visible.
            let compact = area.height < 10;
            let borders = if compact { Borders::TOP } else { Borders::ALL };
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(3)])
                .split(area);
            // The input keeps a bottom border so its text never sits on the
            // panel's bottom edge (lined up with the sidebar's border).
            let input_borders = if compact {
                Borders::TOP | Borders::BOTTOM
            } else {
                Borders::ALL
            };

            // ── Message list ──
            let msg_border = border_style_for(app, focused);

            let msg_block = Block::default()
                .title(Span::styled(
                    " Messages ",
                    Style::default()
                        .fg(theme.colors.foreground)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(borders)
                .border_set(theme.border_set())
                .border_style(msg_border);

            let msg_inner = msg_block.inner(chunks[0]);
            frame.render_widget(msg_block, chunks[0]);

            if let Some(ref session) = tab.ws_session {
                let visible_height = msg_inner.height as usize;
                let total = session.messages.len();
                // `ws_message_scroll` is the first visible row (follows the bottom
                // while auto-follow is on); the clamped value is stored back below.
                let mut scroll = tab.ws_message_scroll;
                let start = scroll.view_start(total, visible_height);

                let lines: Vec<Line> = session
                    .messages
                    .iter()
                    .skip(start)
                    .take(visible_height)
                    .map(|msg| {
                        let time = msg.timestamp.format("%H:%M:%S").to_string();
                        let (arrow, arrow_style) = match msg.direction {
                            MessageDirection::Sent => {
                                ("→", Style::default().fg(theme.colors.accent))
                            }
                            MessageDirection::Received => {
                                ("←", Style::default().fg(theme.colors.success))
                            }
                        };
                        let content = match &msg.content {
                            WsContent::Text(t) => truncate_chars(t, 200),
                            WsContent::Binary(b) => format!("[Binary: {} bytes]", b.len()),
                        };
                        Line::from(vec![
                            Span::styled(format!("[{time}] "), theme.muted_style()),
                            Span::styled(format!("{arrow} "), arrow_style),
                            Span::styled(content, Style::default().fg(theme.colors.foreground)),
                        ])
                    })
                    .collect();

                frame.render_widget(Paragraph::new(lines), msg_inner);
                scroll_update = Some(scroll);
            } else {
                let empty = Paragraph::new("Press Enter to connect")
                    .style(theme.muted_style())
                    .alignment(Alignment::Center);
                frame.render_widget(empty, msg_inner);
            }

            // Register WS input bar region for mouse click handling.
            app.regions.ws_input_bar = Some(chunks[1]);

            // ── Input bar ──
            let in_insert = app.mode == AppMode::Insert && focused;
            let input_border = border_style_for(app, in_insert);

            let mut title_spans = vec![Span::styled(
                " Send ",
                Style::default()
                    .fg(theme.colors.foreground)
                    .add_modifier(Modifier::BOLD),
            )];
            if in_insert {
                title_spans.push(Span::styled(
                    "[INSERT] ",
                    Style::default()
                        .fg(theme.colors.success)
                        .add_modifier(Modifier::DIM),
                ));
            }

            let input_block = Block::default()
                .title(Line::from(title_spans))
                .borders(input_borders)
                .border_set(theme.border_set())
                .border_style(input_border);

            let input_inner = input_block.inner(chunks[1]);
            frame.render_widget(input_block, chunks[1]);

            let tab = app.active_tab();
            let input_text = if tab.ws_message_input.is_empty() && !in_insert {
                Span::styled("Press 'i' to type a message...", theme.muted_style())
            } else {
                Span::styled(
                    &tab.ws_message_input,
                    Style::default().fg(theme.colors.foreground),
                )
            };

            frame.render_widget(
                Paragraph::new(Line::from(vec![input_text])).wrap(Wrap { trim: false }),
                input_inner,
            );
        }
        ResponseTabKind::WsInfo => {
            let border_style = border_style_for(app, focused);
            let block = Block::default()
                .title(Span::styled(
                    " Connection Info ",
                    Style::default().fg(theme.colors.foreground),
                ))
                .borders(Borders::ALL)
                .border_set(theme.border_set())
                .border_style(border_style);
            let inner = block.inner(area);
            frame.render_widget(block, area);

            if let Some(ref session) = tab.ws_session {
                use hitt_core::protocols::websocket::WsStatus;
                let status_str = match &session.status {
                    WsStatus::Connected { connected_at } => {
                        let dur = chrono::Utc::now() - connected_at;
                        format!("Connected ({}s)", dur.num_seconds())
                    }
                    WsStatus::Connecting => "Connecting...".to_string(),
                    WsStatus::Reconnecting { attempt, .. } => {
                        format!("Reconnecting (attempt {attempt})")
                    }
                    WsStatus::Disconnected => "Disconnected".to_string(),
                    WsStatus::Error(e) => format!("Error: {e}"),
                };
                let lines = vec![
                    Line::from(vec![
                        Span::styled("URL:      ", theme.muted_style()),
                        Span::styled(&session.url, Style::default().fg(theme.colors.foreground)),
                    ]),
                    Line::from(vec![
                        Span::styled("Status:   ", theme.muted_style()),
                        Span::styled(status_str, Style::default().fg(theme.colors.foreground)),
                    ]),
                    Line::from(vec![
                        Span::styled("Messages: ", theme.muted_style()),
                        Span::styled(
                            format!("{}", session.messages.len()),
                            Style::default().fg(theme.colors.foreground),
                        ),
                    ]),
                ];
                frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            } else {
                let p = Paragraph::new("No WebSocket session")
                    .style(theme.muted_style())
                    .alignment(Alignment::Center);
                frame.render_widget(p, inner);
            }
        }
        _ => {} // fallback — should not happen for WS protocol
    }
    if let Some(scroll) = scroll_update {
        app.active_tab_mut().ws_message_scroll = scroll;
    }
}

// ---------------------------------------------------------------------------
// SSE response area
// ---------------------------------------------------------------------------

fn render_sse_response(app: &mut App, frame: &mut Frame, area: Rect) {
    // Clamped list scroll (and stream row count) to store back once the
    // borrows below end.
    let mut scroll_update = None;
    let mut stream_rows_update = None;
    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::ResponseBody;

    let border_style = border_style_for(app, focused);

    match tab.response_tab {
        ResponseTabKind::SseEvents => {
            let block = Block::default()
                .title(Span::styled(
                    " Events ",
                    Style::default()
                        .fg(theme.colors.foreground)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_set(theme.border_set())
                .border_style(border_style);
            let mut inner = block.inner(area);
            frame.render_widget(block, area);

            // The status line truncates errors; show the full message above the events.
            if let Some(hitt_core::protocols::sse::SseStatus::Error(e)) =
                tab.sse_session.as_ref().map(|s| &s.status)
            {
                let msg = format!("Error: {e}");
                let rows = u16::try_from(wrapped_rows(&msg, inner.width))
                    .unwrap_or(u16::MAX)
                    .min(inner.height.saturating_sub(1).max(1));
                let err_area = Rect {
                    height: rows,
                    ..inner
                };
                frame.render_widget(
                    Paragraph::new(msg)
                        .style(Style::default().fg(theme.colors.error))
                        .wrap(Wrap { trim: true }),
                    err_area,
                );
                inner.y += rows;
                inner.height = inner.height.saturating_sub(rows);
            }

            if let Some(ref session) = tab.sse_session {
                let visible_height = inner.height as usize;
                let total = session.events.len();
                let mut scroll = tab.sse_event_scroll;
                let start = scroll.view_start(total, visible_height);

                let lines: Vec<Line> = session
                    .events
                    .iter()
                    .skip(start)
                    .take(visible_height)
                    .map(|evt| {
                        let time = evt.timestamp.format("%H:%M:%S").to_string();
                        let event_type = evt.event_type.as_deref().unwrap_or("message");
                        let data_preview = truncate_chars(&evt.data, 100);

                        let mut spans = vec![
                            Span::styled(format!("[{time}] "), theme.muted_style()),
                            Span::styled(
                                format!("[{event_type}] "),
                                Style::default().fg(theme.colors.accent),
                            ),
                            Span::styled(
                                data_preview,
                                Style::default().fg(theme.colors.foreground),
                            ),
                        ];

                        if let Some(ref id) = evt.id {
                            spans.push(Span::styled(format!(" (id:{id})"), theme.muted_style()));
                        }

                        Line::from(spans)
                    })
                    .collect();

                frame.render_widget(Paragraph::new(lines), inner);
                scroll_update = Some(scroll);
            } else {
                let p = Paragraph::new("Press Enter to connect")
                    .style(theme.muted_style())
                    .alignment(Alignment::Center);
                frame.render_widget(p, inner);
            }
        }
        ResponseTabKind::SseStream => {
            let block = Block::default()
                .title(Span::styled(
                    " Accumulated Text ",
                    Style::default()
                        .fg(theme.colors.foreground)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_set(theme.border_set())
                .border_style(border_style);
            let inner = block.inner(area);
            frame.render_widget(block, area);

            if let Some(ref session) = tab.sse_session {
                // `sse_event_scroll` is shared with the events list: here its
                // offset counts wrapped rows (and follows the end of the text).
                // Only the visible rows are built, so a large live stream
                // doesn't stall every frame.
                let mut scroll = tab.sse_event_scroll;
                let (rows, total) = stream_view_rows(
                    &session.accumulated_text,
                    inner.width,
                    inner.height,
                    &mut scroll,
                );
                let lines: Vec<Line> = rows.into_iter().map(Line::raw).collect();
                frame.render_widget(
                    Paragraph::new(lines).style(Style::default().fg(theme.colors.foreground)),
                    inner,
                );
                scroll_update = Some(scroll);
                stream_rows_update = Some(total);
            } else {
                let p = Paragraph::new("No SSE session")
                    .style(theme.muted_style())
                    .alignment(Alignment::Center);
                frame.render_widget(p, inner);
            }
        }
        ResponseTabKind::SseInfo => {
            let block = Block::default()
                .title(Span::styled(
                    " Connection Info ",
                    Style::default().fg(theme.colors.foreground),
                ))
                .borders(Borders::ALL)
                .border_set(theme.border_set())
                .border_style(border_style);
            let inner = block.inner(area);
            frame.render_widget(block, area);

            if let Some(ref session) = tab.sse_session {
                use hitt_core::protocols::sse::SseStatus;
                let status_str = match &session.status {
                    SseStatus::Connected => "Connected".to_string(),
                    SseStatus::Connecting => "Connecting...".to_string(),
                    SseStatus::Disconnected => "Disconnected".to_string(),
                    SseStatus::Error(e) => format!("Error: {e}"),
                };
                let lines = vec![
                    Line::from(vec![
                        Span::styled("URL:    ", theme.muted_style()),
                        Span::styled(&session.url, Style::default().fg(theme.colors.foreground)),
                    ]),
                    Line::from(vec![
                        Span::styled("Status: ", theme.muted_style()),
                        Span::styled(status_str, Style::default().fg(theme.colors.foreground)),
                    ]),
                    Line::from(vec![
                        Span::styled("Events: ", theme.muted_style()),
                        Span::styled(
                            format!("{}", session.events.len()),
                            Style::default().fg(theme.colors.foreground),
                        ),
                    ]),
                ];
                frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            } else {
                let p = Paragraph::new("No SSE session")
                    .style(theme.muted_style())
                    .alignment(Alignment::Center);
                frame.render_widget(p, inner);
            }
        }
        _ => {} // fallback — should not happen for SSE protocol
    }
    if let Some(scroll) = scroll_update {
        app.active_tab_mut().sse_event_scroll = scroll;
    }
    if let Some(rows) = stream_rows_update {
        app.active_tab_mut().sse_stream_rows = rows;
    }
}

/// Display width of `text` (ASCII fast path).
fn text_width(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        Span::raw(text).width()
    }
}

/// Byte ranges of the rows of `line` hard-wrapped at `width` columns
/// (at least one row, possibly empty).
fn hard_wrap(line: &str, width: usize) -> Vec<std::ops::Range<usize>> {
    let width = width.max(1);
    if line.is_ascii() {
        if line.is_empty() {
            return std::iter::once(0..0).collect();
        }
        return (0..line.len())
            .step_by(width)
            .map(|start| start..(start + width).min(line.len()))
            .collect();
    }
    if is_narrow(line) {
        // One column per char: cut every `width` chars.
        let mut starts: Vec<usize> = line.char_indices().step_by(width).map(|(i, _)| i).collect();
        starts.push(line.len());
        return starts.windows(2).map(|w| w[0]..w[1]).collect();
    }
    let mut rows = Vec::new();
    let (mut start, mut col) = (0, 0);
    for (idx, c) in line.char_indices() {
        let cw = text_width(&line[idx..idx + c.len_utf8()]);
        if col + cw > width && col > 0 {
            rows.push(start..idx);
            start = idx;
            col = 0;
        }
        col += cw;
    }
    rows.push(start..line.len());
    rows
}

/// True if every char of `line` is one column wide (e.g. accented Latin
/// text), so rows can be cut by char count.
fn is_narrow(line: &str) -> bool {
    Span::raw(line).width() == line.chars().count()
}

/// Number of rows `line` takes hard-wrapped at `width` columns.
fn hard_wrap_rows(line: &str, width: usize) -> usize {
    let width = width.max(1);
    if line.is_ascii() {
        line.len().div_ceil(width).max(1)
    } else if is_narrow(line) {
        line.chars().count().div_ceil(width).max(1)
    } else {
        hard_wrap(line, width).len()
    }
}

/// The rows of `text` (hard-wrapped at `width`) visible in a `height`-row
/// view scrolled by `scroll`, plus the total row count. Only the visible
/// lines are split into rows; the others are measured, which is cheap for
/// ASCII text, so a large accumulated SSE stream renders quickly.
pub(crate) fn stream_view_rows<'a>(
    text: &'a str,
    width: u16,
    height: u16,
    scroll: &mut crate::app::ListScroll,
) -> (Vec<&'a str>, usize) {
    let width = usize::from(width.max(1));
    let height = usize::from(height);
    let line_rows: Vec<(&str, usize)> = text
        .lines()
        .map(|line| (line, hard_wrap_rows(line, width)))
        .collect();
    let total: usize = line_rows.iter().map(|(_, rows)| rows).sum();
    let start = scroll.view_start(total, height);

    let mut out = Vec::with_capacity(height);
    let mut row = 0;
    for (line, rows) in line_rows {
        if out.len() >= height {
            break;
        }
        if row + rows <= start {
            row += rows;
            continue;
        }
        let skip = start.saturating_sub(row);
        out.extend(
            hard_wrap(line, width)
                .into_iter()
                .skip(skip)
                .take(height - out.len())
                .map(|range| &line[range]),
        );
        row += rows;
    }
    (out, total)
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

fn render_status_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let mode_span = match &app.mode {
        AppMode::Normal => match app.nav_mode {
            crate::app::NavMode::Global => Span::styled(
                " GLOBAL ",
                Style::default()
                    .fg(theme.colors.background)
                    .bg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            crate::app::NavMode::Panel => Span::styled(
                " PANEL ",
                Style::default()
                    .fg(theme.colors.background)
                    .bg(theme.colors.warning)
                    .add_modifier(Modifier::BOLD),
            ),
        },
        AppMode::Insert => Span::styled(
            " INSERT ",
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.success)
                .add_modifier(Modifier::BOLD),
        ),
        AppMode::Command => Span::styled(
            " COMMAND ",
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        AppMode::Modal(_) => Span::styled(
            " MODAL ",
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        AppMode::ChainEditor => Span::styled(
            " CHAIN ",
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        AppMode::ProxyInspector => Span::styled(
            " PROXY ",
            Style::default()
                .fg(theme.colors.background)
                .bg(theme.colors.error)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let focus_label = match app.focus {
        FocusArea::Sidebar => "Sidebar",
        FocusArea::UrlBar => "URL",
        FocusArea::RequestTabs => "Request Tabs",
        FocusArea::RequestBody => "Request Body",
        FocusArea::ResponseBody => "Response Body",
        FocusArea::ResponseTabs => "Response Tabs",
        FocusArea::ChainSteps => "Chain Steps",
        FocusArea::ProxyList => "Proxy List",
    };

    // The mode badge and focus label always fit; the key hints shrink (by
    // dropping trailing hints) and the tab / collection counts go first.
    let width = usize::from(area.width);
    let mode_w = mode_span.content.chars().count();
    let focus_text = format!(" {focus_label} ");
    let essential = mode_w + focus_text.chars().count();
    let help_hint = fit_hint(&status_hint(app), width.saturating_sub(essential));
    let counts_text = format!(
        "  {} tab{}  {} collection{} ",
        app.tabs.len(),
        if app.tabs.len() == 1 { "" } else { "s" },
        app.collections.len(),
        if app.collections.len() == 1 { "" } else { "s" }
    );
    let room = width.saturating_sub(essential + help_hint.chars().count());

    let mut spans = vec![
        mode_span,
        Span::styled(focus_text, Style::default().fg(theme.colors.foreground)),
    ];
    if counts_text.chars().count() <= room {
        spans.push(Span::styled(counts_text, theme.muted_style()));
    }
    let status_line = Line::from(spans);

    // We render the left portion, then overlay the help hint on the right.
    let hint_w = u16::try_from(help_hint.chars().count()).unwrap_or(u16::MAX);
    let bar_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(hint_w)])
        .split(area);

    let left = Paragraph::new(status_line).style(Style::default().bg(theme.colors.background));
    frame.render_widget(left, bar_chunks[0]);

    let right = Paragraph::new(help_hint)
        .style(theme.muted_style())
        .alignment(Alignment::Right);
    frame.render_widget(right, bar_chunks[1]);
}

/// Shorten `hint` (segments separated by two spaces) to at most `max`
/// columns by dropping trailing segments, then by truncating.
fn fit_hint(hint: &str, max: usize) -> String {
    if hint.chars().count() <= max {
        return hint.to_string();
    }
    let segments: Vec<&str> = hint.trim().split("  ").collect();
    for n in (1..segments.len()).rev() {
        let candidate = format!(" {} ", segments[..n].join("  "));
        if candidate.chars().count() <= max {
            return candidate;
        }
    }
    if max == 0 {
        String::new()
    } else {
        truncate_chars(hint, max)
    }
}

/// Key hints for the status bar: depend on the mode first (insert, command,
/// each modal, runners), then on the focused panel.
pub(crate) fn status_hint(app: &App) -> String {
    use hitt_core::core::request::Protocol;

    let hint: &str = match &app.mode {
        AppMode::Insert => {
            if app.editor.body.is_some() {
                " Editing body  arrows:move  Enter:newline  Tab:indent  Esc:done "
            } else if app.editor.request_form().is_some() {
                " Tab/Up/Down:field  Left/Right:option  Enter:save  Esc:cancel "
            } else if app.focus == FocusArea::ResponseBody
                && app.active_tab().request.protocol == Protocol::WebSocket
            {
                " Typing message  Enter:send  Tab:next  Esc:done "
            } else if app.focus == FocusArea::UrlBar {
                " Editing URL  Enter:send  ^R:send+stay  ←/→ ^A/^E:move  ^U:clear  Esc:done "
            } else {
                " Typing  Esc:done "
            }
        }
        AppMode::Command => " Enter:run  Up/Down:history  Esc:cancel ",
        AppMode::Modal(kind) => match kind {
            ModalKind::Search => " Type to search  Up/Down ^N/^P:select  Enter:open  Esc:close ",
            ModalKind::Help => " j/k:scroll  J/K:page  g/G:top/end  Esc/q/?:close ",
            ModalKind::Confirm(_) => " y/Enter:yes  n/Esc:no ",
            ModalKind::Import => " Type a path  Enter:import  Esc:cancel ",
            ModalKind::Export => " Type a path  Enter:export  Esc:cancel ",
            ModalKind::DiffSelector if app.diff_state.view.is_some() => {
                " j/k:scroll  u:unified/split  h:back  Esc:close "
            }
            ModalKind::DiffSelector => " j/k:move  Space:mark  Enter:compare  Esc:close ",
            ModalKind::RenameTab
            | ModalKind::RenameCollection(_)
            | ModalKind::RenameRequest { .. } => " Type a name  Enter:save  Esc:cancel ",
            ModalKind::CollectionPicker => " j/k:select  Enter:save here  Esc:cancel ",
            ModalKind::ThemePicker => " j/k:preview  Enter:apply  Esc:cancel ",
            ModalKind::EnvironmentEdit => match app.editor.env_form().map(|f| f.target) {
                Some(EditTarget::NewEnvironment | EditTarget::RenameEnvironment) => {
                    " Type a name  Enter:save  Esc:cancel "
                }
                Some(_) => " Tab:key/value  Enter:save  Esc:cancel ",
                None => " a/i/x:add/edit/delete  Space:toggle  Esc:close ",
            },
            ModalKind::Runner
                if app
                    .runner
                    .run
                    .as_ref()
                    .is_some_and(|r| r.export_input.is_some()) =>
            {
                " Type a path  Enter:save  Esc:cancel "
            }
            ModalKind::Runner if app.runner.run.as_ref().is_some_and(|r| r.running) => {
                " j/k:select  Esc:cancel run "
            }
            ModalKind::Runner => " j/k:select  Enter:open  x:export  Esc:close ",
        },
        AppMode::ChainEditor if app.active_chain.as_ref().is_some_and(|c| c.running) => {
            " j/k:scroll  Esc:stop chain "
        }
        AppMode::ChainEditor => " j/k:scroll  Esc/q:close ",
        AppMode::ProxyInspector => " Esc:close ",
        AppMode::Normal if app.active_tab().pending_request.is_some() => {
            " Sending...  Esc:cancel request  ?:help "
        }
        AppMode::Normal if app.response_filter_active => {
            " Type to filter  Enter:keep  Esc:clear "
        }
        AppMode::Normal if app.nav_mode == NavMode::Global => {
            " hjkl:nav  Enter:focus  Tab:cycle  q:quit  ?:help "
        }
        AppMode::Normal => match app.focus {
            FocusArea::Sidebar => match app.sidebar_state.section {
                SidebarSection::History => {
                    " Esc:global  j/k:nav  Enter:open  r/^R:replay  [/]:section  ?:help "
                }
                SidebarSection::Chains => {
                    " Esc:global  j/k:nav  Enter:run  x:delete  [/]:section  ?:help "
                }
                SidebarSection::Collections => {
                    " Esc:global  j/k:nav  l/h:open/close  a:add  r:rename  x:delete  R:run  [/]:section "
                }
            },
            FocusArea::UrlBar => " Esc:global  i:edit  m:method/protocol  Enter:send  Tab:next ",
            FocusArea::RequestTabs | FocusArea::ResponseTabs => {
                " Esc:global  h/l:switch tab  1-5:jump  Tab:next "
            }
            FocusArea::ResponseBody => match app.active_tab().request.protocol {
                // `q` only disconnects a live (or connecting) session.
                Protocol::WebSocket if stream_session_live(app) => {
                    " Esc:global  i:input  j/k:scroll  q:disconnect "
                }
                Protocol::WebSocket => " Esc:global  i:input  j/k:scroll ",
                Protocol::Sse if stream_session_live(app) => {
                    " Esc:global  j/k:scroll  a:toggle view  q:disconnect "
                }
                Protocol::Sse => " Esc:global  j/k:scroll  a:toggle view ",
                _ if !app.active_tab().response_filter.is_empty() => {
                    " n/N:next/prev match  Esc:clear filter  j/k:scroll "
                }
                _ => " Esc:global  j/k:scroll  J/K:page  g/G:top/end  /:filter  1-5:tab ",
            },
            FocusArea::RequestBody => request_body_hint(app),
            _ => " Esc:global  ?:help  Tab:focus ",
        },
    };
    hint.to_string()
}

/// True if the active tab's WebSocket / SSE session is connected or connecting.
fn stream_session_live(app: &App) -> bool {
    use hitt_core::protocols::sse::SseStatus;
    use hitt_core::protocols::websocket::WsStatus;
    let tab = app.active_tab();
    // Same states as the `q` handler (`event::actions::ws_is_active`).
    let ws = tab
        .ws_session
        .as_ref()
        .is_some_and(|s| matches!(s.status, WsStatus::Connected { .. } | WsStatus::Connecting));
    let sse = tab
        .sse_session
        .as_ref()
        .is_some_and(|s| matches!(s.status, SseStatus::Connected | SseStatus::Connecting));
    ws || sse
}

/// Status-bar hint for the request area, by sub-tab and body/auth type.
fn request_body_hint(app: &App) -> &'static str {
    use hitt_core::core::request::RequestBody;
    let request = &app.active_tab().request;
    match app.active_tab().request_tab {
        RequestTabKind::Params | RequestTabKind::Headers | RequestTabKind::Assertions => {
            " j/k:select  a:add  i/Enter:edit  x/d:delete  Space:toggle  1-5:tab "
        }
        RequestTabKind::Auth => " t:auth type  j/k:field  i/Enter:edit  v:show secrets  1-5:tab ",
        RequestTabKind::Body => match &request.body {
            Some(RequestBody::Json(_)) => " t:type  i/Enter:edit  f:format  j/k:scroll  1-5:tab ",
            Some(RequestBody::Raw { .. }) => {
                " t:type  i/Enter:edit  c:content-type  j/k:scroll  1-5:tab "
            }
            Some(RequestBody::GraphQL { .. }) => " t:type  i:query  I:variables  j/k:scroll ",
            Some(RequestBody::FormUrlEncoded(_) | RequestBody::FormData(_)) => {
                " t:type  a:add  i/Enter:edit  x/d:delete  Space:toggle "
            }
            _ => " Esc:global  t:body type  1-5:tab ",
        },
    }
}

// ---------------------------------------------------------------------------
// Notification toast
// ---------------------------------------------------------------------------

fn render_notification(
    app: &App,
    frame: &mut Frame,
    notification: &crate::app::Notification,
    area: Rect,
) {
    let theme = &app.theme;

    let (border_color, icon) = match notification.kind {
        NotificationKind::Info => (theme.colors.accent, "INFO"),
        NotificationKind::Success => (theme.colors.success, "OK"),
        NotificationKind::Warning => (theme.colors.warning, "WARN"),
        NotificationKind::Error => (theme.colors.error, "ERR"),
    };

    let msg = format!(" [{}] {} ", icon, notification.message);
    let msg_w = u16::try_from(msg.chars().count()).unwrap_or(u16::MAX);
    let toast_width = msg_w.saturating_add(4).min(area.width.saturating_sub(2));
    // Short terminals: a bare line on the top row instead of a bordered box.
    let bordered = area.height >= 4;
    let (toast_y, toast_height) = if bordered { (1, 3) } else { (0, 1) };

    // Position in top-right corner, never outside the frame.
    let toast_area = Rect::new(
        area.x + area.width.saturating_sub(toast_width + 1),
        area.y + toast_y,
        toast_width,
        toast_height,
    )
    .intersection(area);

    frame.render_widget(Clear, toast_area);

    if !bordered {
        let line = Paragraph::new(msg).style(
            Style::default()
                .fg(border_color)
                .bg(theme.colors.background),
        );
        frame.render_widget(line, toast_area);
        return;
    }

    let toast = Paragraph::new(msg).alignment(Alignment::Left).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(theme.border_set())
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(theme.colors.background)),
    );

    frame.render_widget(toast, toast_area);
}

// ---------------------------------------------------------------------------
// Loading indicator
// ---------------------------------------------------------------------------

/// Current frame of the request spinner (advances ~4 times per second).
pub(crate) fn spinner_frame() -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        / 250) as usize
        % FRAMES.len();
    FRAMES[idx]
}

/// Spinner shown in the response area while the tab's request is in flight.
fn render_loading_indicator(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    elapsed: std::time::Duration,
) {
    let theme = &app.theme;
    let lines = vec![
        Line::from(Span::styled(
            format!(
                "{} Sending... {:.1}s",
                spinner_frame(),
                elapsed.as_secs_f64()
            ),
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled("Esc to cancel", theme.muted_style())),
    ];
    // Vertically centre the two lines.
    let y = area.y + area.height.saturating_sub(2) / 2;
    let centred = Rect::new(area.x, y, area.width, area.height.min(2));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), centred);
}

// ---------------------------------------------------------------------------
// Modal overlay (search, help, confirm, etc.)
// ---------------------------------------------------------------------------

/// Draws the modal and returns its area plus the clickable rows of its list
/// (`(rect, item index)`; empty for modals without a list).
fn render_modal(
    app: &App,
    frame: &mut Frame,
    kind: &ModalKind,
    area: Rect,
) -> (Rect, Vec<(Rect, usize)>) {
    let modal_area = modal_rect(app, kind, area);
    frame.render_widget(Clear, modal_area);

    let items = match kind {
        ModalKind::Help => {
            render_help_modal(app, frame, modal_area);
            Vec::new()
        }
        ModalKind::Search => render_search_modal(app, frame, modal_area),
        ModalKind::RenameTab => {
            render_rename_modal(app, frame, modal_area);
            Vec::new()
        }
        ModalKind::CollectionPicker => render_collection_picker_modal(app, frame, modal_area),
        ModalKind::ThemePicker => render_theme_picker_modal(app, frame, modal_area),
        ModalKind::RenameCollection(_) => {
            render_rename_collection_modal(app, frame, modal_area);
            Vec::new()
        }
        ModalKind::RenameRequest { .. } => {
            render_rename_request_modal(app, frame, modal_area);
            Vec::new()
        }
        ModalKind::Confirm(msg) => {
            render_confirm_modal(app, frame, modal_area, msg);
            Vec::new()
        }
        ModalKind::Import => {
            render_import_modal(app, frame, modal_area);
            Vec::new()
        }
        ModalKind::Export => {
            render_export_modal(app, frame, modal_area);
            Vec::new()
        }
        ModalKind::DiffSelector => crate::ui::widgets::diff_viewer::render(app, frame, modal_area),
        ModalKind::EnvironmentEdit => {
            crate::ui::widgets::request_editors::render_env_editor_modal(app, frame, modal_area)
        }
        ModalKind::Runner => crate::ui::widgets::runner::render(app, frame, modal_area),
    };
    (modal_area, items)
}

/// One rect per list item (single-line items drawn from the top of `area`).
fn list_rows(area: Rect, len: usize) -> Vec<(Rect, usize)> {
    (0..len)
        .take(usize::from(area.height))
        .map(|i| (Rect::new(area.x, area.y + i as u16, area.width, 1), i))
        .collect()
}

/// Clamp `help_scroll` to the help text so `G` / `J` can't scroll past the end.
fn clamp_help_scroll(app: &mut App, screen: Rect) {
    // Same size as the Help entry in `render_modal`, minus the borders.
    let modal = centered_rect(80, 90, screen);
    let rows = wrapped_help_lines(&app.theme, modal.width.saturating_sub(2)).len();
    clamp_scroll(&mut app.help_scroll, rows, modal.height.saturating_sub(2));
}

/// Help text wrapped to `width` columns by hand (rows keep their key column,
/// descriptions continue under it), so the scroll bound counts exactly the
/// rows that are drawn.
fn wrapped_help_lines(theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    help_lines(theme)
        .into_iter()
        .flat_map(|line| match line.spans.as_slice() {
            [key, desc] if key.width() < width => {
                let key_w = key.width();
                let pad = Span::raw(" ".repeat(key_w));
                wrap_words(&desc.content, width - key_w)
                    .into_iter()
                    .enumerate()
                    .map(|(i, chunk)| {
                        let lead = if i == 0 { key.clone() } else { pad.clone() };
                        Line::from(vec![lead, Span::styled(chunk, desc.style)])
                    })
                    .collect::<Vec<_>>()
            }
            spans => {
                let style = spans.first().map_or_else(Style::default, |s| s.style);
                let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
                wrap_words(&text, width)
                    .into_iter()
                    .map(|chunk| Line::from(Span::styled(chunk, style)))
                    .collect()
            }
        })
        .collect()
}

/// Greedy word wrap of `text` into rows of at most `width` columns; words
/// longer than a row are split. Always returns at least one row.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0;
    // `fresh`: nothing placed on the current row yet (no separator needed).
    let mut fresh = true;
    for word in text.split(' ') {
        let w = text_width(word);
        let sep = usize::from(!fresh);
        if cur_w + sep + w <= width {
            if !fresh {
                cur.push(' ');
            }
            cur.push_str(word);
            cur_w += sep + w;
            fresh = false;
            continue;
        }
        if word.is_empty() {
            // Extra spaces at a row break are dropped.
            continue;
        }
        if !fresh {
            rows.push(std::mem::take(&mut cur));
        }
        // Split a word wider than the row; its last piece starts the new row.
        let mut pieces = hard_wrap(word, width).into_iter().peekable();
        while let Some(range) = pieces.next() {
            if pieces.peek().is_some() {
                rows.push(word[range].to_string());
            } else {
                cur = word[range].to_string();
                cur_w = text_width(&cur);
            }
        }
        fresh = false;
    }
    rows.push(cur);
    rows
}

fn help_lines(theme: &Theme) -> Vec<Line<'static>> {
    let key_style = Style::default().fg(theme.colors.accent);
    let desc_style = Style::default().fg(theme.colors.foreground);
    let heading_style = Style::default()
        .fg(theme.colors.warning)
        .add_modifier(Modifier::BOLD);
    let heading = |text: &'static str| Line::from(Span::styled(text, heading_style));
    let row = |key: &'static str, desc: &'static str| {
        Line::from(vec![
            Span::styled(format!("  {key:<18}"), key_style),
            Span::styled(desc, desc_style),
        ])
    };

    let themes: Vec<Line<'static>> = crate::ui::theme::AVAILABLE_THEMES
        .iter()
        .map(|&name| {
            row(
                name,
                if name == "catppuccin" {
                    "(default)"
                } else {
                    ""
                },
            )
        })
        .collect();

    let mut lines = vec![
        // ── Navigation ──────────────────────────────────────────
        heading("Navigation"),
        row("Enter", "Global mode: work inside the focused panel"),
        row("Esc / q", "Panel mode: back to Global mode"),
        row("Tab", "Cycle focus forward"),
        row("Shift+Tab", "Cycle focus backward"),
        row("j / Down", "Navigate down / scroll"),
        row("k / Up", "Navigate up / scroll"),
        row("h / Left", "Collapse / prev sub-tab"),
        row("l / Right", "Expand / next sub-tab"),
        row("Alt+h/j/k/l", "Move between panels (also in Panel mode)"),
        row("1-5", "Jump to sub-tab by number"),
        row("J / Ctrl+D", "Page down (15 lines)"),
        row("K / Ctrl+U", "Page up (15 lines)"),
        row("g", "Scroll / select top"),
        row("G (Shift+g)", "Scroll / select bottom"),
        Line::from(""),
        // ── Requests ────────────────────────────────────────────
        heading("Requests"),
        row("Enter", "Send request / sidebar action"),
        row("Ctrl+R", "Send request"),
        row("Esc", "Cancel in-flight request"),
        row("i", "Edit focused input (URL, editors)"),
        row("m", "Cycle method/protocol"),
        row("y", "Copy response to clipboard"),
        row("Ctrl+S / s", "Save to collection"),
        row("●", "Tab has unsaved changes"),
        Line::from(""),
        // ── URL editing ─────────────────────────────────────────
        heading("URL bar (insert mode, i)"),
        row("Left / Right", "Move cursor"),
        row("Home/End ^A/^E", "Start / end of URL"),
        row("Backspace / Del", "Delete before / at cursor"),
        row("Ctrl+U", "Delete to start of URL"),
        row("Paste", "Inserts at the cursor"),
        row("Enter", "Send (leaves insert mode)"),
        row("Ctrl+R", "Send, stay in insert mode"),
        row("Esc / Tab", "Done / next panel"),
        Line::from(""),
        // ── Response body ───────────────────────────────────────
        heading("Response body"),
        row("/", "Filter response (per tab)"),
        row("n / N", "Next / previous match"),
        row("Esc", "Clear the filter"),
        Line::from(""),
        // ── Request editors ─────────────────────────────────────
        heading("Request editors (Panel focus on request area)"),
        row("j / k", "Select row / field"),
        row("a", "Add row (params, headers, form, tests)"),
        row("i / Enter", "Edit selected row / field / body"),
        row("x / d", "Delete selected row"),
        row("Space", "Toggle row enabled"),
        row("t", "Body / Auth: cycle type"),
        row("", "Auth: Inherit, None, Bearer, Basic, API Key, OAuth2"),
        row("f", "JSON body: format"),
        row("c", "Raw body: edit content type"),
        row("I", "GraphQL body: edit variables"),
        row("v", "Auth: show / hide secrets"),
        row("Tab / Shift+Tab", "Form: next / previous field"),
        row("Left / Right", "Form: change option"),
        row("Enter / Esc", "Form: save / cancel"),
        row("Esc", "Body editor: done (keeps text)"),
        Line::from(""),
        // ── Environment editor ──────────────────────────────────
        heading("Variables editor (E / :envedit)"),
        row("a / i / x", "Add / edit / delete variable"),
        row("Space", "Toggle enabled"),
        row("s", "Mark / unmark secret"),
        row("v", "Reveal secrets"),
        row("e / n", "Next environment / new environment"),
        row("R / D", "Rename / delete environment"),
        row("c", "Switch env / collection variables"),
        Line::from(""),
        // ── Tabs ────────────────────────────────────────────────
        heading("Tabs"),
        row("t / Ctrl+N", "New tab"),
        row("w", "Close tab (last tab: reset to blank)"),
        row("n", "Next tab"),
        row("b", "Previous tab"),
        row("Alt+1-9", "Jump to tab by number"),
        row("F2", "Rename tab"),
        Line::from(""),
        // ── Sidebar ─────────────────────────────────────────────
        heading("Sidebar"),
        row("Enter", "Open request / toggle folder"),
        row("l / Right", "Expand / open request"),
        row("h / Left", "Collapse / go to parent"),
        row("g / G", "First / last item"),
        row("[ / ]", "Prev / next section"),
        row("i", "Import file"),
        row("a", "Add request to collection"),
        row("x", "Delete request / chain (asks first)"),
        row("r", "Rename collection/request"),
        row("R", "Run collection / folder (runner)"),
        row("Enter (Chains)", "Run chain (Esc stops / closes)"),
        row("Enter (History)", "Reopen request + stored response"),
        row("r / ^R (History)", "Replay: reopen and send now"),
        Line::from(""),
        // ── Modes ───────────────────────────────────────────────
        heading("Modes"),
        row(":", "Command mode (Up/Down: history)"),
        row("/  p  Ctrl+P", "Search requests (fuzzy)"),
        row("e / Ctrl+E", "Cycle environment (then No Env)"),
        row("E", "Edit environment variables"),
        row("Ctrl+O", "Import modal"),
        row("Ctrl+X", "Export modal"),
        row("d", "Diff selector"),
        row("?", "Show / close this help"),
        row("Esc", "Back to normal mode"),
        row("q", "Quit (asks if unsaved)"),
        row("Ctrl+C", "Force quit"),
        Line::from(""),
        // ── Real-time Protocols ─────────────────────────────────
        heading("Real-time Protocols (WS / SSE)"),
        row("m", "Cycle to WS/SSE protocol"),
        row("Enter", "Connect / disconnect toggle"),
        row("i", "WS: type message (Insert)"),
        row("j / k", "Scroll messages / events"),
        row("a", "SSE: toggle accumulated view"),
        row("q", "Disconnect (on response, if connected)"),
        Line::from(""),
        // ── Commands ────────────────────────────────────────────
        heading("Commands (: prefix)"),
        row(":q / :quit", "Quit application"),
        row(":q!", "Quit, discard unsaved"),
        row(":help", "Show this help"),
        row(":rename <name>", "Rename current request"),
        row(":theme [name]", "Switch theme (no name: picker)"),
        row(":proxy", "Proxy inspector (not available yet)"),
        Line::from(""),
        heading("Collections"),
        row(":newcol <name>", "Create collection"),
        row(":delcol [name]", "Delete collection (asks first)"),
        row(":save", "Save request to collection"),
        row(":delreq", "Delete selected request (asks first)"),
        row(":addvar <k> <v>", "Add / update collection variable"),
        row(":colvars", "Edit collection variables"),
        Line::from(""),
        heading("Environments"),
        row(":env <name|none>", "Set active environment"),
        row(":newenv <name>", "Create environment"),
        row(":dupenv [name]", "Duplicate environment"),
        row(":renameenv <name>", "Rename active environment"),
        row(":delenv [name]", "Delete environment (asks first)"),
        row(":env-file <path>", "Load .env file (session-only)"),
        row(":envedit", "Edit environment variables"),
        Line::from(""),
        heading("Import / Export"),
        row(":import <path>", "Import (.json/.har/.yaml/.env/cURL)"),
        row(":export <path>", "Export (.json/.md/.sh/.env)"),
        row(":curl", "Copy as cURL to clipboard"),
        row(":paste-curl", "Import cURL from clipboard"),
        row(":docs", "Copy markdown docs"),
        Line::from(""),
        heading("WebSocket / SSE"),
        row(":ws <url>", "Connect WebSocket"),
        row(":ws-disconnect", "Disconnect WebSocket"),
        row(":sse <url>", "Connect SSE stream"),
        row(":sse-disconnect", "Disconnect SSE stream"),
        Line::from(""),
        heading("Chains"),
        row(":newchain <name>", "Create chain (becomes active)"),
        row(":addstep <req>", "Add step to active chain"),
        row("  --chain <name>", "...or to a named chain"),
        row(":chain <name>", "Run chain"),
        row(":importchain <p>", "Import chain YAML"),
        Line::from(""),
        heading("Testing"),
        row(":loadtest <n> <c>", "Load test (n reqs, c concur.)"),
        row(":diff", "Open diff selector"),
        row(":run [col] [fold]", "Run collection (--bail stops early)"),
        row(":runexport <path>", "Export run report (.json/.xml/.txt)"),
        Line::from(""),
        heading("Collection runner"),
        row("j / k", "Select request"),
        row("Enter", "Open request with its response"),
        row("x", "Export report (.json/.xml/.txt)"),
        row("Esc / q", "Cancel run / close"),
        Line::from(""),
        heading("Settings  (:set <key> <value>, saved to config)"),
        row("timeout", "Request timeout in ms"),
        row("follow_redirects", "true / false"),
        row("verify_ssl", "true / false"),
        row("history_limit", "Max history entries"),
        row("persist_history", "Save history across sessions"),
        row("theme", "Theme name"),
        Line::from(""),
        heading("Other"),
        row(":clearhistory", "Clear request history (and file)"),
        row(":replay [n]", "Replay history entry (1 = newest)"),
        Line::from(""),
        heading("Themes"),
    ];
    lines.extend(themes);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " j/k scroll  J/K page  g/G top/bottom  PgUp/PgDn  Esc/q/? close",
        theme.muted_style(),
    )));
    lines
}

fn render_help_modal(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let help_text = wrapped_help_lines(theme, area.width.saturating_sub(2));
    // Already clamped by `clamp_help_scroll`.
    let scroll = u16::try_from(app.help_scroll).unwrap_or(u16::MAX);

    let block = Block::default()
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(theme.focused_border_style())
        .style(Style::default().bg(theme.colors.background));

    // Pre-wrapped by `wrapped_help_lines`: no `Wrap`, so the row count matches.
    let paragraph = Paragraph::new(help_text).block(block).scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

fn render_search_modal(app: &App, frame: &mut Frame, area: Rect) -> Vec<(Rect, usize)> {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Search Requests ",
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

    let search_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    // Search input.
    let input = Paragraph::new(Line::from(vec![
        Span::styled(" > ", Style::default().fg(theme.colors.accent)),
        Span::styled(
            &app.search_query,
            Style::default().fg(theme.colors.foreground),
        ),
        Span::styled("_", Style::default().fg(theme.colors.accent)),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_set(theme.border_set())
            .border_style(theme.unfocused_border_style()),
    );
    frame.render_widget(input, search_chunks[0]);

    // Results list.
    if app.search_results.is_empty() {
        let hint = if app.search_query.is_empty() {
            "Type to search collections..."
        } else {
            "No matching requests found."
        };
        let p = Paragraph::new(hint)
            .style(theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(p, search_chunks[1]);
        Vec::new()
    } else {
        // Scroll only when the selected result would leave the view, so
        // clicking a visible row doesn't move the list.
        let visible = usize::from(search_chunks[1].height).max(1);
        let selected = app.search_selected.min(app.search_results.len() - 1);
        let offset = crate::ui::widgets::request_editors::sticky_offset(
            &app.search_scroll,
            selected,
            visible,
            app.search_results.len(),
        );
        let items: Vec<ratatui::widgets::ListItem> = app
            .search_results
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible)
            .map(|(i, r)| {
                let is_selected = i == selected;
                let pick = |style: Style| {
                    if is_selected {
                        theme.selected_style()
                    } else {
                        style
                    }
                };
                let method_style = r
                    .method
                    .as_ref()
                    .map_or(theme.muted_style(), |m| theme.method_style(m));
                let method_label = r
                    .method
                    .as_ref()
                    .map_or("???", hitt_core::core::request::HttpMethod::as_str);
                let marker = if is_selected { "> " } else { "  " };
                let line = Line::from(vec![
                    Span::styled(marker, pick(theme.accent_style())),
                    Span::styled(format!("{method_label:<7} "), pick(method_style)),
                    Span::styled(&r.name, pick(Style::default().fg(theme.colors.foreground))),
                    Span::styled(
                        r.collection_name
                            .as_deref()
                            .map(|c| format!("  ({c})"))
                            .unwrap_or_default(),
                        pick(theme.muted_style()),
                    ),
                ]);
                let item = ratatui::widgets::ListItem::new(line);
                if is_selected {
                    item.style(theme.selected_style())
                } else {
                    item
                }
            })
            .collect();

        let list = ratatui::widgets::List::new(items);
        frame.render_widget(list, search_chunks[1]);
        // Click targets map rows back to result indexes (offset by the scroll).
        list_rows(search_chunks[1], app.search_results.len() - offset)
            .into_iter()
            .map(|(rect, i)| (rect, i + offset))
            .collect()
    }
}

fn render_confirm_modal(app: &App, frame: &mut Frame, area: Rect, message: &str) {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Confirm ",
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(Style::default().fg(theme.colors.warning))
        .style(Style::default().bg(theme.colors.background));

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default().fg(theme.colors.foreground),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [Y]es  ",
                Style::default()
                    .fg(theme.colors.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  [N]o  ",
                Style::default()
                    .fg(theme.colors.error)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_rename_modal(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Rename Tab ",
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

    let input_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled("  Enter new name:", theme.muted_style()));
    frame.render_widget(label, input_chunks[0]);

    let input_text = format!("  {}█", app.rename_input);
    let input = Paragraph::new(Span::styled(
        input_text,
        Style::default().fg(theme.colors.foreground),
    ));
    frame.render_widget(input, input_chunks[1]);

    let hint = Paragraph::new(Span::styled(
        "  Enter:confirm  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, input_chunks[2]);
}

fn render_collection_picker_modal(app: &App, frame: &mut Frame, area: Rect) -> Vec<(Rect, usize)> {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Save to Collection ",
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

    if app.collections.is_empty() {
        let empty = Paragraph::new("No collections. Use :newcol <name> to create one.")
            .style(theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return Vec::new();
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled("  Select a collection:", theme.muted_style()));
    frame.render_widget(label, chunks[0]);

    let items: Vec<ratatui::widgets::ListItem> = app
        .collections
        .iter()
        .enumerate()
        .map(|(i, coll)| {
            let marker = if i == app.collection_picker_selected {
                "> "
            } else {
                "  "
            };
            let style = if i == app.collection_picker_selected {
                theme.selected_style()
            } else {
                Style::default().fg(theme.colors.foreground)
            };
            ratatui::widgets::ListItem::new(Line::from(Span::styled(
                format!(
                    "{}{}  ({} requests)",
                    marker,
                    coll.name,
                    coll.request_count()
                ),
                style,
            )))
        })
        .collect();

    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, chunks[1]);

    let hint = Paragraph::new(Span::styled(
        "  j/k:move  Enter:select  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, chunks[2]);
    list_rows(chunks[1], app.collections.len())
}

fn render_theme_picker_modal(app: &App, frame: &mut Frame, area: Rect) -> Vec<(Rect, usize)> {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Select Theme ",
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
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled("  Choose a theme:", theme.muted_style()));
    frame.render_widget(label, chunks[0]);

    let config_theme = app.config.theme.as_str();
    let items: Vec<ratatui::widgets::ListItem> = crate::ui::theme::AVAILABLE_THEMES
        .iter()
        .enumerate()
        .map(|(i, &name)| {
            let marker = if i == app.theme_picker_selected {
                "> "
            } else {
                "  "
            };
            let current = if name == config_theme {
                " (current)"
            } else {
                ""
            };
            let style = if i == app.theme_picker_selected {
                theme.selected_style()
            } else {
                Style::default().fg(theme.colors.foreground)
            };
            ratatui::widgets::ListItem::new(Line::from(Span::styled(
                format!("{marker}{name}{current}"),
                style,
            )))
        })
        .collect();

    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, chunks[1]);

    let hint = Paragraph::new(Span::styled(
        "  j/k:move  Enter:select  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, chunks[2]);
    list_rows(chunks[1], crate::ui::theme::AVAILABLE_THEMES.len())
}

fn render_rename_collection_modal(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Rename Collection ",
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

    let input_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled("  Enter new name:", theme.muted_style()));
    frame.render_widget(label, input_chunks[0]);

    let input_text = format!("  {}█", app.rename_input);
    let input = Paragraph::new(Span::styled(
        input_text,
        Style::default().fg(theme.colors.foreground),
    ));
    frame.render_widget(input, input_chunks[1]);

    let hint = Paragraph::new(Span::styled(
        "  Enter:confirm  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, input_chunks[2]);
}

fn render_rename_request_modal(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Rename Request ",
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

    let input_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled("  Enter new name:", theme.muted_style()));
    frame.render_widget(label, input_chunks[0]);

    let input_text = format!("  {}█", app.rename_input);
    let input = Paragraph::new(Span::styled(
        input_text,
        Style::default().fg(theme.colors.foreground),
    ));
    frame.render_widget(input, input_chunks[1]);

    let hint = Paragraph::new(Span::styled(
        "  Enter:confirm  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, input_chunks[2]);
}

fn render_import_modal(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Import ",
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
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            // Formats list: wraps instead of being cut off in narrow modals.
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled("  Enter file path:", theme.muted_style()));
    frame.render_widget(label, chunks[0]);

    // Long paths scroll horizontally: keep the end (and the cursor) visible.
    let input_room = usize::from(chunks[1].width).saturating_sub(3);
    let input_text = format!("  {}\u{2588}", tail_fit(&app.modal_input, input_room));
    let input = Paragraph::new(Span::styled(
        input_text,
        Style::default().fg(theme.colors.foreground),
    ));
    frame.render_widget(input, chunks[1]);

    let hint = Paragraph::new(Span::styled(
        "  Enter:import  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, chunks[2]);

    let formats = Paragraph::new(Line::from(vec![
        Span::styled("  Formats: ", theme.muted_style()),
        Span::styled(".json", Style::default().fg(theme.colors.accent)),
        Span::styled(" Postman  ", theme.muted_style()),
        Span::styled(".har", Style::default().fg(theme.colors.accent)),
        Span::styled(" HAR  ", theme.muted_style()),
        Span::styled(".yaml", Style::default().fg(theme.colors.accent)),
        Span::styled(" OpenAPI  ", theme.muted_style()),
        Span::styled(".env", Style::default().fg(theme.colors.accent)),
        Span::styled(" dotenv  ", theme.muted_style()),
        Span::styled(".curl/.sh/.txt", Style::default().fg(theme.colors.accent)),
        Span::styled(" cURL", theme.muted_style()),
    ]))
    .wrap(Wrap { trim: false });
    frame.render_widget(formats, chunks[3]);
}

fn render_export_modal(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let block = Block::default()
        .title(Span::styled(
            " Export ",
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
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            // Formats list: wraps instead of being cut off in narrow modals.
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled(
        "  Enter output file path:",
        theme.muted_style(),
    ));
    frame.render_widget(label, chunks[0]);

    // Long paths scroll horizontally: keep the end (and the cursor) visible.
    let input_room = usize::from(chunks[1].width).saturating_sub(3);
    let input_text = format!("  {}\u{2588}", tail_fit(&app.modal_input, input_room));
    let input = Paragraph::new(Span::styled(
        input_text,
        Style::default().fg(theme.colors.foreground),
    ));
    frame.render_widget(input, chunks[1]);

    let hint = Paragraph::new(Span::styled(
        "  Enter:export  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, chunks[2]);

    let formats = Paragraph::new(Line::from(vec![
        Span::styled("  Formats: ", theme.muted_style()),
        Span::styled(".json", Style::default().fg(theme.colors.accent)),
        Span::styled(" Postman  ", theme.muted_style()),
        Span::styled(".md", Style::default().fg(theme.colors.accent)),
        Span::styled(" Markdown  ", theme.muted_style()),
        Span::styled(".sh", Style::default().fg(theme.colors.accent)),
        Span::styled(" cURL  ", theme.muted_style()),
        Span::styled(".env", Style::default().fg(theme.colors.accent)),
        Span::styled(" Environment", theme.muted_style()),
    ]))
    .wrap(Wrap { trim: false });
    frame.render_widget(formats, chunks[3]);
}

// ---------------------------------------------------------------------------
// Command palette
// ---------------------------------------------------------------------------

/// Returns the palette area.
fn render_command_palette(app: &App, frame: &mut Frame, area: Rect) -> Rect {
    let theme = &app.theme;

    let palette_width = pct_at_least(area.width, 60, 40).min(80);
    // Too short for the bordered box: draw a bare `:` line instead.
    let bordered = area.height >= 3;
    let palette_height = if bordered { 3 } else { area.height.min(1) };
    let x = area.x + area.width.saturating_sub(palette_width) / 2;
    let y = area.y + (area.height / 4).min(area.height.saturating_sub(palette_height));
    let palette_area = Rect::new(x, y, palette_width, palette_height).intersection(area);

    frame.render_widget(Clear, palette_area);

    // Keep the end of long commands (and the cursor) in view.
    let text_w = usize::from(palette_width.saturating_sub(if bordered { 2 } else { 0 }));
    let command = tail_fit(&app.command_input, text_w.saturating_sub(4));
    let input = Paragraph::new(Line::from(vec![
        Span::styled(
            " : ",
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(command, Style::default().fg(theme.colors.foreground)),
        Span::styled("_", Style::default().fg(theme.colors.accent)),
    ]));
    if !bordered {
        frame.render_widget(
            input.style(Style::default().bg(theme.colors.background)),
            palette_area,
        );
        return palette_area;
    }
    let input = input.block(
        Block::default()
            .title(Span::styled(
                " Command ",
                Style::default()
                    .fg(theme.colors.warning)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_set(theme.border_set())
            .border_style(Style::default().fg(theme.colors.warning))
            .style(Style::default().bg(theme.colors.background)),
    );

    frame.render_widget(input, palette_area);
    palette_area
}

/// The last `max` characters of `text`, prefixed with `…` when cut, so the
/// end of a long input (where the cursor is) stays visible.
fn tail_fit(text: &str, max: usize) -> String {
    let len = text.chars().count();
    if len <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let tail: String = text.chars().skip(len - (max - 1)).collect();
    format!("…{tail}")
}

// ---------------------------------------------------------------------------
// Chain runner
// ---------------------------------------------------------------------------

fn render_chain_runner(app: &App, frame: &mut Frame, area: Rect) {
    use hitt_core::core::chain::ChainStepStatus;

    let theme = &app.theme;

    let chain_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // step list
            Constraint::Length(6), // extracted variables
        ])
        .split(area);

    // ── Header ──
    let chain_name = app
        .active_chain_def
        .as_ref()
        .map_or("Chain", |c| c.name.as_str());

    let failed = app
        .active_chain
        .as_ref()
        .map_or(0, crate::event::chain_failed_steps);
    let status_text = if let Some(ref state) = app.active_chain {
        let total = state.step_statuses.len();
        if state.running {
            format!("Running step {}/{}", state.current_step + 1, total)
        } else if app.chain_cancelled {
            let done = state
                .step_statuses
                .iter()
                .filter(|s| {
                    matches!(
                        s,
                        ChainStepStatus::Success { .. } | ChainStepStatus::Failed { .. }
                    )
                })
                .count();
            format!("Cancelled ({done}/{total} steps run)")
        } else if failed > 0 {
            format!("Failed ({failed}/{total} steps failed)")
        } else {
            "Complete".to_string()
        }
    } else {
        "Idle".to_string()
    };

    let running = app.active_chain.as_ref().is_some_and(|s| s.running);
    let status_style = if running || (app.active_chain.is_some() && app.chain_cancelled) {
        Style::default()
            .fg(theme.colors.warning)
            .add_modifier(Modifier::BOLD)
    } else if failed > 0 {
        Style::default()
            .fg(theme.colors.error)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.colors.success)
            .add_modifier(Modifier::BOLD)
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {chain_name} "),
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" | ", theme.muted_style()),
        Span::styled(status_text, status_style),
    ]))
    .block(
        Block::default()
            .title(Span::styled(
                " Chain Runner ",
                Style::default()
                    .fg(theme.colors.foreground)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_set(theme.border_set())
            .border_style(Style::default().fg(theme.colors.accent)),
    );
    frame.render_widget(header, chain_chunks[0]);

    // ── Step list ──
    let steps_block = Block::default()
        .title(Span::styled(
            " Steps ",
            Style::default().fg(theme.colors.foreground),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(Style::default().fg(theme.colors.muted));
    let inner_steps = steps_block.inner(chain_chunks[1]);
    frame.render_widget(steps_block, chain_chunks[1]);

    if let (Some(ref chain_def), Some(ref state)) = (&app.active_chain_def, &app.active_chain) {
        let visible_height = inner_steps.height as usize;
        let scroll = app
            .chain_scroll
            .min(chain_def.steps.len().saturating_sub(1));

        let step_items: Vec<ListItem> = chain_def
            .steps
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible_height)
            .map(|(i, step)| {
                let status = state
                    .step_statuses
                    .get(i)
                    .unwrap_or(&ChainStepStatus::Pending);

                // Same icons as the collection runner.
                let (icon, icon_style) = match status {
                    ChainStepStatus::Pending => ("·", Style::default().fg(theme.colors.muted)),
                    ChainStepStatus::Running => (
                        spinner_frame(),
                        Style::default()
                            .fg(theme.colors.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    ChainStepStatus::Success { .. } => {
                        ("✓", Style::default().fg(theme.colors.success))
                    }
                    ChainStepStatus::Failed { .. } => {
                        ("✗", Style::default().fg(theme.colors.error))
                    }
                    ChainStepStatus::Skipped { .. } => ("-", theme.muted_style()),
                };

                // Find request name from collection
                let req_name = app
                    .active_chain_coll_idx
                    .and_then(|ci| app.collections.get(ci))
                    .and_then(|c| c.find_request(&step.request_id))
                    .map_or_else(
                        || format!("(request not found: {})", &step.request_id.to_string()[..8]),
                        |r| format!("{} {}", r.method, r.name),
                    );

                let detail = match status {
                    ChainStepStatus::Success {
                        status,
                        duration_ms,
                    } => {
                        format!("  {status} {duration_ms}ms")
                    }
                    ChainStepStatus::Failed { error } => {
                        format!("  {}", truncate_chars(error, 60))
                    }
                    ChainStepStatus::Skipped { reason } => {
                        format!("  {reason}")
                    }
                    _ => String::new(),
                };

                let detail_style = match status {
                    ChainStepStatus::Success { status, .. } => theme.status_style(*status),
                    ChainStepStatus::Failed { .. } => Style::default().fg(theme.colors.error),
                    _ => theme.muted_style(),
                };

                let is_current = i == state.current_step;
                let line = Line::from(vec![
                    Span::styled(format!(" {icon} "), icon_style),
                    Span::styled(
                        req_name,
                        if is_current {
                            Style::default()
                                .fg(theme.colors.foreground)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.colors.foreground)
                        },
                    ),
                    Span::styled(detail, detail_style),
                ]);

                if is_current {
                    ListItem::new(theme.selected_line(line)).style(theme.selected_style())
                } else {
                    ListItem::new(line)
                }
            })
            .collect();

        let list = List::new(step_items);
        frame.render_widget(list, inner_steps);
    }

    // ── Extracted variables ──
    let vars_block = Block::default()
        .title(Span::styled(
            " Variables ",
            Style::default().fg(theme.colors.foreground),
        ))
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(Style::default().fg(theme.colors.muted));
    let inner_vars = vars_block.inner(chain_chunks[2]);
    frame.render_widget(vars_block, chain_chunks[2]);

    if let Some(ref state) = app.active_chain {
        if state.extracted_variables.is_empty() {
            let empty = Paragraph::new("No variables extracted yet")
                .style(theme.muted_style())
                .alignment(Alignment::Center);
            frame.render_widget(empty, inner_vars);
        } else {
            let mut vars: Vec<(&String, &String)> = state.extracted_variables.iter().collect();
            vars.sort_by_key(|(k, _)| k.as_str());

            let var_items: Vec<ListItem> = vars
                .iter()
                .take(inner_vars.height as usize)
                .map(|(name, value)| {
                    let line = Line::from(vec![
                        Span::styled(
                            format!("  {name} "),
                            Style::default()
                                .fg(theme.colors.accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("= ", theme.muted_style()),
                        Span::styled(value.as_str(), Style::default().fg(theme.colors.foreground)),
                    ]);
                    ListItem::new(line)
                })
                .collect();

            let list = List::new(var_items);
            frame.render_widget(list, inner_vars);
        }
    }
}

// ---------------------------------------------------------------------------
// Utility: centred rectangle
// ---------------------------------------------------------------------------

/// Centred `width` x `height` rect, shrunk to fit inside `area`.
fn centered_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// `pct`% of `total`, but at least `min` (and never more than `total`).
fn pct_at_least(total: u16, pct: u16, min: u16) -> u16 {
    let scaled = u16::try_from(u32::from(total) * u32::from(pct) / 100).unwrap_or(total);
    scaled.max(min).min(total)
}

/// Where a modal is drawn. Dialogs with a fixed layout (rename, confirm,
/// import / export, pickers) are sized by their content so their rows never
/// collapse; the rest scale with the screen. Always inside `area`.
fn modal_rect(app: &App, kind: &ModalKind, area: Rect) -> Rect {
    let pct = |w: u16, h: u16| centered_rect(w, h, area);
    match kind {
        ModalKind::Help => pct(80, 90),
        ModalKind::Search => centered_fixed(
            pct_at_least(area.width, 60, 40),
            pct_at_least(area.height, 50, 10),
            area,
        ),
        ModalKind::Confirm(message) => {
            let msg_w = u16::try_from(message.chars().count()).unwrap_or(u16::MAX);
            let width = msg_w.saturating_add(6).clamp(30, 70).min(area.width);
            // Borders + blank, message, blank, buttons.
            let rows = wrapped_rows(message, width.saturating_sub(2));
            let height = u16::try_from(rows).unwrap_or(u16::MAX).saturating_add(5);
            centered_fixed(width, height, area)
        }
        ModalKind::Import | ModalKind::Export => {
            centered_fixed(pct_at_least(area.width, 60, 44), 8, area)
        }
        // The diff view needs much more room than the response picker.
        ModalKind::DiffSelector if app.diff_state.view.is_some() => pct(90, 85),
        ModalKind::DiffSelector => pct(70, 50),
        ModalKind::RenameTab | ModalKind::RenameCollection(_) | ModalKind::RenameRequest { .. } => {
            // Borders + label, input, hint.
            centered_fixed(pct_at_least(area.width, 50, 40).min(70), 5, area)
        }
        ModalKind::CollectionPicker => {
            let rows = u16::try_from(app.collections.len()).unwrap_or(u16::MAX);
            // Borders + label + list + hint.
            centered_fixed(
                pct_at_least(area.width, 50, 44),
                rows.saturating_add(4).max(6),
                area,
            )
        }
        ModalKind::ThemePicker => {
            let rows = u16::try_from(crate::ui::theme::AVAILABLE_THEMES.len()).unwrap_or(10);
            centered_fixed(pct_at_least(area.width, 45, 42), rows + 4, area)
        }
        ModalKind::EnvironmentEdit => pct(75, 65),
        ModalKind::Runner => pct(85, 85),
    }
}

/// Returns a centred `Rect` that occupies `width_pct`% x `height_pct`% of `area`.
fn centered_rect(width_pct: u16, height_pct: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1])[1]
}
