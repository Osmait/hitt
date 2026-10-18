use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppMode, FocusArea, RequestTabKind, ResponseTabKind, SidebarSection};

use super::chain::cancel_chain_run;
use super::sidebar::{handle_sidebar_collapse, handle_sidebar_expand, sidebar_len};

pub(super) async fn handle_chain_editor_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            // First press stops a running chain (keeping its results), the
            // next one closes the runner.
            if cancel_chain_run(app) {
                return Ok(());
            }
            app.mode = AppMode::Normal;
            app.active_chain = None;
            app.active_chain_def = None;
            app.active_chain_coll_idx = None;
            app.chain_scroll = 0;
            // The chain steps panel is gone; go back to where the chain was started.
            app.focus = app.chain_return_focus;
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

pub(super) async fn handle_proxy_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    if key.code == KeyCode::Esc {
        app.mode = AppMode::Normal;
    }
    Ok(())
}

pub(super) fn handle_nav_down(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            let max = sidebar_len(app).saturating_sub(1);
            app.sidebar_state.selected = (app.sidebar_state.selected + 1).min(max);
        }
        FocusArea::RequestBody => {
            app.request_scroll = app.request_scroll.saturating_add(1);
        }
        FocusArea::ResponseBody => {
            // Protocol-aware scroll (WS messages / SSE events stop at the bottom).
            if !super::protocols::scroll_stream_list(app, 1, true) {
                app.response_scroll = app.response_scroll.saturating_add(1);
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

pub(super) fn handle_nav_up(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            app.sidebar_state.selected = app.sidebar_state.selected.saturating_sub(1);
        }
        FocusArea::RequestBody => {
            app.request_scroll = app.request_scroll.saturating_sub(1);
        }
        FocusArea::ResponseBody => {
            if !super::protocols::scroll_stream_list(app, 1, false) {
                app.response_scroll = app.response_scroll.saturating_sub(1);
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

pub(super) fn handle_nav_left(app: &mut App) {
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
                app.request_scroll = 0;
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

pub(super) fn handle_nav_right(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            handle_sidebar_expand(app);
        }
        FocusArea::RequestTabs | FocusArea::RequestBody => {
            let tabs = RequestTabKind::all();
            let current = app.active_tab().request_tab;
            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
            if idx + 1 < tabs.len() {
                app.active_tab_mut().request_tab = tabs[idx + 1];
                app.request_scroll = 0;
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

/// Scroll the focused body by `lines` (rendering clamps to the content end).
/// On the sidebar the selection moves by half the visible list instead.
pub(super) fn scroll_focused_by(app: &mut App, lines: usize, down: bool) {
    if app.focus == FocusArea::Sidebar {
        page_sidebar(app, down);
        return;
    }
    if app.focus == FocusArea::ResponseBody
        && super::protocols::scroll_stream_list(app, lines, down)
    {
        return;
    }
    let scroll = match app.focus {
        FocusArea::ResponseBody => &mut app.response_scroll,
        FocusArea::RequestBody => &mut app.request_scroll,
        _ => return,
    };
    *scroll = if down {
        scroll.saturating_add(lines)
    } else {
        scroll.saturating_sub(lines)
    };
}

/// Sidebar rows used by the panel border (2) and the section tabs (1).
const SIDEBAR_CHROME_ROWS: u16 = 3;
/// Page size before the sidebar has been drawn.
const SIDEBAR_DEFAULT_PAGE: usize = 10;

/// J / K / Ctrl+D / Ctrl+U on the sidebar: move the selection by half the
/// visible entries, clamped to the list.
fn page_sidebar(app: &mut App, down: bool) {
    let rows_per_entry = if app.sidebar_state.section == SidebarSection::History {
        crate::ui::widgets::sidebar::HISTORY_ROWS_PER_ENTRY
    } else {
        1
    };
    let page = app.regions.sidebar.map_or(SIDEBAR_DEFAULT_PAGE, |r| {
        let visible = usize::from(r.height.saturating_sub(SIDEBAR_CHROME_ROWS)) / rows_per_entry;
        (visible / 2).max(1)
    });
    let last = sidebar_len(app).saturating_sub(1);
    let state = &mut app.sidebar_state;
    state.selected = if down {
        (state.selected + page).min(last)
    } else {
        state.selected.saturating_sub(page)
    };
}

/// Switch the sidebar to the next (or previous) section, wrapping around.
pub(super) fn cycle_sidebar_section(app: &mut App, forward: bool) {
    const ORDER: [SidebarSection; 3] = [
        SidebarSection::Collections,
        SidebarSection::Chains,
        SidebarSection::History,
    ];
    let idx = ORDER
        .iter()
        .position(|s| *s == app.sidebar_state.section)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % ORDER.len()
    } else {
        (idx + ORDER.len() - 1) % ORDER.len()
    };
    app.sidebar_state.section = ORDER[next];
    app.sidebar_state.selected = 0;
    app.sidebar_state.scroll_offset = 0;
}
