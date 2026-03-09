use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppMode, FocusArea, RequestTabKind, ResponseTabKind, SidebarSection};

use super::sidebar::{build_sidebar_items, handle_sidebar_action, handle_sidebar_collapse};

pub(super) async fn handle_chain_editor_mode(app: &mut App, key: KeyEvent) -> Result<()> {
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

pub(super) async fn handle_proxy_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    if key.code == KeyCode::Esc {
        app.mode = AppMode::Normal;
    }
    Ok(())
}

pub(super) fn handle_nav_down(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            let max = match app.sidebar_state.section {
                SidebarSection::Collections => build_sidebar_items(app).len().saturating_sub(1),
                SidebarSection::History => app.history.entries().len().saturating_sub(1),
                SidebarSection::Chains => app
                    .collections
                    .iter()
                    .map(|c| c.chains.len())
                    .sum::<usize>()
                    .saturating_sub(1),
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

pub(super) fn handle_nav_up(app: &mut App) {
    match app.focus {
        FocusArea::Sidebar => {
            app.sidebar_state.selected = app.sidebar_state.selected.saturating_sub(1);
        }
        FocusArea::RequestBody => {
            app.response_scroll = app.response_scroll.saturating_sub(1);
        }
        FocusArea::ResponseBody => match app.active_tab().request.protocol {
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
        },
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

pub(super) fn handle_nav_right(app: &mut App) {
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
