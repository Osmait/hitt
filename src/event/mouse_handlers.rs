use anyhow::Result;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{App, AppMode, FocusArea, NavMode, NotificationKind};
use crate::core::constants::SCROLL_DELTA;

use super::protocols::cycle_protocol_method;
use super::sidebar::handle_sidebar_action;

pub(super) async fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Result<()> {
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

pub(super) fn hit_test(rect: Option<Rect>, col: u16, row: u16) -> bool {
    match rect {
        Some(r) => col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height,
        None => false,
    }
}

pub(super) fn hit_test_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

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
            app.notify(format!("Environment: {name}"), NotificationKind::Info);
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
                        let tab = crate::app::RequestTab::from_request(req.clone(), Some(coll_idx));
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
        let delta: usize = SCROLL_DELTA;
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
