use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind, RequestTabKind, ResponseTabKind,
    SidebarSection,
};
use crate::core::constants::HALF_PAGE_SCROLL;

use super::actions::{
    cycle_environment, delete_selected_request, disconnect_sse, disconnect_ws, save_active_request,
};
use super::navigation::{handle_nav_down, handle_nav_left, handle_nav_right, handle_nav_up};
use super::persistence::save_all_collections;
use super::protocols::cycle_protocol_method;
use super::sidebar::{build_sidebar_items, handle_sidebar_action, SidebarItem};

#[allow(clippy::too_many_lines)]
pub(super) async fn handle_normal_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        // ── Enter: Global → Panel mode; Panel → Send/Sidebar action ──
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if app.nav_mode == NavMode::Global {
                app.nav_mode = NavMode::Panel;
            } else if app.focus == FocusArea::Sidebar {
                handle_sidebar_action(app);
            } else {
                app.send_request().await?;
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
        (KeyModifiers::NONE, KeyCode::Char('/' | 'p'))
        | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.mode = AppMode::Modal(ModalKind::Search);
            app.search_query.clear();
            app.search_results.clear();
        }

        // ── Environment cycle (e or Ctrl+E) ─────────────────────────
        (KeyModifiers::NONE | KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            cycle_environment(app);
        }

        // ── New tab ──────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('t')) | (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
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
        (KeyModifiers::NONE, KeyCode::Char('j') | KeyCode::Down) => {
            if app.nav_mode == NavMode::Global {
                app.global_nav_down();
            } else {
                handle_nav_down(app);
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('k') | KeyCode::Up) => {
            if app.nav_mode == NavMode::Global {
                app.global_nav_up();
            } else {
                handle_nav_up(app);
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('h') | KeyCode::Left) => {
            if app.nav_mode == NavMode::Global {
                app.global_nav_left();
            } else {
                handle_nav_left(app);
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('l') | KeyCode::Right) => {
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
                if let crate::core::request::Protocol::WebSocket = app.active_tab().request.protocol
                {
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
                        disconnect_ws(app);
                        return Ok(());
                    }
                    crate::core::request::Protocol::Sse => {
                        disconnect_sse(app);
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

        // ── Save (s or Ctrl+S) ──────────────────────────────────────
        (KeyModifiers::NONE | KeyModifiers::CONTROL, KeyCode::Char('s')) => {
            save_active_request(app);
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
                            app.notify(
                                format!("New request in '{name}'"),
                                NotificationKind::Success,
                            );
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
                            app.notify(
                                format!("New request in '{name}'"),
                                NotificationKind::Success,
                            );
                        }
                    }
                }
            }
        }

        // ── Sidebar: delete request ─────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('x')) if app.focus == FocusArea::Sidebar => {
            delete_selected_request(app);
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
                        SidebarItem::Request {
                            coll_idx,
                            request_id,
                        } => {
                            let coll_idx = *coll_idx;
                            let request_id = *request_id;
                            if let Some(coll) = app.collections.get(coll_idx) {
                                if let Some(req) = coll.find_request(&request_id) {
                                    app.rename_input = req.name.clone();
                                    app.mode = AppMode::Modal(ModalKind::RenameRequest {
                                        coll_idx,
                                        request_id,
                                    });
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
                app.response_scroll = app.response_scroll.saturating_add(HALF_PAGE_SCROLL);
            }
        }
        (KeyModifiers::SHIFT, KeyCode::Char('K')) => {
            if app.focus == FocusArea::ResponseBody || app.focus == FocusArea::RequestBody {
                app.response_scroll = app.response_scroll.saturating_sub(HALF_PAGE_SCROLL);
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
