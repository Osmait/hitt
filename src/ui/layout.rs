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

    // Notification toast (overlaid on top-right corner).
    if let Some(ref notification) = app.notification {
        render_notification(app, frame, notification, size);
    }

    // Loading overlay.
    if app.loading {
        render_loading_indicator(app, frame, size);
    }

    // Modal dialogs (search, help, etc.) are drawn last so they sit on top.
    if let AppMode::Modal(ref kind) = app.mode {
        render_modal(app, frame, kind, size);
    }

    // Command palette.
    if app.mode == AppMode::Command {
        render_command_palette(app, frame, size);
    }
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

    // Record the whole header tab bar as a single clickable region.
    // Individual tab hit-testing is done in the click handler by dividing evenly.
    app.regions.header_tab_bar = Some(header_chunks[1]);
    // Store per-tab regions: divide the inner area (minus borders) evenly.
    if !app.tabs.is_empty() {
        let tab_area = header_chunks[1];
        // The Tabs widget has a BOTTOM border, so clickable area is row 0 of the block.
        let inner_y = tab_area.y;
        let inner_x = tab_area.x + 1; // account for left padding
        let usable_w = tab_area.width.saturating_sub(2);
        let tab_count = app.tabs.len() as u16;
        let tab_width = if tab_count > 0 {
            usable_w / tab_count
        } else {
            0
        };
        for i in 0..app.tabs.len() {
            let x = inner_x + (i as u16) * tab_width;
            let w = if i as u16 == tab_count - 1 {
                usable_w - (i as u16) * tab_width
            } else {
                tab_width
            };
            app.regions
                .header_tabs
                .push((Rect::new(x, inner_y, w, tab_area.height), i));
        }
    }
    app.regions.env_selector = Some(header_chunks[2]);

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

    // --- Request tabs (with Alt+N number hints) ---
    let tab_titles: Vec<Line> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let label = tab.title();
            let truncated = if label.len() > 18 {
                format!("{}...", &label[..15])
            } else {
                label
            };
            let num = i + 1;
            let display = if num <= 9 {
                format!("{num} {truncated}")
            } else {
                truncated
            };
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
        .select(app.active_tab)
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

    // Record individual sidebar item regions (one per visible row).
    let content_area = sidebar_chunks[1];
    let visible_rows = content_area.height as usize;
    for i in 0..visible_rows {
        app.regions.sidebar_items.push(Rect::new(
            content_area.x,
            content_area.y + i as u16,
            content_area.width,
            1,
        ));
    }

    // Delegate actual content rendering to the sidebar widget module.
    crate::ui::widgets::sidebar::render(app, frame, sidebar_chunks[1]);
}

fn render_sidebar_section_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    let sections = [
        (SidebarSection::Collections, "Collections"),
        (SidebarSection::Chains, "Chains"),
        (SidebarSection::History, "History"),
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
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // URL bar
            Constraint::Length(2),      // Request tab selector
            Constraint::Percentage(40), // Request body / params area
            Constraint::Length(2),      // Response tab selector
            Constraint::Min(0),         // Response body area
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
    use crate::core::request::Protocol;

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

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::UrlBar;

    let border_style = border_style_for(app, focused);

    let method_span = Span::styled(method_label, method_style_val);
    let url_span = Span::styled(
        &tab.request.url,
        Style::default().fg(theme.colors.foreground),
    );
    let cursor_hint = if focused && app.mode == AppMode::Insert {
        Span::styled(" [INSERT] ", Style::default().fg(theme.colors.warning))
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
}

// ---------------------------------------------------------------------------
// Request tab bar
// ---------------------------------------------------------------------------

fn render_request_tab_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    // Record clickable regions for each request tab.
    app.regions.request_tab_bar = Some(area);
    let all_tabs = RequestTabKind::all();
    let tab_count = all_tabs.len() as u16;
    let tab_width = if tab_count > 0 {
        area.width / tab_count
    } else {
        0
    };
    for (i, kind) in all_tabs.iter().enumerate() {
        let x = area.x + (i as u16) * tab_width;
        let w = if i as u16 == tab_count - 1 {
            area.width - (i as u16) * tab_width
        } else {
            tab_width
        };
        app.regions
            .request_tabs
            .push((Rect::new(x, area.y, w, area.height), *kind));
    }

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::RequestTabs;

    let tab_titles: Vec<Line> = RequestTabKind::all()
        .iter()
        .map(|kind| {
            let style = if tab.request_tab == *kind {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else if focused {
                Style::default().fg(theme.colors.foreground)
            } else {
                theme.muted_style()
            };
            // Append item count badges where relevant.
            let label = match kind {
                RequestTabKind::Params if !tab.request.params.is_empty() => {
                    format!("{} ({})", kind.label(), tab.request.params.len())
                }
                RequestTabKind::Headers if !tab.request.headers.is_empty() => {
                    format!("{} ({})", kind.label(), tab.request.headers.len())
                }
                RequestTabKind::Assertions if !tab.request.assertions.is_empty() => {
                    format!("{} ({})", kind.label(), tab.request.assertions.len())
                }
                _ => kind.label().to_string(),
            };
            Line::from(Span::styled(label, style))
        })
        .collect();

    let selected = RequestTabKind::all()
        .iter()
        .position(|k| *k == tab.request_tab)
        .unwrap_or(0);

    let tabs_widget = Tabs::new(tab_titles)
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" | ", theme.muted_style()));

    frame.render_widget(tabs_widget, area);
}

// ---------------------------------------------------------------------------
// Request area (body / params / headers / auth / assertions)
// ---------------------------------------------------------------------------

fn render_request_area(app: &mut App, frame: &mut Frame, area: Rect) {
    app.regions.request_body = Some(area);

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::RequestBody;

    let border_style = border_style_for(app, focused);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Delegate to specialized renderers based on active request tab.
    match tab.request_tab {
        RequestTabKind::Params => {
            render_key_value_table(
                &tab.request.params,
                "No query parameters",
                theme,
                frame,
                inner,
            );
        }
        RequestTabKind::Headers => {
            render_key_value_table(&tab.request.headers, "No headers", theme, frame, inner);
        }
        RequestTabKind::Auth => {
            let text = match &tab.request.auth {
                Some(auth) => format!("{auth:?}"),
                None => "No authentication configured".to_string(),
            };
            let paragraph = Paragraph::new(text)
                .style(theme.muted_style())
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, inner);
        }
        RequestTabKind::Body => {
            let body_text = match &tab.request.body {
                Some(crate::core::request::RequestBody::Json(s)) => s.clone(),
                Some(crate::core::request::RequestBody::Raw { content, .. }) => content.clone(),
                Some(crate::core::request::RequestBody::GraphQL { query, .. }) => query.clone(),
                Some(crate::core::request::RequestBody::Protobuf { message }) => message.clone(),
                Some(crate::core::request::RequestBody::None) | None => {
                    String::from("No request body")
                }
                Some(
                    crate::core::request::RequestBody::FormData(pairs)
                    | crate::core::request::RequestBody::FormUrlEncoded(pairs),
                ) => pairs
                    .iter()
                    .map(|p| format!("{}: {}", p.key, p.value))
                    .collect::<Vec<_>>()
                    .join("\n"),
                Some(crate::core::request::RequestBody::Binary(path)) => {
                    format!("[Binary: {}]", path.display())
                }
            };
            let paragraph = Paragraph::new(body_text)
                .style(Style::default().fg(theme.colors.foreground))
                .wrap(Wrap { trim: false });
            frame.render_widget(paragraph, inner);
        }
        RequestTabKind::Assertions => {
            if tab.request.assertions.is_empty() {
                let p = Paragraph::new("No assertions defined").style(theme.muted_style());
                frame.render_widget(p, inner);
            } else {
                let lines: Vec<Line> = tab
                    .request
                    .assertions
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        Line::from(Span::styled(
                            format!("  {}. {:?}", i + 1, a),
                            Style::default().fg(theme.colors.foreground),
                        ))
                    })
                    .collect();
                let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
                frame.render_widget(paragraph, inner);
            }
        }
    }
}

/// Render a simple key-value table (used for params and headers).
fn render_key_value_table(
    pairs: &[crate::core::request::KeyValuePair],
    empty_msg: &str,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    if pairs.is_empty() {
        let p = Paragraph::new(empty_msg).style(theme.muted_style());
        frame.render_widget(p, area);
        return;
    }

    let lines: Vec<Line> = pairs
        .iter()
        .map(|kv| {
            let enabled_indicator = if kv.enabled { " " } else { "x " };
            let key_style = Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD);
            let val_style = Style::default().fg(theme.colors.foreground);
            let disabled_style = theme.muted_style();

            if kv.enabled {
                Line::from(vec![
                    Span::styled(enabled_indicator, val_style),
                    Span::styled(&kv.key, key_style),
                    Span::styled(": ", theme.muted_style()),
                    Span::styled(&kv.value, val_style),
                ])
            } else {
                Line::from(vec![
                    Span::styled(enabled_indicator, disabled_style),
                    Span::styled(&kv.key, disabled_style),
                    Span::styled(": ", disabled_style),
                    Span::styled(&kv.value, disabled_style),
                ])
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Response tab bar
// ---------------------------------------------------------------------------

fn render_response_tab_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::core::request::Protocol;

    // Pre-compute the layout to record response tab clickable regions.
    let bar_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)])
        .split(area);

    let protocol_tabs = ResponseTabKind::for_protocol(&app.active_tab().request.protocol);

    app.regions.response_tab_bar = Some(bar_chunks[1]);
    let resp_count = protocol_tabs.len() as u16;
    let resp_tab_w = if resp_count > 0 {
        bar_chunks[1].width / resp_count
    } else {
        0
    };
    for (i, kind) in protocol_tabs.iter().enumerate() {
        let x = bar_chunks[1].x + (i as u16) * resp_tab_w;
        let w = if i as u16 == resp_count - 1 {
            bar_chunks[1].width - (i as u16) * resp_tab_w
        } else {
            resp_tab_w
        };
        app.regions.response_tabs.push((
            Rect::new(x, bar_chunks[1].y, w, bar_chunks[1].height),
            *kind,
        ));
    }

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::ResponseTabs;

    // Status summary — protocol-aware.
    let status_line = match tab.request.protocol {
        Protocol::WebSocket => {
            if let Some(ref session) = tab.ws_session {
                use crate::protocols::websocket::WsStatus;
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
                Line::from(vec![
                    Span::styled(format!(" {dot} "), dot_style),
                    Span::styled("WS", theme.protocol_style_ws()),
                    Span::styled(label, Style::default().fg(theme.colors.foreground)),
                ])
            } else {
                Line::from(Span::styled(
                    " WS — Press Enter to connect",
                    theme.muted_style(),
                ))
            }
        }
        Protocol::Sse => {
            if let Some(ref session) = tab.sse_session {
                use crate::protocols::sse::SseStatus;
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
                Line::from(vec![
                    Span::styled(format!(" {dot} "), dot_style),
                    Span::styled("SSE", theme.protocol_style_sse()),
                    Span::styled(label, Style::default().fg(theme.colors.foreground)),
                ])
            } else {
                Line::from(Span::styled(
                    " SSE — Press Enter to connect",
                    theme.muted_style(),
                ))
            }
        }
        _ => {
            if let Some(ref resp) = tab.response {
                let status_style = theme.status_style(resp.status);
                Line::from(vec![
                    Span::styled(
                        format!(" {} {} ", resp.status, resp.status_text),
                        status_style,
                    ),
                    Span::styled(
                        format!(" {} ", resp.timing.format_total()),
                        Style::default().fg(theme.colors.foreground),
                    ),
                    Span::styled(format!(" {} ", resp.size.format()), theme.muted_style()),
                ])
            } else {
                Line::from(Span::styled(" No response yet", theme.muted_style()))
            }
        }
    };
    frame.render_widget(Paragraph::new(status_line), bar_chunks[0]);

    // Response tabs — protocol-aware.
    let tab_titles: Vec<Line> = protocol_tabs
        .iter()
        .map(|kind| {
            let style = if tab.response_tab == *kind {
                Style::default()
                    .fg(theme.colors.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else if focused {
                Style::default().fg(theme.colors.foreground)
            } else {
                theme.muted_style()
            };
            Line::from(Span::styled(kind.label(), style))
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
        .divider(Span::styled(" | ", theme.muted_style()));

    frame.render_widget(tabs_widget, bar_chunks[1]);
}

// ---------------------------------------------------------------------------
// Response area
// ---------------------------------------------------------------------------

fn render_response_area(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::core::request::Protocol;

    app.regions.response_body = Some(area);

    let protocol = app.active_tab().request.protocol.clone();
    match protocol {
        Protocol::WebSocket => render_ws_response(app, frame, area),
        Protocol::Sse => render_sse_response(app, frame, area),
        _ => render_http_response(app, frame, area),
    }
}

fn render_http_response(app: &mut App, frame: &mut Frame, area: Rect) {
    let scroll_offset = app.response_scroll;

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::ResponseBody;

    let border_style = border_style_for(app, focused);

    let title = if focused {
        " Response [j/k scroll] "
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

    let Some(ref response) = tab.response else {
        let empty = Paragraph::new("Send a request to see the response here.")
            .style(theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    };

    match tab.response_tab {
        ResponseTabKind::Body => {
            let body = response.body_text().unwrap_or("[binary data]");
            // Try JSON syntax highlighting first
            let content_type = response.header_value("content-type").unwrap_or("");
            let is_json = content_type.contains("json")
                || body.trim_start().starts_with('{')
                || body.trim_start().starts_with('[');

            let paragraph = if is_json {
                let lines = crate::utils::pretty_print::highlight_json(
                    body,
                    &theme.colors.syntax,
                    theme.colors.foreground,
                );
                Paragraph::new(lines).scroll((scroll_offset as u16, 0))
            } else {
                Paragraph::new(crate::utils::pretty_print::pretty_xml(body))
                    .style(Style::default().fg(theme.colors.foreground))
                    .scroll((scroll_offset as u16, 0))
            };
            frame.render_widget(paragraph, inner);
        }
        ResponseTabKind::Headers => {
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
                frame.render_widget(p, inner);
            } else {
                let p = Paragraph::new(lines).scroll((scroll_offset as u16, 0));
                frame.render_widget(p, inner);
            }
        }
        ResponseTabKind::Cookies => {
            if response.cookies.is_empty() {
                let p = Paragraph::new("No cookies").style(theme.muted_style());
                frame.render_widget(p, inner);
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
                frame.render_widget(
                    Paragraph::new(lines).scroll((scroll_offset as u16, 0)),
                    inner,
                );
            }
        }
        ResponseTabKind::Timing => {
            let timing = &response.timing;
            let lines = vec![
                Line::from(vec![
                    Span::styled("DNS Lookup:      ", theme.muted_style()),
                    Span::styled(
                        format!("{:.1}ms", timing.dns_lookup.as_secs_f64() * 1000.0),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("TCP Connect:     ", theme.muted_style()),
                    Span::styled(
                        format!("{:.1}ms", timing.tcp_connect.as_secs_f64() * 1000.0),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("TLS Handshake:   ", theme.muted_style()),
                    Span::styled(
                        timing.tls_handshake.map_or_else(
                            || "N/A".to_string(),
                            |d| format!("{:.1}ms", d.as_secs_f64() * 1000.0),
                        ),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("First Byte:      ", theme.muted_style()),
                    Span::styled(
                        format!("{:.1}ms", timing.first_byte.as_secs_f64() * 1000.0),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Content Download: ", theme.muted_style()),
                    Span::styled(
                        format!("{:.1}ms", timing.content_download.as_secs_f64() * 1000.0),
                        Style::default().fg(theme.colors.foreground),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "Total:           ",
                        Style::default()
                            .fg(theme.colors.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        timing.format_total(),
                        Style::default()
                            .fg(theme.colors.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];
            frame.render_widget(Paragraph::new(lines), inner);
        }
        ResponseTabKind::Assertions => {
            if response.assertion_results.is_empty() {
                let p = Paragraph::new("No assertion results").style(theme.muted_style());
                frame.render_widget(p, inner);
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
                frame.render_widget(Paragraph::new(lines), inner);
            }
        }
        // WS/SSE tabs handled by their own renderers; should not reach here.
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// WebSocket response area
// ---------------------------------------------------------------------------

fn render_ws_response(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::protocols::websocket::{MessageDirection, WsContent};

    let theme = &app.theme;
    let tab = app.active_tab();
    let focused = app.focus == FocusArea::ResponseBody;

    match tab.response_tab {
        ResponseTabKind::WsMessages => {
            // Split into message list + input bar.
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(3)])
                .split(area);

            // ── Message list ──
            let msg_border = border_style_for(app, focused);

            let msg_block = Block::default()
                .title(Span::styled(
                    " Messages ",
                    Style::default()
                        .fg(theme.colors.foreground)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_set(theme.border_set())
                .border_style(msg_border);

            let msg_inner = msg_block.inner(chunks[0]);
            frame.render_widget(msg_block, chunks[0]);

            if let Some(ref session) = tab.ws_session {
                let visible_height = msg_inner.height as usize;
                let total = session.messages.len();
                let scroll = tab.ws_message_scroll;

                let start = if total <= visible_height {
                    0
                } else if scroll + visible_height > total {
                    total.saturating_sub(visible_height)
                } else {
                    scroll
                };

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
                            WsContent::Text(t) => {
                                if t.len() > 200 {
                                    format!("{}…", &t[..200])
                                } else {
                                    t.clone()
                                }
                            }
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
                .borders(Borders::ALL)
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
                use crate::protocols::websocket::WsStatus;
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
                frame.render_widget(Paragraph::new(lines), inner);
            } else {
                let p = Paragraph::new("No WebSocket session")
                    .style(theme.muted_style())
                    .alignment(Alignment::Center);
                frame.render_widget(p, inner);
            }
        }
        _ => {} // fallback — should not happen for WS protocol
    }
}

// ---------------------------------------------------------------------------
// SSE response area
// ---------------------------------------------------------------------------

fn render_sse_response(app: &mut App, frame: &mut Frame, area: Rect) {
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
            let inner = block.inner(area);
            frame.render_widget(block, area);

            if let Some(ref session) = tab.sse_session {
                let visible_height = inner.height as usize;
                let total = session.events.len();
                let scroll = tab.sse_event_scroll;

                let start = if total <= visible_height {
                    0
                } else if scroll + visible_height > total {
                    total.saturating_sub(visible_height)
                } else {
                    scroll
                };

                let lines: Vec<Line> = session
                    .events
                    .iter()
                    .skip(start)
                    .take(visible_height)
                    .map(|evt| {
                        let time = evt.timestamp.format("%H:%M:%S").to_string();
                        let event_type = evt.event_type.as_deref().unwrap_or("message");
                        let data_preview = if evt.data.len() > 100 {
                            format!("{}…", &evt.data[..100])
                        } else {
                            evt.data.clone()
                        };

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
                let text = Paragraph::new(session.accumulated_text.as_str())
                    .style(Style::default().fg(theme.colors.foreground))
                    .wrap(Wrap { trim: false })
                    .scroll((tab.sse_event_scroll as u16, 0));
                frame.render_widget(text, inner);
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
                use crate::protocols::sse::SseStatus;
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
                frame.render_widget(Paragraph::new(lines), inner);
            } else {
                let p = Paragraph::new("No SSE session")
                    .style(theme.muted_style())
                    .alignment(Alignment::Center);
                frame.render_widget(p, inner);
            }
        }
        _ => {} // fallback — should not happen for SSE protocol
    }
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

    let help_hint = if app.mode == AppMode::Normal && app.nav_mode == crate::app::NavMode::Global {
        " hjkl:nav  Enter:focus  Tab:cycle  q:quit  ?:help "
    } else {
        match app.focus {
            FocusArea::Sidebar => " Esc:global  j/k:nav  Tab:next  ?:help ",
            FocusArea::UrlBar => " Esc:global  i:edit  m:method/protocol  Enter:send  Tab:next ",
            FocusArea::RequestTabs | FocusArea::ResponseTabs => {
                " Esc:global  h/l:switch tab  1-5:jump  Tab:next "
            }
            FocusArea::ResponseBody => {
                use crate::core::request::Protocol;
                match app.active_tab().request.protocol {
                    Protocol::WebSocket => " Esc:global  i:input  j/k:scroll  q:disconnect ",
                    Protocol::Sse => " Esc:global  j/k:scroll  a:toggle view  q:disconnect ",
                    _ => " Esc:global  j/k:scroll  ^d/^u:page  g/G:top/end  1-5:tab ",
                }
            }
            FocusArea::RequestBody => " Esc:global  j/k:scroll  ^d/^u:page  g/G:top/end  1-5:tab ",
            _ => " Esc:global  ?:help  Tab:focus ",
        }
    };

    let status_line = Line::from(vec![
        mode_span,
        Span::styled(
            format!("  {focus_label}  "),
            Style::default().fg(theme.colors.foreground),
        ),
        Span::styled(
            format!(
                "  {} tab{} ",
                app.tabs.len(),
                if app.tabs.len() == 1 { "" } else { "s" }
            ),
            theme.muted_style(),
        ),
        Span::styled(
            format!(
                "  {} collection{} ",
                app.collections.len(),
                if app.collections.len() == 1 { "" } else { "s" }
            ),
            theme.muted_style(),
        ),
        Span::raw(""),
    ]);

    // We render the left portion, then overlay the help hint on the right.
    let bar_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(help_hint.len() as u16),
        ])
        .split(area);

    let left = Paragraph::new(status_line).style(Style::default().bg(theme.colors.background));
    frame.render_widget(left, bar_chunks[0]);

    let right = Paragraph::new(help_hint)
        .style(theme.muted_style())
        .alignment(Alignment::Right);
    frame.render_widget(right, bar_chunks[1]);
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
    let toast_width = (msg.len() as u16 + 4).min(area.width.saturating_sub(2));
    let toast_height = 3_u16;

    // Position in top-right corner.
    let toast_area = Rect::new(
        area.width.saturating_sub(toast_width + 1),
        1,
        toast_width,
        toast_height,
    );

    frame.render_widget(Clear, toast_area);

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

fn render_loading_indicator(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let spinner_frames = ["|", "/", "-", "\\"];
    // Use elapsed time to pick a frame (roughly ~4 fps based on tick interval).
    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        / 250) as usize
        % spinner_frames.len();

    let label = format!(" {} Sending... ", spinner_frames[idx]);
    let width = label.len() as u16 + 2;
    let height = 3_u16;

    // Centred overlay.
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    let loading_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, loading_area);

    let loading = Paragraph::new(label).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_set(theme.border_set())
            .border_style(Style::default().fg(theme.colors.warning))
            .style(Style::default().bg(theme.colors.background)),
    );
    frame.render_widget(loading, loading_area);
}

// ---------------------------------------------------------------------------
// Modal overlay (search, help, confirm, etc.)
// ---------------------------------------------------------------------------

fn render_modal(app: &App, frame: &mut Frame, kind: &ModalKind, area: Rect) {
    let theme = &app.theme;

    // Compute a centred rectangle whose size depends on the modal type.
    let (width_pct, height_pct) = match kind {
        ModalKind::Help => (80, 90),
        ModalKind::Search => (60, 50),
        ModalKind::EnvironmentEdit => (65, 70),
        ModalKind::Confirm(_) => (40, 20),
        ModalKind::Import | ModalKind::Export | ModalKind::CurlImport => (60, 40),
        ModalKind::LoadTestConfig => (60, 60),
        ModalKind::DiffSelector => (50, 40),
        ModalKind::RenameTab | ModalKind::RenameCollection(_) | ModalKind::RenameRequest { .. } => {
            (40, 15)
        }
        ModalKind::CollectionPicker => (50, 50),
    };

    let modal_area = centered_rect(width_pct, height_pct, area);
    frame.render_widget(Clear, modal_area);

    match kind {
        ModalKind::Help => render_help_modal(app, frame, modal_area),
        ModalKind::Search => render_search_modal(app, frame, modal_area),
        ModalKind::RenameTab => render_rename_modal(app, frame, modal_area),
        ModalKind::CollectionPicker => render_collection_picker_modal(app, frame, modal_area),
        ModalKind::RenameCollection(_) => render_rename_collection_modal(app, frame, modal_area),
        ModalKind::RenameRequest { .. } => render_rename_request_modal(app, frame, modal_area),
        ModalKind::Confirm(msg) => render_confirm_modal(app, frame, modal_area, msg),
        ModalKind::Import => render_import_modal(app, frame, modal_area),
        ModalKind::Export => render_export_modal(app, frame, modal_area),
        _ => {
            let title = match kind {
                ModalKind::EnvironmentEdit => " Edit Environment ",
                ModalKind::LoadTestConfig => " Load Test Configuration ",
                ModalKind::DiffSelector => " Select Responses to Diff ",
                ModalKind::CurlImport => " Import from cURL ",
                _ => " Modal ",
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
            frame.render_widget(block, modal_area);
        }
    }
}

fn render_help_modal(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

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

    let help_text = vec![
        // ── Navigation ──────────────────────────────────────────
        heading("Navigation"),
        row("Tab", "Cycle focus forward"),
        row("Shift+Tab", "Cycle focus backward"),
        row("j / Down", "Navigate down / scroll"),
        row("k / Up", "Navigate up / scroll"),
        row("h / Left", "Collapse / prev sub-tab"),
        row("l / Right", "Expand / next sub-tab"),
        row("1-5", "Jump to sub-tab by number"),
        row("J (Shift+j)", "Page down (15 lines)"),
        row("K (Shift+k)", "Page up (15 lines)"),
        row("g", "Scroll to top"),
        row("G (Shift+g)", "Scroll to bottom"),
        Line::from(""),
        // ── Requests ────────────────────────────────────────────
        heading("Requests"),
        row("Enter", "Send request / sidebar action"),
        row("Ctrl+R", "Send request"),
        row("i", "Insert mode (edit URL)"),
        row("m", "Cycle method/protocol"),
        row("y", "Copy response to clipboard"),
        row("Ctrl+S / s", "Save to collection"),
        Line::from(""),
        // ── Tabs ────────────────────────────────────────────────
        heading("Tabs"),
        row("t / Ctrl+N", "New tab"),
        row("w", "Close tab"),
        row("n", "Next tab"),
        row("b", "Previous tab"),
        row("Alt+1-9", "Jump to tab by number"),
        row("F2", "Rename tab"),
        Line::from(""),
        // ── Sidebar ─────────────────────────────────────────────
        heading("Sidebar"),
        row("Enter / l", "Open / expand"),
        row("h", "Collapse"),
        row("a", "Add request to collection"),
        row("x", "Delete selected request"),
        row("r", "Rename collection/request"),
        Line::from(""),
        // ── Modes ───────────────────────────────────────────────
        heading("Modes"),
        row(":", "Command mode"),
        row("/ or p", "Search requests"),
        row("Ctrl+P", "Fuzzy search"),
        row("e / Ctrl+E", "Cycle environment"),
        row("Ctrl+I", "Import modal"),
        row("Ctrl+X", "Export modal"),
        row("d", "Diff selector"),
        row("?", "Show this help"),
        row("Esc", "Back to normal mode"),
        row("q", "Quit"),
        row("Ctrl+C", "Force quit"),
        Line::from(""),
        // ── Real-time Protocols ─────────────────────────────────
        heading("Real-time Protocols (WS / SSE)"),
        row("m", "Cycle to WS/SSE protocol"),
        row("Enter", "Connect / disconnect toggle"),
        row("i", "WS: type message (Insert)"),
        row("j / k", "Scroll messages / events"),
        row("a", "SSE: toggle accumulated view"),
        row("q", "Disconnect (on response)"),
        Line::from(""),
        // ── Commands ────────────────────────────────────────────
        heading("Commands (: prefix)"),
        row(":q / :quit", "Quit application"),
        row(":help", "Show this help"),
        row(":rename <name>", "Rename current request"),
        row(":theme <name>", "Switch theme"),
        Line::from(""),
        heading("Collections"),
        row(":newcol <name>", "Create collection"),
        row(":delcol [name]", "Delete collection"),
        row(":save", "Save request to collection"),
        row(":delreq", "Delete selected request"),
        row(":addvar <k> <v>", "Add collection variable"),
        Line::from(""),
        heading("Environments"),
        row(":env <name>", "Set active environment"),
        row(":newenv <name>", "Create environment"),
        row(":dupenv [name]", "Duplicate environment"),
        row(":env-file <path>", "Load .env file"),
        Line::from(""),
        heading("Import / Export"),
        row(":import <path>", "Import (auto-detect format)"),
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
        heading("Testing"),
        row(":loadtest <n> <c>", "Load test (n reqs, c concur.)"),
        row(":diff", "Open diff selector"),
        Line::from(""),
        heading("Settings  (:set <key> <value>)"),
        row("timeout", "Request timeout in ms"),
        row("follow_redirects", "true / false"),
        row("verify_ssl", "true / false"),
        row("vim_mode", "true / false"),
        row("history_limit", "Max history entries"),
        row("theme", "Theme name"),
        Line::from(""),
        heading("Other"),
        row(":clearhistory", "Clear request history"),
        Line::from(""),
        heading("Themes"),
        row("catppuccin", "(default)"),
        row("dracula", ""),
        row("gruvbox", ""),
        row("tokyo-night", ""),
        Line::from(""),
        Line::from(Span::styled(
            " j/k scroll  J/K page  g/G top/bottom  PgUp/PgDn  Esc close",
            theme.muted_style(),
        )),
    ];

    let scroll = app.help_scroll as u16;

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

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

fn render_search_modal(app: &App, frame: &mut Frame, area: Rect) {
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
    } else {
        let items: Vec<ratatui::widgets::ListItem> = app
            .search_results
            .iter()
            .map(|r| {
                let method_style = r
                    .method
                    .as_ref()
                    .map_or(theme.muted_style(), |m| theme.method_style(m));
                let method_label = r
                    .method
                    .as_ref()
                    .map_or("???", super::super::core::request::HttpMethod::as_str);
                ratatui::widgets::ListItem::new(Line::from(vec![
                    Span::styled(format!("{method_label:<7} "), method_style),
                    Span::styled(&r.name, Style::default().fg(theme.colors.foreground)),
                    Span::styled(
                        r.collection_name
                            .as_deref()
                            .map(|c| format!("  ({c})"))
                            .unwrap_or_default(),
                        theme.muted_style(),
                    ),
                ]))
            })
            .collect();

        let list = ratatui::widgets::List::new(items)
            .highlight_style(theme.selected_style())
            .highlight_symbol("> ");
        frame.render_widget(list, search_chunks[1]);
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

fn render_collection_picker_modal(app: &App, frame: &mut Frame, area: Rect) {
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
        return;
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
        "  j/k:navigate  Enter:select  Esc:cancel",
        theme.muted_style(),
    ));
    frame.render_widget(hint, chunks[2]);
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
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled("  Enter file path:", theme.muted_style()));
    frame.render_widget(label, chunks[0]);

    let input_text = format!("  {}\u{2588}", app.modal_input);
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
        Span::styled("*", Style::default().fg(theme.colors.accent)),
        Span::styled(" cURL", theme.muted_style()),
    ]));
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
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let label = Paragraph::new(Span::styled(
        "  Enter output file path:",
        theme.muted_style(),
    ));
    frame.render_widget(label, chunks[0]);

    let input_text = format!("  {}\u{2588}", app.modal_input);
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
    ]));
    frame.render_widget(formats, chunks[3]);
}

// ---------------------------------------------------------------------------
// Command palette
// ---------------------------------------------------------------------------

fn render_command_palette(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let palette_width = (area.width * 60 / 100).min(80);
    let palette_height = 3_u16;
    let x = (area.width.saturating_sub(palette_width)) / 2;
    let y = area.height / 4;
    let palette_area = Rect::new(x, y, palette_width, palette_height);

    frame.render_widget(Clear, palette_area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled(
            " : ",
            Style::default()
                .fg(theme.colors.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &app.command_input,
            Style::default().fg(theme.colors.foreground),
        ),
        Span::styled("_", Style::default().fg(theme.colors.accent)),
    ]))
    .block(
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
}

// ---------------------------------------------------------------------------
// Chain runner
// ---------------------------------------------------------------------------

fn render_chain_runner(app: &App, frame: &mut Frame, area: Rect) {
    use crate::core::chain::ChainStepStatus;

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

    let status_text = if let Some(ref state) = app.active_chain {
        if state.running {
            let total = state.step_statuses.len();
            format!("Running step {}/{}", state.current_step + 1, total)
        } else {
            "Complete".to_string()
        }
    } else {
        "Idle".to_string()
    };

    let running = app.active_chain.as_ref().is_some_and(|s| s.running);
    let status_style = if running {
        Style::default()
            .fg(theme.colors.warning)
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

                let (icon, icon_style) = match status {
                    ChainStepStatus::Pending => ("  ", Style::default().fg(theme.colors.muted)),
                    ChainStepStatus::Running => (
                        "  ",
                        Style::default()
                            .fg(theme.colors.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    ChainStepStatus::Success { .. } => {
                        ("  ", Style::default().fg(theme.colors.success))
                    }
                    ChainStepStatus::Failed { .. } => {
                        ("  ", Style::default().fg(theme.colors.error))
                    }
                    ChainStepStatus::Skipped { .. } => ("  ", theme.muted_style()),
                };

                // Find request name from collection
                let req_name = app
                    .active_chain_coll_idx
                    .and_then(|ci| app.collections.get(ci))
                    .and_then(|c| c.find_request(&step.request_id))
                    .map_or_else(
                        || format!("Request {}", &step.request_id.to_string()[..8]),
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
                        let short = if error.len() > 40 {
                            &error[..40]
                        } else {
                            error
                        };
                        format!("  {short}")
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
                    Span::styled(icon, icon_style),
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
                    ListItem::new(line).style(theme.selected_style())
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
