use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind, RequestTabKind, ResponseTabKind,
    SidebarSection,
};
use crate::core::request::HttpMethod;
use crate::protocols::sse::{SseStatus, SseEvent as ProtocolSseEvent};
use crate::protocols::websocket::{WsStatus, WsMessage};

#[derive(Debug, Clone)]
pub enum WsEventData {
    Connected,
    Disconnected,
    MessageReceived(WsMessage),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum SseEventData {
    Connected,
    Disconnected,
    Event(ProtocolSseEvent),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ChainStepEvent {
    Running { step_index: usize },
    Success { step_index: usize, status: u16, duration_ms: u64, extracted: std::collections::HashMap<String, String> },
    Failed { step_index: usize, error: String },
    Skipped { step_index: usize, reason: String },
    Complete,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
    Resize(u16, u16),
    RequestComplete,
    Notification(String, NotificationKind),
    WebSocketEvent { session_id: Uuid, event: WsEventData },
    SseEvent { session_id: Uuid, event: SseEventData },
    ChainStepComplete(ChainStepEvent),
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    _tx: mpsc::UnboundedSender<AppEvent>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();

        // Use a dedicated OS thread for blocking crossterm I/O.
        std::thread::spawn(move || {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let sent = match evt {
                            Event::Key(key) => event_tx.send(AppEvent::Key(key)),
                            Event::Mouse(mouse) => {
                                // Filter out noisy move/drag events.
                                match mouse.kind {
                                    crossterm::event::MouseEventKind::Moved
                                    | crossterm::event::MouseEventKind::Drag(_) => Ok(()),
                                    _ => event_tx.send(AppEvent::Mouse(mouse)),
                                }
                            }
                            Event::Resize(w, h) => event_tx.send(AppEvent::Resize(w, h)),
                            _ => Ok(()),
                        };
                        if sent.is_err() {
                            break; // channel closed, app is shutting down
                        }
                    }
                } else if event_tx.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        Self { rx, _tx: tx }
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self._tx.clone()
    }
}

pub async fn handle_event(app: &mut App, event: AppEvent) -> Result<()> {
    match event {
        AppEvent::Key(key) => handle_key(app, key).await?,
        AppEvent::Mouse(mouse) => handle_mouse(app, mouse).await?,
        AppEvent::Tick => {
            app.clear_expired_notification();
        }
        AppEvent::Resize(_, _) => {}
        AppEvent::RequestComplete => {}
        AppEvent::Notification(msg, kind) => {
            app.notify(msg, kind);
        }
        AppEvent::WebSocketEvent { session_id, event } => {
            handle_ws_protocol_event(app, session_id, event);
        }
        AppEvent::SseEvent { session_id, event } => {
            handle_sse_protocol_event(app, session_id, event);
        }
        AppEvent::ChainStepComplete(chain_event) => {
            handle_chain_step_event(app, chain_event);
        }
    }
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    // Global keybindings (work in any mode)
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.should_quit = true;
            return Ok(());
        }
        _ => {}
    }

    match &app.mode {
        AppMode::Normal => handle_normal_mode(app, key).await,
        AppMode::Insert => handle_insert_mode(app, key).await,
        AppMode::Command => handle_command_mode(app, key).await,
        AppMode::Modal(kind) => {
            let kind = kind.clone();
            handle_modal_mode(app, key, &kind).await
        }
        AppMode::ChainEditor => handle_chain_editor_mode(app, key).await,
        AppMode::ProxyInspector => handle_proxy_mode(app, key).await,
    }
}

async fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Result<()> {
    let col = mouse.column;
    let row = mouse.row;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_click(app, col, row).await?;
        }
        MouseEventKind::ScrollUp => {
            handle_mouse_scroll(app, col, row, true);
        }
        MouseEventKind::ScrollDown => {
            handle_mouse_scroll(app, col, row, false);
        }
        _ => {}
    }

    Ok(())
}

fn hit_test(rect: Option<Rect>, col: u16, row: u16) -> bool {
    match rect {
        Some(r) => col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height,
        None => false,
    }
}

fn hit_test_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

use ratatui::layout::Rect;

async fn handle_mouse_click(app: &mut App, col: u16, row: u16) -> Result<()> {
    let regions = app.regions.clone();
    let num_tabs = app.tabs.len();

    // Check header tab bar clicks (switch between request tabs).
    // First try per-tab regions, then fallback to dividing the whole bar.
    for (rect, idx) in &regions.header_tabs {
        if hit_test_rect(*rect, col, row) {
            app.switch_tab(*idx);
            app.response_scroll = 0;
            return Ok(());
        }
    }
    // Fallback: if click is anywhere in the header tab bar area, pick tab by position.
    if num_tabs > 0 {
        if let Some(bar) = regions.header_tab_bar {
            if hit_test(Some(bar), col, row) {
                let relative_x = col.saturating_sub(bar.x);
                let tab_idx = (relative_x as usize * num_tabs) / bar.width.max(1) as usize;
                let tab_idx = tab_idx.min(num_tabs - 1);
                app.switch_tab(tab_idx);
                app.response_scroll = 0;
                return Ok(());
            }
        }
    }

    // Check new tab button
    if hit_test(regions.new_tab_button, col, row) {
        app.new_tab();
        return Ok(());
    }

    // Check environment selector
    if hit_test(regions.env_selector, col, row) {
        if !app.environments.is_empty() {
            app.active_env = Some(match app.active_env {
                Some(i) => (i + 1) % app.environments.len(),
                None => 0,
            });
            let name = app
                .active_environment()
                .map(|e| e.name.clone())
                .unwrap_or_default();
            app.notify(format!("Environment: {}", name), NotificationKind::Info);
        }
        return Ok(());
    }

    // Check method selector click (cycle method/protocol)
    if hit_test(regions.method_selector, col, row) {
        cycle_protocol_method(app.active_tab_mut());
        return Ok(());
    }

    // Check send button
    if hit_test(regions.send_button, col, row) {
        app.send_request().await?;
        return Ok(());
    }

    // Check URL bar click
    if hit_test(regions.url_bar, col, row) {
        app.focus = FocusArea::UrlBar;
        app.mode = AppMode::Insert;
        app.nav_mode = NavMode::Panel;
        return Ok(());
    }

    // Check request tab bar clicks
    for (rect, kind) in &regions.request_tabs {
        if hit_test_rect(*rect, col, row) {
            app.active_tab_mut().request_tab = *kind;
            app.focus = FocusArea::RequestTabs;
            return Ok(());
        }
    }

    // Check request body area
    if hit_test(regions.request_body, col, row) {
        app.focus = FocusArea::RequestBody;
        app.nav_mode = NavMode::Panel;
        return Ok(());
    }

    // Check response tab bar clicks
    for (rect, kind) in &regions.response_tabs {
        if hit_test_rect(*rect, col, row) {
            app.active_tab_mut().response_tab = *kind;
            app.focus = FocusArea::ResponseTabs;
            return Ok(());
        }
    }

    // Check WS input bar click -> enter insert mode for WS messaging
    if hit_test(regions.ws_input_bar, col, row) {
        app.focus = FocusArea::ResponseBody;
        app.mode = AppMode::Insert;
        app.nav_mode = NavMode::Panel;
        return Ok(());
    }

    // Check response body area
    if hit_test(regions.response_body, col, row) {
        app.focus = FocusArea::ResponseBody;
        app.nav_mode = NavMode::Panel;
        return Ok(());
    }

    // Check sidebar section tabs
    for (rect, section) in &regions.sidebar_section_tabs {
        if hit_test_rect(*rect, col, row) {
            app.sidebar_state.section = *section;
            app.focus = FocusArea::Sidebar;
            return Ok(());
        }
    }

    // Check sidebar item clicks — select the item and trigger the action
    // (expand/collapse collections, open requests).
    for (i, rect) in regions.sidebar_items.iter().enumerate() {
        if hit_test_rect(*rect, col, row) {
            app.sidebar_state.selected = app.sidebar_state.scroll_offset + i;
            app.focus = FocusArea::Sidebar;
            app.nav_mode = NavMode::Panel;
            handle_sidebar_action(app);
            return Ok(());
        }
    }

    // Check search result clicks (when in search modal)
    for (i, rect) in regions.search_results_items.iter().enumerate() {
        if hit_test_rect(*rect, col, row) {
            if let Some(result) = app.search_results.get(i) {
                let request_id = result.request_id;
                let coll_idx = result.collection_index;
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Some(req) = coll.find_request(&request_id) {
                        let tab =
                            crate::app::RequestTab::from_request(req.clone(), Some(coll_idx));
                        app.tabs.push(tab);
                        app.active_tab = app.tabs.len() - 1;
                    }
                }
                app.mode = AppMode::Normal;
            }
            return Ok(());
        }
    }

    // Check sidebar area (general click for focus)
    if hit_test(regions.sidebar, col, row) {
        app.focus = FocusArea::Sidebar;
        return Ok(());
    }

    // Check status bar
    if hit_test(regions.status_bar, col, row) {
        // No specific action, but could be extended
    }

    Ok(())
}

fn handle_mouse_scroll(app: &mut App, col: u16, row: u16, up: bool) {
    let regions = &app.regions;

    // Scroll in sidebar
    if hit_test(regions.sidebar, col, row) {
        if up {
            app.sidebar_state.selected = app.sidebar_state.selected.saturating_sub(1);
        } else {
            app.sidebar_state.selected = app.sidebar_state.selected.saturating_add(1);
        }
        return;
    }

    // Scroll in response body (protocol-aware)
    if hit_test(regions.response_body, col, row) {
        use crate::core::request::Protocol;
        let delta: usize = 3;
        match app.active_tab().request.protocol {
            Protocol::WebSocket => {
                let tab = app.active_tab_mut();
                if up {
                    tab.ws_message_scroll = tab.ws_message_scroll.saturating_sub(delta);
                } else {
                    tab.ws_message_scroll = tab.ws_message_scroll.saturating_add(delta);
                }
            }
            Protocol::Sse => {
                let tab = app.active_tab_mut();
                if up {
                    tab.sse_event_scroll = tab.sse_event_scroll.saturating_sub(delta);
                } else {
                    tab.sse_event_scroll = tab.sse_event_scroll.saturating_add(delta);
                }
            }
            _ => {
                if up {
                    app.response_scroll = app.response_scroll.saturating_sub(delta);
                } else {
                    app.response_scroll = app.response_scroll.saturating_add(delta);
                }
            }
        }
        return;
    }

    // Scroll in request body
    if hit_test(regions.request_body, col, row) {
        // Could add request body scroll offset if needed
    }
}

async fn handle_normal_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        // ── Enter: Global → Panel mode; Panel → Send/Sidebar action ──
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if app.nav_mode == NavMode::Global {
                app.nav_mode = NavMode::Panel;
            } else {
                if app.focus == FocusArea::Sidebar {
                    handle_sidebar_action(app);
                } else {
                    app.send_request().await?;
                }
            }
        }

        // ── Esc: Panel → Global mode ────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Esc) => {
            if app.nav_mode == NavMode::Panel {
                app.nav_mode = NavMode::Global;
                app.snap_focus_to_major_panel();
            }
        }

        // ── Search ───────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('/'))
        | (KeyModifiers::NONE, KeyCode::Char('p')) => {
            app.mode = AppMode::Modal(ModalKind::Search);
            app.search_query.clear();
            app.search_results.clear();
        }

        // ── Environment cycle ────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('e')) => {
            if app.environments.is_empty() {
                app.notify("No environments configured".into(), NotificationKind::Warning);
            } else {
                app.active_env = Some(match app.active_env {
                    Some(i) => (i + 1) % app.environments.len(),
                    None => 0,
                });
                let name = app.active_environment().map(|e| e.name.clone()).unwrap_or_default();
                app.notify(format!("Environment: {}", name), NotificationKind::Info);
            }
        }

        // ── New tab ──────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('t')) => {
            app.new_tab();
        }

        // ── Close tab ────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('w')) => {
            app.close_tab();
        }

        // ── Next / previous header tab ───────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('n')) => {
            app.next_tab();
            app.response_scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Char('b')) => {
            app.prev_tab();
            app.response_scroll = 0;
        }

        // ── Tab switching with Alt+number ────────────────────────────
        (KeyModifiers::ALT, KeyCode::Char(c)) if c.is_ascii_digit() => {
            let idx = c.to_digit(10).unwrap_or(0) as usize;
            if idx > 0 {
                app.switch_tab(idx - 1);
            }
        }

        // ── Rename current tab (F2) ──────────────────────────────────
        (KeyModifiers::NONE, KeyCode::F(2)) => {
            app.rename_input = app.active_tab().request.name.clone();
            app.mode = AppMode::Modal(ModalKind::RenameTab);
        }

        // ── Focus cycling ────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Tab) => {
            if app.nav_mode == NavMode::Global {
                app.cycle_major_focus_forward();
            } else {
                app.cycle_focus_forward();
            }
        }
        (KeyModifiers::SHIFT, KeyCode::BackTab) => {
            if app.nav_mode == NavMode::Global {
                app.cycle_major_focus_backward();
            } else {
                app.cycle_focus_backward();
            }
        }

        // ── Navigation (vim-style) ──────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            if app.nav_mode == NavMode::Global {
                app.global_nav_down();
            } else {
                handle_nav_down(app);
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            if app.nav_mode == NavMode::Global {
                app.global_nav_up();
            } else {
                handle_nav_up(app);
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
            if app.nav_mode == NavMode::Global {
                app.global_nav_left();
            } else {
                handle_nav_left(app);
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
            if app.nav_mode == NavMode::Global {
                app.global_nav_right();
            } else {
                handle_nav_right(app);
            }
        }

        // ── Alt+hjkl: panel navigation shortcut (works in Panel mode) ─
        (KeyModifiers::ALT, KeyCode::Char('j')) => {
            app.global_nav_down();
        }
        (KeyModifiers::ALT, KeyCode::Char('k')) => {
            app.global_nav_up();
        }
        (KeyModifiers::ALT, KeyCode::Char('h')) => {
            app.global_nav_left();
        }
        (KeyModifiers::ALT, KeyCode::Char('l')) => {
            app.global_nav_right();
        }

        // ── Enter insert mode ────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('i')) => {
            app.nav_mode = NavMode::Panel;
            // When focus is on ResponseBody with WS protocol, enter Insert for WS input
            if app.focus == FocusArea::ResponseBody {
                if let crate::core::request::Protocol::WebSocket = app.active_tab().request.protocol {
                    app.mode = AppMode::Insert;
                    return Ok(());
                }
            }
            app.mode = AppMode::Insert;
        }

        // ── Command mode ─────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char(':')) => {
            app.mode = AppMode::Command;
            app.command_input.clear();
        }

        // ── Help ─────────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            app.mode = AppMode::Modal(ModalKind::Help);
        }

        // ── Quit / Disconnect ────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('q')) => {
            // Protocol-aware: disconnect WS/SSE when on response area
            if app.focus == FocusArea::ResponseBody {
                match app.active_tab().request.protocol {
                    crate::core::request::Protocol::WebSocket => {
                        if let Some(ref tx) = app.active_tab().ws_cmd_sender {
                            let _ = tx.send(crate::protocols::websocket::WsCommand::Disconnect);
                        }
                        let tab = app.active_tab_mut();
                        tab.ws_cmd_sender = None;
                        if let Some(ref mut s) = tab.ws_session {
                            s.status = crate::protocols::websocket::WsStatus::Disconnected;
                        }
                        app.notify("Disconnected".into(), NotificationKind::Info);
                        return Ok(());
                    }
                    crate::core::request::Protocol::Sse => {
                        if let Some(ref tx) = app.active_tab().sse_cmd_sender {
                            let _ = tx.send(crate::protocols::sse::SseCommand::Disconnect);
                        }
                        let tab = app.active_tab_mut();
                        tab.sse_cmd_sender = None;
                        if let Some(ref mut s) = tab.sse_session {
                            s.status = crate::protocols::sse::SseStatus::Disconnected;
                        }
                        app.notify("Disconnected".into(), NotificationKind::Info);
                        return Ok(());
                    }
                    _ => {}
                }
            }
            if app.nav_mode == NavMode::Global {
                app.should_quit = true;
            } else {
                app.nav_mode = NavMode::Global;
                app.snap_focus_to_major_panel();
            }
        }

        // ── Save ─────────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('s')) => {
            if let Some(coll_idx) = app.active_tab().collection_index {
                // Update request in place within its collection
                let req = app.active_tab().request.clone();
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    if let Some(existing) = coll.find_request_mut(&req.id) {
                        *existing = req;
                    }
                }
                save_all_collections(app);
                app.active_tab_mut().dirty = false;
                app.notify("Saved".into(), NotificationKind::Success);
            } else if app.collections.is_empty() {
                app.notify("No collections. Use :newcol <name> first".into(), NotificationKind::Warning);
            } else {
                // Open collection picker
                app.collection_picker_selected = 0;
                app.mode = AppMode::Modal(ModalKind::CollectionPicker);
            }
        }

        // ── SSE toggle accumulated view (when on response body) ──────
        (KeyModifiers::NONE, KeyCode::Char('a')) if app.focus == FocusArea::ResponseBody => {
            if let crate::core::request::Protocol::Sse = app.active_tab().request.protocol {
                let tab = app.active_tab_mut();
                tab.sse_show_accumulated = !tab.sse_show_accumulated;
                tab.response_tab = if tab.sse_show_accumulated {
                    ResponseTabKind::SseStream
                } else {
                    ResponseTabKind::SseEvents
                };
            }
        }

        // ── Sidebar: add new empty request to collection ─────────────
        (KeyModifiers::NONE, KeyCode::Char('a')) if app.focus == FocusArea::Sidebar => {
            if app.sidebar_state.section == SidebarSection::Collections {
                let items = build_sidebar_items(app);
                if let Some(item) = items.get(app.sidebar_state.selected) {
                    match item {
                        SidebarItem::Collection { coll_idx, .. }
                        | SidebarItem::Folder { coll_idx, .. } => {
                            let coll_idx = *coll_idx;
                            let req = crate::core::request::Request::new(
                                "New Request",
                                crate::core::request::HttpMethod::GET,
                                "",
                            );
                            app.collections[coll_idx].add_request(req.clone());
                            save_all_collections(app);
                            // Open the new request in a new tab
                            let tab = crate::app::RequestTab::from_request(req, Some(coll_idx));
                            app.tabs.push(tab);
                            app.active_tab = app.tabs.len() - 1;
                            app.focus = FocusArea::UrlBar;
                            app.nav_mode = NavMode::Panel;
                            let name = app.collections[coll_idx].name.clone();
                            app.notify(format!("New request in '{}'", name), NotificationKind::Success);
                        }
                        SidebarItem::Request { coll_idx, .. } => {
                            // If a request is selected, add to its parent collection
                            let coll_idx = *coll_idx;
                            let req = crate::core::request::Request::new(
                                "New Request",
                                crate::core::request::HttpMethod::GET,
                                "",
                            );
                            app.collections[coll_idx].add_request(req.clone());
                            save_all_collections(app);
                            let tab = crate::app::RequestTab::from_request(req, Some(coll_idx));
                            app.tabs.push(tab);
                            app.active_tab = app.tabs.len() - 1;
                            app.focus = FocusArea::UrlBar;
                            app.nav_mode = NavMode::Panel;
                            let name = app.collections[coll_idx].name.clone();
                            app.notify(format!("New request in '{}'", name), NotificationKind::Success);
                        }
                    }
                }
            }
        }

        // ── Sidebar: delete request ─────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('x')) if app.focus == FocusArea::Sidebar => {
            if app.sidebar_state.section == SidebarSection::Collections {
                let items = build_sidebar_items(app);
                if let Some(SidebarItem::Request { coll_idx, request_id }) = items.get(app.sidebar_state.selected) {
                    let coll_idx = *coll_idx;
                    let request_id = *request_id;
                    remove_request_from_collection(&mut app.collections[coll_idx].items, &request_id);
                    save_all_collections(app);
                    app.notify("Request deleted".into(), NotificationKind::Success);
                    // Adjust selection
                    let new_len = build_sidebar_items(app).len();
                    if app.sidebar_state.selected >= new_len && new_len > 0 {
                        app.sidebar_state.selected = new_len - 1;
                    }
                } else {
                    app.notify("Select a request to delete".into(), NotificationKind::Warning);
                }
            }
        }

        // ── Sidebar: rename collection or request ─────────────────
        (KeyModifiers::NONE, KeyCode::Char('r')) if app.focus == FocusArea::Sidebar => {
            if app.sidebar_state.section == SidebarSection::Collections {
                let items = build_sidebar_items(app);
                if let Some(item) = items.get(app.sidebar_state.selected) {
                    match item {
                        SidebarItem::Collection { coll_idx, .. }
                        | SidebarItem::Folder { coll_idx, .. } => {
                            let coll_idx = *coll_idx;
                            app.rename_input = app.collections[coll_idx].name.clone();
                            app.mode = AppMode::Modal(ModalKind::RenameCollection(coll_idx));
                        }
                        SidebarItem::Request { coll_idx, request_id } => {
                            let coll_idx = *coll_idx;
                            let request_id = *request_id;
                            if let Some(coll) = app.collections.get(coll_idx) {
                                if let Some(req) = coll.find_request(&request_id) {
                                    app.rename_input = req.name.clone();
                                    app.mode = AppMode::Modal(ModalKind::RenameRequest { coll_idx, request_id });
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Copy response ────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('y')) => {
            if let Some(resp) = &app.active_tab().response {
                if let Some(text) = resp.body_text() {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(text);
                        app.notify("Copied to clipboard".into(), NotificationKind::Success);
                    }
                }
            }
        }

        // ── Diff ─────────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('d')) => {
            app.mode = AppMode::Modal(ModalKind::DiffSelector);
        }

        // ── Method/protocol cycling (when on URL bar) ────────────────
        (KeyModifiers::NONE, KeyCode::Char('m')) => {
            if app.focus == FocusArea::UrlBar {
                cycle_protocol_method(app.active_tab_mut());
            }
        }

        // ── Page scroll (J = half page down, K = half page up) ──────
        (KeyModifiers::SHIFT, KeyCode::Char('J')) => {
            if app.focus == FocusArea::ResponseBody || app.focus == FocusArea::RequestBody {
                app.response_scroll = app.response_scroll.saturating_add(15);
            }
        }
        (KeyModifiers::SHIFT, KeyCode::Char('K')) => {
            if app.focus == FocusArea::ResponseBody || app.focus == FocusArea::RequestBody {
                app.response_scroll = app.response_scroll.saturating_sub(15);
            }
        }

        // ── Top/bottom of scroll ─────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('g')) => {
            if app.focus == FocusArea::ResponseBody || app.focus == FocusArea::RequestBody {
                app.response_scroll = 0;
            }
        }
        (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
            if app.focus == FocusArea::ResponseBody || app.focus == FocusArea::RequestBody {
                app.response_scroll = usize::MAX / 2; // will be clamped by rendering
            }
        }

        // ── Number keys 1-5 to switch request/response sub-tabs ─────
        (KeyModifiers::NONE, KeyCode::Char(c @ '1'..='5')) => {
            let idx = (c as usize) - ('1' as usize);
            match app.focus {
                FocusArea::RequestTabs | FocusArea::RequestBody => {
                    let tabs = RequestTabKind::all();
                    if idx < tabs.len() {
                        app.active_tab_mut().request_tab = tabs[idx];
                        app.response_scroll = 0;
                    }
                }
                FocusArea::ResponseTabs | FocusArea::ResponseBody => {
                    let protocol = app.active_tab().request.protocol.clone();
                    let tabs = ResponseTabKind::for_protocol(&protocol);
                    if idx < tabs.len() {
                        app.active_tab_mut().response_tab = tabs[idx];
                        app.response_scroll = 0;
                    }
                }
                _ => {}
            }
        }

        // ── Ctrl+R: Send request ────────────────────────────────────
        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
            app.send_request().await?;
        }

        // ── Ctrl+S: Save ────────────────────────────────────────────
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
            if let Some(coll_idx) = app.active_tab().collection_index {
                let req = app.active_tab().request.clone();
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    if let Some(existing) = coll.find_request_mut(&req.id) {
                        *existing = req;
                    }
                }
                save_all_collections(app);
                app.active_tab_mut().dirty = false;
                app.notify("Saved".into(), NotificationKind::Success);
            } else if app.collections.is_empty() {
                app.notify("No collections. Use :newcol <name> first".into(), NotificationKind::Warning);
            } else {
                app.collection_picker_selected = 0;
                app.mode = AppMode::Modal(ModalKind::CollectionPicker);
            }
        }

        // ── Ctrl+P: Fuzzy search ────────────────────────────────────
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.mode = AppMode::Modal(ModalKind::Search);
            app.search_query.clear();
            app.search_results.clear();
        }

        // ── Ctrl+E: Switch environment ──────────────────────────────
        (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            if app.environments.is_empty() {
                app.notify("No environments configured".into(), NotificationKind::Warning);
            } else {
                app.active_env = Some(match app.active_env {
                    Some(i) => (i + 1) % app.environments.len(),
                    None => 0,
                });
                let name = app.active_environment().map(|e| e.name.clone()).unwrap_or_default();
                app.notify(format!("Environment: {}", name), NotificationKind::Info);
            }
        }

        // ── Ctrl+N: New request/tab ─────────────────────────────────
        (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            app.new_tab();
        }

        // ── Ctrl+I: Import modal ────────────────────────────────────
        (KeyModifiers::CONTROL, KeyCode::Char('i')) => {
            app.modal_input.clear();
            app.mode = AppMode::Modal(ModalKind::Import);
        }

        // ── Ctrl+X: Export modal ────────────────────────────────────
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
            app.modal_input.clear();
            app.mode = AppMode::Modal(ModalKind::Export);
        }

        _ => {}
    }
    Ok(())
}

async fn handle_insert_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    // WS message input when focused on ResponseBody with WebSocket protocol
    if app.focus == FocusArea::ResponseBody {
        if let crate::core::request::Protocol::WebSocket = app.active_tab().request.protocol {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    app.mode = AppMode::Normal;
                    app.nav_mode = NavMode::Panel;
                }
                (KeyModifiers::NONE, KeyCode::Char(c)) => {
                    app.active_tab_mut().ws_message_input.push(c);
                }
                (KeyModifiers::NONE, KeyCode::Backspace) => {
                    app.active_tab_mut().ws_message_input.pop();
                }
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    let tab = app.active_tab_mut();
                    if !tab.ws_message_input.is_empty() {
                        let text = tab.ws_message_input.clone();
                        tab.ws_message_input.clear();
                        // Record message locally
                        if let Some(ref mut session) = tab.ws_session {
                            session.messages.push(crate::protocols::websocket::WsMessage {
                                direction: crate::protocols::websocket::MessageDirection::Sent,
                                content: crate::protocols::websocket::WsContent::Text(text.clone()),
                                timestamp: chrono::Utc::now(),
                            });
                            tab.ws_message_scroll = session.messages.len().saturating_sub(1);
                        }
                        // Send via cmd channel
                        if let Some(ref tx) = tab.ws_cmd_sender {
                            let _ = tx.send(crate::protocols::websocket::WsCommand::SendText(text));
                        }
                    }
                }
                (KeyModifiers::NONE, KeyCode::Tab) => {
                    app.mode = AppMode::Normal;
                    app.cycle_focus_forward();
                }
                _ => {}
            }
            return Ok(());
        }
    }

    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.mode = AppMode::Normal;
            app.nav_mode = NavMode::Panel;
        }
        (KeyModifiers::NONE, KeyCode::Char(c)) => match app.focus {
            FocusArea::UrlBar => {
                app.active_tab_mut().request.url.push(c);
                app.active_tab_mut().dirty = true;
            }
            _ => {}
        },
        (KeyModifiers::NONE, KeyCode::Backspace) => match app.focus {
            FocusArea::UrlBar => {
                app.active_tab_mut().request.url.pop();
                app.active_tab_mut().dirty = true;
            }
            _ => {}
        },
        (KeyModifiers::NONE, KeyCode::Enter) => match app.focus {
            FocusArea::UrlBar => {
                app.mode = AppMode::Normal;
                app.send_request().await?;
            }
            _ => {}
        },
        (KeyModifiers::NONE, KeyCode::Tab) => {
            app.mode = AppMode::Normal;
            app.cycle_focus_forward();
        }
        _ => {}
    }
    Ok(())
}

async fn handle_command_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.nav_mode = NavMode::Panel;
            app.command_input.clear();
        }
        KeyCode::Enter => {
            let cmd = app.command_input.clone();
            app.mode = AppMode::Normal;
            execute_command(app, &cmd).await?;
            app.command_input.clear();
        }
        KeyCode::Char(c) => {
            app.command_input.push(c);
        }
        KeyCode::Backspace => {
            app.command_input.pop();
            if app.command_input.is_empty() {
                app.mode = AppMode::Normal;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn handle_modal_mode(app: &mut App, key: KeyEvent, kind: &ModalKind) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        _ => match kind {
            ModalKind::Search => handle_search_modal(app, key),
            ModalKind::RenameTab => handle_rename_modal(app, key),
            ModalKind::CollectionPicker => handle_collection_picker_modal(app, key),
            ModalKind::RenameCollection(idx) => {
                let idx = *idx;
                handle_rename_collection_modal(app, key, idx);
            }
            ModalKind::RenameRequest { coll_idx, request_id } => {
                let coll_idx = *coll_idx;
                let request_id = *request_id;
                handle_rename_request_modal(app, key, coll_idx, request_id);
            }
            ModalKind::Import => handle_import_modal(app, key),
            ModalKind::Export => handle_export_modal(app, key),
            ModalKind::Help => {
                match (key.modifiers, key.code) {
                    (_, KeyCode::Char('q')) => app.mode = AppMode::Normal,
                    (KeyModifiers::NONE, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                        app.help_scroll = app.help_scroll.saturating_add(1);
                    }
                    (KeyModifiers::NONE, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                        app.help_scroll = app.help_scroll.saturating_sub(1);
                    }
                    (KeyModifiers::SHIFT, KeyCode::Char('J')) => {
                        app.help_scroll = app.help_scroll.saturating_add(10);
                    }
                    (KeyModifiers::SHIFT, KeyCode::Char('K')) => {
                        app.help_scroll = app.help_scroll.saturating_sub(10);
                    }
                    (KeyModifiers::NONE, KeyCode::Char('g')) => {
                        app.help_scroll = 0;
                    }
                    (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                        app.help_scroll = usize::MAX / 2;
                    }
                    (_, KeyCode::PageDown) => {
                        app.help_scroll = app.help_scroll.saturating_add(10);
                    }
                    (_, KeyCode::PageUp) => {
                        app.help_scroll = app.help_scroll.saturating_sub(10);
                    }
                    _ => {}
                }
            }
            _ => {}
        },
    }
    Ok(())
}

fn handle_search_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => {
            app.search_query.push(c);
            update_search_results(app);
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            update_search_results(app);
        }
        KeyCode::Enter => {
            // Select the first search result
            if let Some(result) = app.search_results.first() {
                let request_id = result.request_id;
                let coll_idx = result.collection_index;
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Some(req) = coll.find_request(&request_id) {
                        let tab = crate::app::RequestTab::from_request(req.clone(), Some(coll_idx));
                        app.tabs.push(tab);
                        app.active_tab = app.tabs.len() - 1;
                    }
                }
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_rename_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.clone();
            if !new_name.is_empty() {
                app.active_tab_mut().request.name = new_name;
                app.active_tab_mut().dirty = true;
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_collection_picker_modal(app: &mut App, key: KeyEvent) {
    if app.collections.is_empty() {
        app.mode = AppMode::Normal;
        return;
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let max = app.collections.len().saturating_sub(1);
            app.collection_picker_selected = (app.collection_picker_selected + 1).min(max);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.collection_picker_selected = app.collection_picker_selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            let coll_idx = app.collection_picker_selected;
            let req = app.active_tab().request.clone();
            app.collections[coll_idx].add_request(req);
            app.active_tab_mut().collection_index = Some(coll_idx);
            app.active_tab_mut().dirty = false;
            save_all_collections(app);
            let name = app.collections[coll_idx].name.clone();
            app.notify(format!("Saved to '{}'", name), NotificationKind::Success);
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_rename_collection_modal(app: &mut App, key: KeyEvent, coll_idx: usize) {
    match key.code {
        KeyCode::Char(c) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.clone();
            if !new_name.is_empty() {
                // Delete the old collection file before renaming
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Ok(store) = crate::storage::collections_store::CollectionsStore::new(
                        app.config.collections_dir.clone(),
                    ) {
                        let _ = store.delete_collection(coll);
                    }
                }
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    coll.name = new_name.clone();
                }
                save_all_collections(app);
                app.notify(format!("Renamed to '{}'", new_name), NotificationKind::Success);
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_rename_request_modal(app: &mut App, key: KeyEvent, coll_idx: usize, request_id: uuid::Uuid) {
    match key.code {
        KeyCode::Char(c) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.clone();
            if !new_name.is_empty() {
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    if let Some(req) = coll.find_request_mut(&request_id) {
                        req.name = new_name.clone();
                    }
                }
                save_all_collections(app);
                app.notify(format!("Renamed to '{}'", new_name), NotificationKind::Success);
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_import_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => {
            app.modal_input.push(c);
        }
        KeyCode::Backspace => {
            app.modal_input.pop();
        }
        KeyCode::Enter => {
            let path = app.modal_input.clone();
            app.mode = AppMode::Normal;
            if !path.is_empty() {
                execute_import(app, path.trim());
            }
            app.modal_input.clear();
        }
        _ => {}
    }
}

fn handle_export_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => {
            app.modal_input.push(c);
        }
        KeyCode::Backspace => {
            app.modal_input.pop();
        }
        KeyCode::Enter => {
            let path = app.modal_input.clone();
            app.mode = AppMode::Normal;
            if !path.is_empty() {
                execute_export(app, path.trim());
            }
            app.modal_input.clear();
        }
        _ => {}
    }
}

fn update_search_results(app: &mut App) {
    use fuzzy_matcher::skim::SkimMatcherV2;
    use fuzzy_matcher::FuzzyMatcher;

    let matcher = SkimMatcherV2::default();
    let query = &app.search_query;

    app.search_results.clear();

    if query.is_empty() {
        return;
    }

    for (coll_idx, coll) in app.collections.iter().enumerate() {
        for req in coll.all_requests() {
            let search_str = format!("{} {} {}", req.method, req.name, req.url);
            if matcher.fuzzy_match(&search_str, query).is_some() {
                app.search_results.push(crate::app::SearchResult {
                    name: req.name.clone(),
                    method: Some(req.method),
                    url: req.url.clone(),
                    collection_name: Some(coll.name.clone()),
                    request_id: req.id,
                    collection_index: coll_idx,
                });
            }
        }
    }
}

async fn handle_chain_editor_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            // Abort chain if running
            if let Some(ref mut state) = app.active_chain {
                state.running = false;
            }
            app.mode = AppMode::Normal;
            app.active_chain = None;
            app.active_chain_def = None;
            app.active_chain_coll_idx = None;
            app.chain_scroll = 0;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(ref def) = app.active_chain_def {
                let max = def.steps.len().saturating_sub(1);
                app.chain_scroll = (app.chain_scroll + 1).min(max);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.chain_scroll = app.chain_scroll.saturating_sub(1);
        }
        _ => {}
    }
    Ok(())
}

async fn handle_proxy_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
    Ok(())
}


// ---------------------------------------------------------------------------
// Chain execution
// ---------------------------------------------------------------------------

pub fn start_chain_execution(app: &mut App, chain: crate::core::chain::RequestChain, coll_idx: usize) {
    use crate::core::chain::ChainExecutionState;

    let mut state = ChainExecutionState::new(&chain);
    state.running = true;
    app.active_chain = Some(state);
    app.active_chain_def = Some(chain.clone());
    app.active_chain_coll_idx = Some(coll_idx);
    app.chain_scroll = 0;
    app.mode = AppMode::ChainEditor;
    app.focus = FocusArea::ChainSteps;

    // Clone what the async task needs
    let http_client = app.http_client.clone();
    let collection = app.collections[coll_idx].clone();
    let event_tx = app.event_tx();
    let chain_def = chain;

    tokio::spawn(async move {
        run_chain_task(http_client, collection, chain_def, event_tx).await;
    });
}

async fn run_chain_task(
    http_client: crate::core::client::HttpClient,
    collection: crate::core::collection::Collection,
    chain: crate::core::chain::RequestChain,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
) {
    use crate::core::chain::{evaluate_condition, extract_values, StepCondition};
    use crate::core::variables::VariableResolver;
    use std::collections::HashMap;

    let mut extracted_variables: HashMap<String, String> = HashMap::new();
    let mut last_response: Option<crate::core::response::Response> = None;

    for (step_index, step) in chain.steps.iter().enumerate() {
        // Check condition
        if let Some(ref condition) = step.condition {
            if !evaluate_condition(condition, last_response.as_ref(), &extracted_variables) {
                let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Skipped {
                    step_index,
                    reason: "Condition not met".into(),
                }));
                continue;
            }
        }

        // Notify: Running
        let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Running { step_index }));

        // Find request
        let request = match collection.find_request(&step.request_id) {
            Some(req) => req.clone(),
            None => {
                let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Failed {
                    step_index,
                    error: format!("Request {} not found in collection", step.request_id),
                }));
                break;
            }
        };

        // Build resolver with current extracted variables
        let resolver = VariableResolver::from_context(
            Some(&extracted_variables),
            &collection.variables,
            None,
            None,
            None,
        );

        // Send request
        match http_client.send(&request, &resolver).await {
            Ok(response) => {
                let status = response.status;
                let duration_ms = response.timing.total.as_millis() as u64;

                // Extract values
                let new_vars = extract_values(&step.extractions, &response);
                extracted_variables.extend(new_vars.clone());
                last_response = Some(response);

                let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Success {
                    step_index,
                    status,
                    duration_ms,
                    extracted: new_vars,
                }));
            }
            Err(e) => {
                let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Failed {
                    step_index,
                    error: e.to_string(),
                }));
                break;
            }
        }

        // Apply delay
        if let Some(delay) = step.delay_ms {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
    }

    let _ = event_tx.send(AppEvent::ChainStepComplete(ChainStepEvent::Complete));
}

fn handle_chain_step_event(app: &mut App, event: ChainStepEvent) {
    use crate::core::chain::ChainStepStatus;

    match event {
        ChainStepEvent::Running { step_index } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Running;
                    state.current_step = step_index;
                }
            }
        }
        ChainStepEvent::Success { step_index, status, duration_ms, extracted } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Success { status, duration_ms };
                    state.current_step = step_index;
                    state.extracted_variables.extend(extracted);
                }
            }
        }
        ChainStepEvent::Failed { step_index, error } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Failed { error: error.clone() };
                    state.current_step = step_index;
                }
            }
            app.notify(format!("Chain step {} failed: {}", step_index + 1, error), NotificationKind::Error);
        }
        ChainStepEvent::Skipped { step_index, reason } => {
            if let Some(ref mut state) = app.active_chain {
                if step_index < state.step_statuses.len() {
                    state.step_statuses[step_index] = ChainStepStatus::Skipped { reason };
                    state.current_step = step_index;
                }
            }
        }
        ChainStepEvent::Complete => {
            if let Some(ref mut state) = app.active_chain {
                state.running = false;
            }
            app.notify("Chain execution complete".into(), NotificationKind::Success);
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol event handlers
// ---------------------------------------------------------------------------

pub fn handle_ws_protocol_event(app: &mut App, session_id: Uuid, event: WsEventData) {
    // Find the tab that owns this session
    let tab_idx = match app.tabs.iter().position(|t| {
        t.ws_session.as_ref().map(|s| s.id) == Some(session_id)
    }) {
        Some(i) => i,
        None => return,
    };
    let tab = &mut app.tabs[tab_idx];
    let session = match tab.ws_session.as_mut() {
        Some(s) => s,
        None => return,
    };
    match event {
        WsEventData::Connected => {
            session.status = WsStatus::Connected {
                connected_at: chrono::Utc::now(),
            };
        }
        WsEventData::Disconnected => {
            session.status = WsStatus::Disconnected;
            tab.ws_cmd_sender = None;
        }
        WsEventData::MessageReceived(msg) => {
            session.messages.push(msg);
            // Auto-scroll if at or near bottom
            let len = session.messages.len();
            if tab.ws_message_scroll >= len.saturating_sub(2) {
                tab.ws_message_scroll = len.saturating_sub(1);
            }
        }
        WsEventData::Error(e) => {
            session.status = WsStatus::Error(e);
            tab.ws_cmd_sender = None;
        }
    }
}

pub fn handle_sse_protocol_event(app: &mut App, session_id: Uuid, event: SseEventData) {
    // Find the tab that owns this session
    let tab_idx = match app.tabs.iter().position(|t| {
        t.sse_session.as_ref().map(|s| s.id) == Some(session_id)
    }) {
        Some(i) => i,
        None => return,
    };
    let tab = &mut app.tabs[tab_idx];
    let session = match tab.sse_session.as_mut() {
        Some(s) => s,
        None => return,
    };
    match event {
        SseEventData::Connected => {
            session.status = SseStatus::Connected;
        }
        SseEventData::Disconnected => {
            session.status = SseStatus::Disconnected;
            tab.sse_cmd_sender = None;
        }
        SseEventData::Event(evt) => {
            if let Some(ref id) = evt.id {
                session.last_event_id = Some(id.clone());
            }
            session.accumulated_text.push_str(&evt.data);
            session.accumulated_text.push('\n');
            session.events.push(evt);
            // Auto-scroll
            let len = session.events.len();
            if tab.sse_event_scroll >= len.saturating_sub(2) {
                tab.sse_event_scroll = len.saturating_sub(1);
            }
        }
        SseEventData::Error(e) => {
            session.status = SseStatus::Error(e);
            tab.sse_cmd_sender = None;
        }
    }
}

fn cycle_protocol_method(tab: &mut crate::app::RequestTab) {
    use crate::core::request::Protocol;
    match tab.request.protocol {
        Protocol::Http => {
            let methods = HttpMethod::all();
            let idx = methods.iter().position(|m| *m == tab.request.method).unwrap_or(0);
            if idx + 1 < methods.len() {
                tab.request.method = methods[idx + 1];
            } else {
                tab.request.protocol = Protocol::WebSocket;
                tab.response_tab = ResponseTabKind::WsMessages;
            }
        }
        Protocol::WebSocket => {
            tab.request.protocol = Protocol::Sse;
            tab.response_tab = ResponseTabKind::SseEvents;
        }
        Protocol::Sse => {
            tab.request.protocol = Protocol::Http;
            tab.request.method = HttpMethod::GET;
            tab.response_tab = ResponseTabKind::Body;
        }
        _ => {} // GRPC unchanged
    }
}

fn handle_nav_down(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            let max = match app.sidebar_state.section {
                SidebarSection::Collections => {
                    build_sidebar_items(app).len().saturating_sub(1)
                }
                SidebarSection::History => {
                    app.history.entries().len().saturating_sub(1)
                }
                SidebarSection::Chains => {
                    app.collections.iter().map(|c| c.chains.len()).sum::<usize>().saturating_sub(1)
                }
            };
            app.sidebar_state.selected = (app.sidebar_state.selected + 1).min(max);
        }
        FocusArea::RequestBody => {
            app.response_scroll = app.response_scroll.saturating_add(1);
        }
        FocusArea::ResponseBody => {
            // Protocol-aware scroll
            match app.active_tab().request.protocol {
                crate::core::request::Protocol::WebSocket => {
                    let tab = app.active_tab_mut();
                    if let Some(ref session) = tab.ws_session {
                        let max = session.messages.len().saturating_sub(1);
                        tab.ws_message_scroll = (tab.ws_message_scroll + 1).min(max);
                    }
                }
                crate::core::request::Protocol::Sse => {
                    let tab = app.active_tab_mut();
                    if let Some(ref session) = tab.sse_session {
                        let max = session.events.len().saturating_sub(1);
                        tab.sse_event_scroll = (tab.sse_event_scroll + 1).min(max);
                    }
                }
                _ => {
                    app.response_scroll = app.response_scroll.saturating_add(1);
                }
            }
        }
        FocusArea::RequestTabs => {
            let tabs = RequestTabKind::all();
            let current = app.active_tab().request_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx + 1 < tabs.len() {
                app.active_tab_mut().request_tab = tabs[idx + 1];
            }
        }
        FocusArea::ResponseTabs => {
            let protocol = app.active_tab().request.protocol.clone();
            let tabs = ResponseTabKind::for_protocol(&protocol);
            let current = app.active_tab().response_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx + 1 < tabs.len() {
                app.active_tab_mut().response_tab = tabs[idx + 1];
            }
        }
        _ => {}
    }
}

fn handle_nav_up(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            app.sidebar_state.selected = app.sidebar_state.selected.saturating_sub(1);
        }
        FocusArea::RequestBody => {
            app.response_scroll = app.response_scroll.saturating_sub(1);
        }
        FocusArea::ResponseBody => {
            match app.active_tab().request.protocol {
                crate::core::request::Protocol::WebSocket => {
                    app.active_tab_mut().ws_message_scroll =
                        app.active_tab().ws_message_scroll.saturating_sub(1);
                }
                crate::core::request::Protocol::Sse => {
                    app.active_tab_mut().sse_event_scroll =
                        app.active_tab().sse_event_scroll.saturating_sub(1);
                }
                _ => {
                    app.response_scroll = app.response_scroll.saturating_sub(1);
                }
            }
        }
        FocusArea::RequestTabs => {
            let tabs = RequestTabKind::all();
            let current = app.active_tab().request_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx > 0 {
                app.active_tab_mut().request_tab = tabs[idx - 1];
            }
        }
        FocusArea::ResponseTabs => {
            let protocol = app.active_tab().request.protocol.clone();
            let tabs = ResponseTabKind::for_protocol(&protocol);
            let current = app.active_tab().response_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx > 0 {
                app.active_tab_mut().response_tab = tabs[idx - 1];
            }
        }
        _ => {}
    }
}

fn handle_nav_left(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            handle_sidebar_collapse(app);
        }
        FocusArea::RequestTabs | FocusArea::RequestBody => {
            let tabs = RequestTabKind::all();
            let current = app.active_tab().request_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx > 0 {
                app.active_tab_mut().request_tab = tabs[idx - 1];
                app.response_scroll = 0;
            }
        }
        FocusArea::ResponseTabs | FocusArea::ResponseBody => {
            let protocol = app.active_tab().request.protocol.clone();
            let tabs = ResponseTabKind::for_protocol(&protocol);
            let current = app.active_tab().response_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx > 0 {
                app.active_tab_mut().response_tab = tabs[idx - 1];
                app.response_scroll = 0;
            }
        }
        _ => {}
    }
}

fn handle_nav_right(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            handle_sidebar_action(app);
        }
        FocusArea::RequestTabs | FocusArea::RequestBody => {
            let tabs = RequestTabKind::all();
            let current = app.active_tab().request_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx + 1 < tabs.len() {
                app.active_tab_mut().request_tab = tabs[idx + 1];
                app.response_scroll = 0;
            }
        }
        FocusArea::ResponseTabs | FocusArea::ResponseBody => {
            let protocol = app.active_tab().request.protocol.clone();
            let tabs = ResponseTabKind::for_protocol(&protocol);
            let current = app.active_tab().response_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx + 1 < tabs.len() {
                app.active_tab_mut().response_tab = tabs[idx + 1];
                app.response_scroll = 0;
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Sidebar navigation helpers
// ---------------------------------------------------------------------------

/// Describes what a flat sidebar row represents.
#[derive(Debug, Clone)]
pub enum SidebarItem {
    /// A collection header row. `coll_idx` is the index into `app.collections`.
    Collection { coll_idx: usize, id: uuid::Uuid },
    /// A folder row inside a collection.
    Folder { coll_idx: usize, id: uuid::Uuid },
    /// A request row.
    Request { coll_idx: usize, request_id: uuid::Uuid },
}

/// Builds the flat list of sidebar items exactly matching the render order.
pub fn build_sidebar_items(app: &App) -> Vec<SidebarItem> {
    let mut items = Vec::new();
    for (coll_idx, coll) in app.collections.iter().enumerate() {
        items.push(SidebarItem::Collection { coll_idx, id: coll.id });
        if app.sidebar_state.expanded.contains(&coll.id) {
            flatten_sidebar_items(&coll.items, coll_idx, &app.sidebar_state.expanded, &mut items);
        }
    }
    items
}

fn flatten_sidebar_items(
    collection_items: &[crate::core::collection::CollectionItem],
    coll_idx: usize,
    expanded: &std::collections::HashSet<uuid::Uuid>,
    out: &mut Vec<SidebarItem>,
) {
    for item in collection_items {
        match item {
            crate::core::collection::CollectionItem::Request(req) => {
                out.push(SidebarItem::Request { coll_idx, request_id: req.id });
            }
            crate::core::collection::CollectionItem::Folder { id, items, .. } => {
                out.push(SidebarItem::Folder { coll_idx, id: *id });
                if expanded.contains(id) {
                    flatten_sidebar_items(items, coll_idx, expanded, out);
                }
            }
        }
    }
}

/// Toggle expand/collapse or open a request for the currently selected sidebar row.
fn handle_sidebar_action(app: &mut App) {
    if app.sidebar_state.section == SidebarSection::Chains {
        // Find the selected chain across all collections
        let mut chain_idx = 0usize;
        for (coll_idx, coll) in app.collections.iter().enumerate() {
            for chain in &coll.chains {
                if chain_idx == app.sidebar_state.selected {
                    start_chain_execution(app, chain.clone(), coll_idx);
                    return;
                }
                chain_idx += 1;
            }
        }
        return;
    }

    if app.sidebar_state.section == SidebarSection::History {
        let entries = app.history.entries();
        if let Some(entry) = entries.get(app.sidebar_state.selected) {
            let request = crate::core::request::Request::new(
                &format!("{} {}", entry.method, entry.short_url(40)),
                entry.method,
                &entry.url,
            );
            if let Some(req_id) = entry.request_id {
                for (ci, coll) in app.collections.iter().enumerate() {
                    if let Some(found) = coll.find_request(&req_id) {
                        let tab = crate::app::RequestTab::from_request(found.clone(), Some(ci));
                        app.tabs.push(tab);
                        app.active_tab = app.tabs.len() - 1;
                        app.focus = FocusArea::UrlBar;
                        return;
                    }
                }
            }
            let tab = crate::app::RequestTab::from_request(request, None);
            app.tabs.push(tab);
            app.active_tab = app.tabs.len() - 1;
            app.focus = FocusArea::UrlBar;
        }
        return;
    }

    if app.sidebar_state.section != SidebarSection::Collections {
        return;
    }

    let items = build_sidebar_items(app);
    let selected = app.sidebar_state.selected;

    if let Some(item) = items.get(selected) {
        match item {
            SidebarItem::Collection { id, .. } | SidebarItem::Folder { id, .. } => {
                // Toggle expand/collapse
                let id = *id;
                if app.sidebar_state.expanded.contains(&id) {
                    app.sidebar_state.expanded.remove(&id);
                } else {
                    app.sidebar_state.expanded.insert(id);
                }
            }
            SidebarItem::Request { coll_idx, request_id } => {
                let coll_idx = *coll_idx;
                let request_id = *request_id;
                // Open the request in a new tab
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Some(req) = coll.find_request(&request_id) {
                        let tab = crate::app::RequestTab::from_request(req.clone(), Some(coll_idx));
                        app.tabs.push(tab);
                        app.active_tab = app.tabs.len() - 1;
                        app.focus = FocusArea::UrlBar;
                        app.response_scroll = 0;
                    }
                }
            }
        }
    }
}

/// Collapse the currently selected sidebar item (or its parent).
fn handle_sidebar_collapse(app: &mut App) {
    if app.sidebar_state.section != SidebarSection::Collections {
        return;
    }

    let items = build_sidebar_items(app);
    let selected = app.sidebar_state.selected;

    if let Some(item) = items.get(selected) {
        match item {
            SidebarItem::Collection { id, .. } | SidebarItem::Folder { id, .. } => {
                // If expanded, collapse it. If already collapsed, do nothing.
                app.sidebar_state.expanded.remove(id);
            }
            SidebarItem::Request { .. } => {
                // Navigate up to the parent folder/collection
                if selected > 0 {
                    app.sidebar_state.selected = selected.saturating_sub(1);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Command execution
// ---------------------------------------------------------------------------

pub async fn execute_command(app: &mut App, cmd: &str) -> Result<()> {
    let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
    let command = parts[0];
    let args = parts.get(1).map(|s| *s);

    match command {
        "q" | "quit" => {
            app.should_quit = true;
        }
        "env" => {
            if let Some(name) = args {
                if let Some(idx) = app.environments.iter().position(|e| e.name == name) {
                    app.active_env = Some(idx);
                    app.notify(format!("Environment: {}", name), NotificationKind::Info);
                } else {
                    app.notify(
                        format!("Environment '{}' not found", name),
                        NotificationKind::Error,
                    );
                }
            }
        }
        "curl" => {
            // Copy current request as curl
            let tab = app.active_tab();
            let resolver = app.build_resolver();
            let curl = crate::exporters::curl::to_curl(&tab.request, &resolver);
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&curl);
                app.notify("Copied curl command".into(), NotificationKind::Success);
            }
        }
        "paste-curl" => {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(text) = clipboard.get_text() {
                    match crate::importers::curl::parse_curl(&text) {
                        Ok(request) => {
                            app.tabs.push(crate::app::RequestTab::from_request(request, None));
                            app.active_tab = app.tabs.len() - 1;
                            app.notify("Imported from curl".into(), NotificationKind::Success);
                        }
                        Err(e) => {
                            app.notify(format!("Failed to parse curl: {}", e), NotificationKind::Error);
                        }
                    }
                }
            }
        }
        "theme" => {
            if let Some(name) = args {
                match crate::ui::theme::Theme::load(name) {
                    Ok(theme) => {
                        app.theme = theme;
                        app.notify(format!("Theme: {}", name), NotificationKind::Info);
                    }
                    Err(_) => {
                        app.notify(format!("Theme '{}' not found", name), NotificationKind::Error);
                    }
                }
            }
        }
        "import" => {
            if let Some(path) = args {
                execute_import(app, path.trim());
            } else {
                app.modal_input.clear();
                app.mode = AppMode::Modal(ModalKind::Import);
            }
        }
        "export" => {
            if let Some(path) = args {
                execute_export(app, path.trim());
            } else {
                app.modal_input.clear();
                app.mode = AppMode::Modal(ModalKind::Export);
            }
        }
        "loadtest" => {
            if let Some(args_str) = args {
                let parts: Vec<&str> = args_str.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let (Ok(n), Ok(c)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        let config = crate::testing::load_test::LoadTestConfig::new(
                            app.active_tab().request.id,
                            n,
                            c,
                        );
                        let request = app.active_tab().request.clone();
                        let resolver = app.build_resolver();
                        app.notify(
                            format!("Running load test: {} requests, {} concurrency", n, c),
                            NotificationKind::Info,
                        );
                        match crate::testing::load_test::run_load_test(&config, &request, &resolver).await {
                            Ok(result) => {
                                app.notify(
                                    format!(
                                        "Load test complete: {}/{} ok, {:.1} rps, p50={}ms p99={}ms",
                                        result.successful,
                                        result.total_requests,
                                        result.rps,
                                        result.latency.median.as_millis(),
                                        result.latency.p99.as_millis(),
                                    ),
                                    NotificationKind::Success,
                                );
                            }
                            Err(e) => {
                                app.notify(format!("Load test failed: {}", e), NotificationKind::Error);
                            }
                        }
                    }
                }
            }
        }
        "docs" => {
            // Export current collection as markdown documentation
            if let Some(coll_idx) = app.active_tab().collection_index {
                if let Some(coll) = app.collections.get(coll_idx) {
                    let docs = crate::exporters::markdown_docs::generate_docs(coll);
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(&docs);
                        app.notify("Markdown docs copied to clipboard".into(), NotificationKind::Success);
                    }
                }
            } else {
                app.notify("No collection selected".into(), NotificationKind::Warning);
            }
        }
        "help" => {
            app.mode = AppMode::Modal(ModalKind::Help);
        }
        "newcol" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :newcol <name>".into(), NotificationKind::Warning);
                } else {
                    let coll = crate::core::collection::Collection::new(name);
                    app.collections.push(coll);
                    save_all_collections(app);
                    app.notify(format!("Created collection '{}'", name), NotificationKind::Success);
                }
            } else {
                app.notify("Usage: :newcol <name>".into(), NotificationKind::Warning);
            }
        }
        "save" => {
            if let Some(coll_idx) = app.active_tab().collection_index {
                let req = app.active_tab().request.clone();
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    if let Some(existing) = coll.find_request_mut(&req.id) {
                        *existing = req;
                    }
                }
                save_all_collections(app);
                app.active_tab_mut().dirty = false;
                app.notify("Saved".into(), NotificationKind::Success);
            } else if app.collections.is_empty() {
                app.notify("No collections. Use :newcol <name> first".into(), NotificationKind::Warning);
            } else {
                app.collection_picker_selected = 0;
                app.mode = AppMode::Modal(ModalKind::CollectionPicker);
            }
        }
        "delreq" => {
            if app.sidebar_state.section == SidebarSection::Collections {
                let items = build_sidebar_items(app);
                if let Some(SidebarItem::Request { coll_idx, request_id }) = items.get(app.sidebar_state.selected) {
                    let coll_idx = *coll_idx;
                    let request_id = *request_id;
                    remove_request_from_collection(&mut app.collections[coll_idx].items, &request_id);
                    save_all_collections(app);
                    app.notify("Request deleted".into(), NotificationKind::Success);
                    let new_len = build_sidebar_items(app).len();
                    if app.sidebar_state.selected >= new_len && new_len > 0 {
                        app.sidebar_state.selected = new_len - 1;
                    }
                } else {
                    app.notify("Select a request in the sidebar first".into(), NotificationKind::Warning);
                }
            } else {
                app.notify("Switch to Collections sidebar first".into(), NotificationKind::Warning);
            }
        }
        "set" => {
            if let Some(rest) = args {
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    app.notify("Usage: :set <key> <value>".into(), NotificationKind::Warning);
                } else {
                    let key = parts[0];
                    let val = parts[1].trim();
                    match key {
                        "timeout" => {
                            if let Ok(ms) = val.parse::<u64>() {
                                app.config.timeout_ms = ms;
                                app.notify(format!("timeout = {}ms", ms), NotificationKind::Success);
                            } else {
                                app.notify(format!("Invalid timeout value: {}", val), NotificationKind::Error);
                            }
                        }
                        "follow_redirects" => {
                            if let Ok(b) = val.parse::<bool>() {
                                app.config.follow_redirects = b;
                                app.notify(format!("follow_redirects = {}", b), NotificationKind::Success);
                            } else {
                                app.notify(format!("Invalid bool: {}", val), NotificationKind::Error);
                            }
                        }
                        "verify_ssl" => {
                            if let Ok(b) = val.parse::<bool>() {
                                app.config.verify_ssl = b;
                                app.notify(format!("verify_ssl = {}", b), NotificationKind::Success);
                            } else {
                                app.notify(format!("Invalid bool: {}", val), NotificationKind::Error);
                            }
                        }
                        "vim_mode" => {
                            if let Ok(b) = val.parse::<bool>() {
                                app.config.vim_mode = b;
                                app.notify(format!("vim_mode = {}", b), NotificationKind::Success);
                            } else {
                                app.notify(format!("Invalid bool: {}", val), NotificationKind::Error);
                            }
                        }
                        "history_limit" => {
                            if let Ok(n) = val.parse::<usize>() {
                                app.config.history_limit = n;
                                app.notify(format!("history_limit = {}", n), NotificationKind::Success);
                            } else {
                                app.notify(format!("Invalid number: {}", val), NotificationKind::Error);
                            }
                        }
                        "theme" => {
                            match crate::ui::theme::Theme::load(val) {
                                Ok(theme) => {
                                    app.config.theme = val.to_string();
                                    app.theme = theme;
                                    app.notify(format!("theme = {}", val), NotificationKind::Success);
                                }
                                Err(_) => {
                                    app.notify(format!("Theme '{}' not found", val), NotificationKind::Error);
                                }
                            }
                        }
                        _ => {
                            app.notify(format!("Unknown setting: {}", key), NotificationKind::Error);
                        }
                    }
                }
            } else {
                app.notify("Usage: :set <key> <value>".into(), NotificationKind::Warning);
            }
        }
        "env-file" => {
            if let Some(path_str) = args {
                let path_str = path_str.trim();
                let path = std::path::Path::new(path_str);
                let expanded = if path_str.starts_with('~') {
                    if let Some(home) = dirs::home_dir() {
                        home.join(path_str.trim_start_matches("~/"))
                    } else {
                        path.to_path_buf()
                    }
                } else {
                    path.to_path_buf()
                };
                match std::fs::read_to_string(&expanded) {
                    Ok(content) => {
                        let env_name = expanded
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(".env");
                        match crate::importers::dotenv::dotenv_to_environment(env_name, &content) {
                            Ok(env) => {
                                let name = env.name.clone();
                                app.environments.push(env);
                                app.active_env = Some(app.environments.len() - 1);
                                app.notify(
                                    format!("Loaded environment '{}'", name),
                                    NotificationKind::Success,
                                );
                            }
                            Err(e) => {
                                app.notify(
                                    format!("Failed to parse env file: {}", e),
                                    NotificationKind::Error,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        app.notify(format!("Cannot read file: {}", e), NotificationKind::Error);
                    }
                }
            } else {
                app.notify("Usage: :env-file <path>".into(), NotificationKind::Warning);
            }
        }
        "diff" => {
            app.mode = AppMode::Modal(ModalKind::DiffSelector);
        }
        "ws" => {
            if let Some(url) = args {
                let url = url.trim();
                if url.is_empty() {
                    app.notify("Usage: :ws <url>".into(), NotificationKind::Warning);
                } else {
                    app.active_tab_mut().request.protocol = crate::core::request::Protocol::WebSocket;
                    app.active_tab_mut().request.url = url.to_string();
                    app.send_request().await?;
                }
            } else {
                app.notify("Usage: :ws <url>".into(), NotificationKind::Warning);
            }
        }
        "sse" => {
            if let Some(url) = args {
                let url = url.trim();
                if url.is_empty() {
                    app.notify("Usage: :sse <url>".into(), NotificationKind::Warning);
                } else {
                    app.active_tab_mut().request.protocol = crate::core::request::Protocol::Sse;
                    app.active_tab_mut().request.url = url.to_string();
                    app.send_request().await?;
                }
            } else {
                app.notify("Usage: :sse <url>".into(), NotificationKind::Warning);
            }
        }
        "ws-disconnect" => {
            if let Some(ref tx) = app.active_tab().ws_cmd_sender {
                let _ = tx.send(crate::protocols::websocket::WsCommand::Disconnect);
            }
            let tab = app.active_tab_mut();
            tab.ws_cmd_sender = None;
            if let Some(ref mut s) = tab.ws_session {
                s.status = crate::protocols::websocket::WsStatus::Disconnected;
            }
            app.notify("Disconnected".into(), NotificationKind::Info);
        }
        "sse-disconnect" => {
            if let Some(ref tx) = app.active_tab().sse_cmd_sender {
                let _ = tx.send(crate::protocols::sse::SseCommand::Disconnect);
            }
            let tab = app.active_tab_mut();
            tab.sse_cmd_sender = None;
            if let Some(ref mut s) = tab.sse_session {
                s.status = crate::protocols::sse::SseStatus::Disconnected;
            }
            app.notify("Disconnected".into(), NotificationKind::Info);
        }
        "chain" => {
            if let Some(name) = args {
                let name = name.trim();
                // Search all collections for a chain matching name
                let mut found = None;
                for (coll_idx, coll) in app.collections.iter().enumerate() {
                    if let Some(chain) = coll.chains.iter().find(|c| c.name == name) {
                        found = Some((chain.clone(), coll_idx));
                        break;
                    }
                }
                if let Some((chain, coll_idx)) = found {
                    start_chain_execution(app, chain, coll_idx);
                } else {
                    app.notify(format!("Chain '{}' not found", name), NotificationKind::Warning);
                }
            } else {
                app.notify("Usage: :chain <name>".into(), NotificationKind::Warning);
            }
        }
        "newchain" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :newchain <name>".into(), NotificationKind::Warning);
                } else if app.collections.is_empty() {
                    app.notify("No collections. Use :newcol <name> first".into(), NotificationKind::Warning);
                } else {
                    let chain = crate::core::chain::RequestChain::new(name);
                    // Add to first collection (or the collection of the active tab if any)
                    let coll_idx = app.active_tab().collection_index.unwrap_or(0);
                    let coll_idx = coll_idx.min(app.collections.len() - 1);
                    app.collections[coll_idx].chains.push(chain);
                    save_all_collections(app);
                    app.notify(
                        format!("Created chain '{}' in '{}'", name, app.collections[coll_idx].name),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify("Usage: :newchain <name>".into(), NotificationKind::Warning);
            }
        }
        "addstep" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :addstep <request_name>".into(), NotificationKind::Warning);
                } else {
                    // Find request by name across collections
                    let found_req: Option<Uuid> = app.collections.iter()
                        .flat_map(|c| c.all_requests())
                        .find(|r| r.name == name)
                        .map(|r| r.id);

                    if let Some(request_id) = found_req {
                        // Find the last chain in any collection and add step
                        let mut target: Option<(usize, usize)> = None;
                        for (ci, coll) in app.collections.iter().enumerate().rev() {
                            if !coll.chains.is_empty() {
                                target = Some((ci, coll.chains.len() - 1));
                                break;
                            }
                        }
                        if let Some((ci, chain_i)) = target {
                            app.collections[ci].chains[chain_i].add_step(request_id);
                            let chain_name = app.collections[ci].chains[chain_i].name.clone();
                            save_all_collections(app);
                            app.notify(
                                format!("Added '{}' to chain '{}'", name, chain_name),
                                NotificationKind::Success,
                            );
                        } else {
                            app.notify("No chains exist. Use :newchain <name> first".into(), NotificationKind::Warning);
                        }
                    } else {
                        app.notify(format!("Request '{}' not found", name), NotificationKind::Warning);
                    }
                }
            } else {
                app.notify("Usage: :addstep <request_name>".into(), NotificationKind::Warning);
            }
        }
        "importchain" => {
            if let Some(path) = args {
                let path_str = path.trim();
                if path_str.is_empty() {
                    app.notify("Usage: :importchain <path>".into(), NotificationKind::Warning);
                } else if app.collections.is_empty() {
                    app.notify("No collections. Use :newcol <name> first".into(), NotificationKind::Warning);
                } else {
                    execute_import_chain(app, path_str);
                }
            } else {
                app.notify("Usage: :importchain <path>".into(), NotificationKind::Warning);
            }
        }
        "proxy" => {
            app.notify("Proxy inspector not yet available".into(), NotificationKind::Warning);
        }
        "newenv" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :newenv <name>".into(), NotificationKind::Warning);
                } else {
                    let env = crate::core::environment::Environment::new(name);
                    app.environments.push(env);
                    app.active_env = Some(app.environments.len() - 1);
                    app.notify(
                        format!("Created environment '{}'", name),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify("Usage: :newenv <name>".into(), NotificationKind::Warning);
            }
        }
        "dupenv" => {
            if let Some(idx) = app.active_env {
                if let Some(source) = app.environments.get(idx).cloned() {
                    let new_name = if let Some(name) = args {
                        let name = name.trim();
                        if name.is_empty() {
                            format!("{} (copy)", source.name)
                        } else {
                            name.to_string()
                        }
                    } else {
                        format!("{} (copy)", source.name)
                    };
                    let mut dup = source;
                    dup.id = uuid::Uuid::new_v4();
                    dup.name = new_name.clone();
                    app.environments.push(dup);
                    app.active_env = Some(app.environments.len() - 1);
                    app.notify(
                        format!("Duplicated environment as '{}'", new_name),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify("No active environment to duplicate".into(), NotificationKind::Warning);
            }
        }
        "delcol" => {
            if let Some(name) = args {
                let name = name.trim();
                if let Some(pos) = app.collections.iter().position(|c| c.name == name) {
                    let coll = app.collections.remove(pos);
                    // Delete from disk
                    if let Ok(store) = crate::storage::collections_store::CollectionsStore::new(
                        app.config.collections_dir.clone(),
                    ) {
                        let _ = store.delete_collection(&coll);
                    }
                    // Fix tab collection_index references
                    for tab in &mut app.tabs {
                        match tab.collection_index {
                            Some(ci) if ci == pos => tab.collection_index = None,
                            Some(ci) if ci > pos => tab.collection_index = Some(ci - 1),
                            _ => {}
                        }
                    }
                    app.notify(
                        format!("Deleted collection '{}'", name),
                        NotificationKind::Success,
                    );
                } else {
                    app.notify(
                        format!("Collection '{}' not found", name),
                        NotificationKind::Error,
                    );
                }
            } else {
                // Try to delete the collection selected in sidebar
                if app.sidebar_state.section == SidebarSection::Collections {
                    let items = build_sidebar_items(app);
                    if let Some(SidebarItem::Collection { coll_idx, .. }) =
                        items.get(app.sidebar_state.selected)
                    {
                        let coll_idx = *coll_idx;
                        let coll = app.collections.remove(coll_idx);
                        if let Ok(store) = crate::storage::collections_store::CollectionsStore::new(
                            app.config.collections_dir.clone(),
                        ) {
                            let _ = store.delete_collection(&coll);
                        }
                        for tab in &mut app.tabs {
                            match tab.collection_index {
                                Some(ci) if ci == coll_idx => tab.collection_index = None,
                                Some(ci) if ci > coll_idx => tab.collection_index = Some(ci - 1),
                                _ => {}
                            }
                        }
                        app.notify(
                            format!("Deleted collection '{}'", coll.name),
                            NotificationKind::Success,
                        );
                    } else {
                        app.notify(
                            "Select a collection in the sidebar or provide a name".into(),
                            NotificationKind::Warning,
                        );
                    }
                } else {
                    app.notify(
                        "Usage: :delcol <name> or select a collection in the sidebar".into(),
                        NotificationKind::Warning,
                    );
                }
            }
        }
        "addvar" => {
            if let Some(rest) = args {
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    app.notify("Usage: :addvar <key> <value>".into(), NotificationKind::Warning);
                } else {
                    let key = parts[0].trim();
                    let val = parts[1].trim();
                    // Add to the active tab's collection
                    if let Some(coll_idx) = app.active_tab().collection_index {
                        if let Some(coll) = app.collections.get_mut(coll_idx) {
                            coll.variables.push(
                                crate::core::request::KeyValuePair::new(key, val),
                            );
                            save_all_collections(app);
                            app.notify(
                                format!("Added variable '{}' = '{}'", key, val),
                                NotificationKind::Success,
                            );
                        }
                    } else {
                        app.notify(
                            "No collection selected. Save to a collection first".into(),
                            NotificationKind::Warning,
                        );
                    }
                }
            } else {
                app.notify("Usage: :addvar <key> <value>".into(), NotificationKind::Warning);
            }
        }
        "clearhistory" => {
            app.history.clear();
            app.notify("History cleared".into(), NotificationKind::Success);
        }
        "rename" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :rename <name>".into(), NotificationKind::Warning);
                } else {
                    app.active_tab_mut().request.name = name.to_string();
                    app.active_tab_mut().dirty = true;
                    app.notify(
                        format!("Renamed to '{}'", name),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify("Usage: :rename <name>".into(), NotificationKind::Warning);
            }
        }
        _ => {
            app.notify(format!("Unknown command: {}", command), NotificationKind::Error);
        }
    }
    Ok(())
}

/// Import a file by path. Auto-detects format from extension.
fn execute_import(app: &mut App, path_str: &str) {
    let path = std::path::Path::new(path_str);

    // Expand ~ to home directory
    let expanded = if path_str.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            home.join(path_str.trim_start_matches("~/"))
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    let content = match std::fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(e) => {
            app.notify(format!("Cannot read file: {}", e), NotificationKind::Error);
            return;
        }
    };

    let ext = expanded
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        // Postman collection or environment JSON
        "json" => {
            // Try Postman collection first, then environment
            if let Ok(collection) = crate::postman::import::import_postman_collection(&content) {
                let name = collection.name.clone();
                let count = collection.request_count();
                app.collections.push(collection);
                app.notify(
                    format!("Imported collection '{}' ({} requests)", name, count),
                    NotificationKind::Success,
                );
            } else if let Ok(env) = crate::postman::env_import::import_postman_environment(&content) {
                let name = env.name.clone();
                app.environments.push(env);
                app.notify(
                    format!("Imported environment '{}'", name),
                    NotificationKind::Success,
                );
            } else {
                app.notify("Failed to parse JSON (not a Postman collection or environment)".into(), NotificationKind::Error);
            }
        }
        // HAR (HTTP Archive)
        "har" => {
            match crate::importers::har::import_har(&content) {
                Ok(collection) => {
                    let count = collection.request_count();
                    app.collections.push(collection);
                    app.notify(
                        format!("Imported HAR archive ({} requests)", count),
                        NotificationKind::Success,
                    );
                }
                Err(e) => {
                    app.notify(format!("Failed to parse HAR: {}", e), NotificationKind::Error);
                }
            }
        }
        // Chain YAML or OpenAPI / Swagger
        "yaml" | "yml" => {
            if crate::importers::chain::looks_like_chain(&content) && !app.collections.is_empty() {
                let coll_idx = app.active_tab().collection_index.unwrap_or(0);
                let coll_idx = coll_idx.min(app.collections.len() - 1);
                match crate::importers::chain::import_chain(&content, &app.collections[coll_idx]) {
                    Ok(chain) => {
                        let chain_name = chain.name.clone();
                        let step_count = chain.steps.len();
                        let coll_name = app.collections[coll_idx].name.clone();
                        app.collections[coll_idx].chains.push(chain);
                        app.notify(
                            format!("Imported chain '{}' ({} steps) into '{}'", chain_name, step_count, coll_name),
                            NotificationKind::Success,
                        );
                    }
                    Err(_) => {
                        // Fall through to OpenAPI
                        match crate::importers::openapi::import_openapi(&content) {
                            Ok(collection) => {
                                let name = collection.name.clone();
                                let count = collection.request_count();
                                app.collections.push(collection);
                                app.notify(
                                    format!("Imported OpenAPI '{}' ({} requests)", name, count),
                                    NotificationKind::Success,
                                );
                            }
                            Err(e) => {
                                app.notify(format!("Failed to parse YAML: {}", e), NotificationKind::Error);
                            }
                        }
                    }
                }
            } else {
                match crate::importers::openapi::import_openapi(&content) {
                    Ok(collection) => {
                        let name = collection.name.clone();
                        let count = collection.request_count();
                        app.collections.push(collection);
                        app.notify(
                            format!("Imported OpenAPI '{}' ({} requests)", name, count),
                            NotificationKind::Success,
                        );
                    }
                    Err(e) => {
                        app.notify(format!("Failed to parse OpenAPI: {}", e), NotificationKind::Error);
                    }
                }
            }
        }
        // .env files
        "env" => {
            match crate::importers::dotenv::dotenv_to_environment(
                expanded.file_name().and_then(|n| n.to_str()).unwrap_or(".env"),
                &content,
            ) {
                Ok(env) => {
                    let name = env.name.clone();
                    app.environments.push(env);
                    if app.active_env.is_none() {
                        app.active_env = Some(app.environments.len() - 1);
                    }
                    app.notify(
                        format!("Loaded environment '{}'", name),
                        NotificationKind::Success,
                    );
                }
                Err(e) => {
                    app.notify(format!("Failed to parse .env: {}", e), NotificationKind::Error);
                }
            }
        }
        // Try cURL (any other extension or no extension)
        _ => {
            match crate::importers::curl::parse_curl(&content) {
                Ok(request) => {
                    app.tabs.push(crate::app::RequestTab::from_request(request, None));
                    app.active_tab = app.tabs.len() - 1;
                    app.notify("Imported from cURL".into(), NotificationKind::Success);
                }
                Err(e) => {
                    app.notify(
                        format!("Unknown format. cURL parse failed: {}", e),
                        NotificationKind::Error,
                    );
                }
            }
        }
    }

    // Persist all collections to disk after a successful import.
    save_all_collections(app);
}

/// Read a YAML file and import it as a chain into the active collection.
fn execute_import_chain(app: &mut App, path_str: &str) {
    let path = std::path::Path::new(path_str);
    let expanded = if path_str.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            home.join(path_str.trim_start_matches("~/"))
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    let content = match std::fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(e) => {
            app.notify(format!("Cannot read file: {}", e), NotificationKind::Error);
            return;
        }
    };

    let coll_idx = app.active_tab().collection_index.unwrap_or(0);
    let coll_idx = coll_idx.min(app.collections.len() - 1);

    match crate::importers::chain::import_chain(&content, &app.collections[coll_idx]) {
        Ok(chain) => {
            let chain_name = chain.name.clone();
            let step_count = chain.steps.len();
            let coll_name = app.collections[coll_idx].name.clone();
            app.collections[coll_idx].chains.push(chain);
            save_all_collections(app);
            app.notify(
                format!("Imported chain '{}' ({} steps) into '{}'", chain_name, step_count, coll_name),
                NotificationKind::Success,
            );
        }
        Err(e) => {
            app.notify(format!("Failed to import chain: {}", e), NotificationKind::Error);
        }
    }
}

/// Saves all collections in memory to the collections directory.
fn save_all_collections(app: &App) {
    if let Ok(store) = crate::storage::collections_store::CollectionsStore::new(
        app.config.collections_dir.clone(),
    ) {
        for coll in &app.collections {
            if let Err(e) = store.save_collection(coll) {
                tracing::warn!("Failed to save collection '{}': {}", coll.name, e);
            }
        }
    }
}

/// Recursively removes a request with the given ID from a collection item tree.
fn remove_request_from_collection(
    items: &mut Vec<crate::core::collection::CollectionItem>,
    request_id: &uuid::Uuid,
) -> bool {
    if let Some(pos) = items.iter().position(|item| {
        matches!(item, crate::core::collection::CollectionItem::Request(r) if r.id == *request_id)
    }) {
        items.remove(pos);
        return true;
    }
    for item in items.iter_mut() {
        if let crate::core::collection::CollectionItem::Folder { items: sub, .. } = item {
            if remove_request_from_collection(sub, request_id) {
                return true;
            }
        }
    }
    false
}

/// Export to a file. Format is auto-detected from extension.
///
/// Supported extensions:
///   .json  → Postman collection
///   .md    → Markdown documentation
///   .sh / .curl / .txt → cURL command(s)
///   .env   → Environment variables
fn execute_export(app: &mut App, path_str: &str) {
    let path = std::path::Path::new(path_str);

    let expanded = if path_str.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            home.join(path_str.trim_start_matches("~/"))
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    let ext = expanded
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let result: Result<String, String> = match ext.as_str() {
        "json" => {
            // Export active collection as Postman, or current request if no collection
            if let Some(coll_idx) = app.active_tab().collection_index {
                if let Some(coll) = app.collections.get(coll_idx) {
                    crate::postman::export::export_postman_collection(coll)
                        .map_err(|e| e.to_string())
                } else {
                    Err("Collection not found".into())
                }
            } else if !app.collections.is_empty() {
                // Export the first collection
                crate::postman::export::export_postman_collection(&app.collections[0])
                    .map_err(|e| e.to_string())
            } else {
                Err("No collection to export".into())
            }
        }
        "md" | "markdown" => {
            if let Some(coll_idx) = app.active_tab().collection_index {
                if let Some(coll) = app.collections.get(coll_idx) {
                    Ok(crate::exporters::markdown_docs::generate_docs(coll))
                } else {
                    Err("Collection not found".into())
                }
            } else if !app.collections.is_empty() {
                Ok(crate::exporters::markdown_docs::generate_docs(&app.collections[0]))
            } else {
                Err("No collection to export".into())
            }
        }
        "sh" | "curl" | "txt" => {
            let resolver = app.build_resolver();
            let curl = crate::exporters::curl::to_curl(&app.active_tab().request, &resolver);
            Ok(curl)
        }
        "env" => {
            if let Some(env) = app.active_environment() {
                crate::postman::env_export::export_postman_environment(env)
                    .map_err(|e| e.to_string())
            } else {
                Err("No active environment to export".into())
            }
        }
        _ => {
            Err(format!(
                "Unknown export format '.{}'. Use .json, .md, .sh, .curl, or .env",
                ext
            ))
        }
    };

    match result {
        Ok(content) => match std::fs::write(&expanded, &content) {
            Ok(_) => {
                app.notify(
                    format!("Exported to {}", expanded.display()),
                    NotificationKind::Success,
                );
            }
            Err(e) => {
                app.notify(format!("Failed to write file: {}", e), NotificationKind::Error);
            }
        },
        Err(e) => {
            app.notify(e, NotificationKind::Error);
        }
    }
}
