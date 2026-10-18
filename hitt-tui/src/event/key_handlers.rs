use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind, RequestTabKind, ResponseTabKind,
    SidebarSection,
};
use hitt_core::core::constants::HALF_PAGE_SCROLL;

use super::actions::{
    cycle_environment, disconnect_sse, disconnect_ws, request_close_tab,
    request_delete_selected_item, request_quit, save_active_request, sse_is_active, ws_is_active,
};
use super::editors::{handle_request_editor_key, open_env_editor};
use super::insert_mode::enter_insert_mode;
use super::is_text_input;
use super::navigation::{
    cycle_sidebar_section, handle_nav_down, handle_nav_left, handle_nav_right, handle_nav_up,
    scroll_focused_by,
};
use super::persistence::{request_name_taken, save_all_collections};
use super::protocols::cycle_protocol_method;
use super::sidebar::{build_sidebar_items, handle_sidebar_action, sidebar_len, SidebarItem};

#[allow(clippy::too_many_lines)]
pub(super) async fn handle_normal_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    // ── Response filter input (intercepts keys while filter bar is active) ──
    if app.response_filter_active {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                app.response_filter_active = false;
                app.active_tab_mut().response_filter.clear();
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                // Confirm filter and exit filter input mode.
                // Keep the filter text visible, navigate matches with n/N.
                app.response_filter_active = false;
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                let tab = app.active_tab_mut();
                tab.response_filter.pop();
                tab.response_filter_match_idx = 0;
            }
            (m, KeyCode::Char(c)) if is_text_input(m) => {
                let tab = app.active_tab_mut();
                tab.response_filter.push(c);
                tab.response_filter_match_idx = 0;
            }
            _ => {}
        }
        return Ok(());
    }

    // ── Request sub-tab editors (Panel focus on the request area) ──
    // Their table keys (a, x/d, Space, Enter, t, f...) shadow the global
    // bindings only here; anything they don't use falls through.
    if app.focus == FocusArea::RequestBody
        && app.nav_mode == NavMode::Panel
        && handle_request_editor_key(app, key)
    {
        return Ok(());
    }

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

        // ── Esc: Cancel in-flight request, clear response filter, or Panel → Global ──
        (KeyModifiers::NONE, KeyCode::Esc) => {
            if app.cancel_active_request() {
                // Request cancelled; nothing else to do.
            } else if app.focus == FocusArea::ResponseBody
                && !app.active_tab().response_filter.is_empty()
            {
                let tab = app.active_tab_mut();
                tab.response_filter.clear();
                tab.response_filter_match_idx = 0;
            } else if app.nav_mode == NavMode::Panel {
                app.nav_mode = NavMode::Global;
                app.snap_focus_to_major_panel();
            }
        }

        // ── Search / Response filter ────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('/')) => {
            if app.focus == FocusArea::ResponseBody {
                let tab = app.active_tab_mut();
                tab.response_filter.clear();
                tab.response_filter_match_idx = 0;
                app.response_filter_active = true;
            } else {
                open_search_modal(app);
            }
        }
        (KeyModifiers::NONE | KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            open_search_modal(app);
        }

        // ── Environment cycle (e or Ctrl+E) ─────────────────────────
        (KeyModifiers::NONE | KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            cycle_environment(app);
        }

        // ── Environment variables editor (E) ────────────────────────
        (KeyModifiers::SHIFT, KeyCode::Char('E')) => {
            open_env_editor(app, crate::editor::VarScope::Environment);
        }

        // ── New tab ──────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('t')) | (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            app.new_tab();
        }

        // ── Close tab (asks first if it has unsaved changes) ─────────
        (KeyModifiers::NONE, KeyCode::Char('w')) => {
            request_close_tab(app);
        }

        // ── Next match (filter) or next tab ──────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('n')) => {
            if app.focus == FocusArea::ResponseBody && !app.active_tab().response_filter.is_empty()
            {
                let tab = app.active_tab_mut();
                tab.response_filter_match_idx = tab.response_filter_match_idx.saturating_add(1);
            } else {
                // Tab switches reset the scroll offsets (see `App::switch_tab`).
                app.next_tab();
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('b')) => {
            app.prev_tab();
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

        // ── Enter insert mode (or import, on the sidebar) ────────────
        (KeyModifiers::NONE, KeyCode::Char('i')) => {
            enter_insert_mode(app);
        }

        // ── Sidebar: switch section (Collections / Chains / History) ─
        (KeyModifiers::NONE, KeyCode::Char('[')) if app.focus == FocusArea::Sidebar => {
            cycle_sidebar_section(app, false);
        }
        (KeyModifiers::NONE, KeyCode::Char(']')) if app.focus == FocusArea::Sidebar => {
            cycle_sidebar_section(app, true);
        }

        // ── Command mode ─────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char(':')) => {
            app.mode = AppMode::Command;
            app.command_input.clear();
            app.command_history_idx = None;
        }

        // ── Help ─────────────────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            app.help_scroll = 0;
            app.mode = AppMode::Modal(ModalKind::Help);
        }

        // ── Quit / Disconnect ────────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('q')) => {
            // Protocol-aware: disconnect a live WS/SSE session when on the
            // response area; otherwise `q` behaves as usual.
            if app.focus == FocusArea::ResponseBody {
                if ws_is_active(app) {
                    disconnect_ws(app);
                    return Ok(());
                }
                if sse_is_active(app) {
                    disconnect_sse(app);
                    return Ok(());
                }
            }
            if app.nav_mode == NavMode::Global {
                request_quit(app);
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
            if let hitt_core::core::request::Protocol::Sse = app.active_tab().request.protocol {
                // The visible tab is the source of truth (`2` / clicks change it too).
                let tab = app.active_tab_mut();
                let show_stream = tab.response_tab != ResponseTabKind::SseStream;
                tab.sse_show_accumulated = show_stream;
                tab.response_tab = if show_stream {
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
                if let Some(
                    SidebarItem::Collection { coll_idx, .. }
                    | SidebarItem::Folder { coll_idx, .. }
                    | SidebarItem::Request { coll_idx, .. },
                ) = items.get(app.sidebar_state.selected)
                {
                    add_new_request(app, *coll_idx);
                }
            }
        }

        // ── Sidebar: run collection / folder (collection runner) ────
        (KeyModifiers::SHIFT, KeyCode::Char('R')) if app.focus == FocusArea::Sidebar => {
            super::runner::run_sidebar_selection(app).await;
        }

        // ── Sidebar: delete request / chain (after confirmation) ─────
        (KeyModifiers::NONE, KeyCode::Char('x')) if app.focus == FocusArea::Sidebar => {
            request_delete_selected_item(app);
        }

        // ── Sidebar History: r / Ctrl+R replay the selected entry ──
        (KeyModifiers::NONE | KeyModifiers::CONTROL, KeyCode::Char('r'))
            if app.focus == FocusArea::Sidebar
                && app.sidebar_state.section == SidebarSection::History =>
        {
            app.replay_history_entry(app.sidebar_state.selected).await?;
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

        // ── Page scroll (J / Ctrl+D = half page down, K / Ctrl+U = half page up) ──
        (KeyModifiers::SHIFT, KeyCode::Char('J')) | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
            scroll_focused_by(app, HALF_PAGE_SCROLL, true);
        }
        (KeyModifiers::SHIFT, KeyCode::Char('K')) | (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            scroll_focused_by(app, HALF_PAGE_SCROLL, false);
        }

        // ── Top/bottom of scroll ─────────────────────────────────────
        (KeyModifiers::NONE, KeyCode::Char('g')) => match app.focus {
            FocusArea::ResponseBody => {
                if !super::protocols::scroll_stream_list_edge(app, false) {
                    app.response_scroll = 0;
                }
            }
            FocusArea::RequestBody => app.request_scroll = 0,
            FocusArea::Sidebar => app.sidebar_state.selected = 0,
            _ => {}
        },
        (KeyModifiers::SHIFT, KeyCode::Char('G')) => match app.focus {
            // Rendering clamps these to the last page of content.
            FocusArea::ResponseBody => {
                if !super::protocols::scroll_stream_list_edge(app, true) {
                    app.response_scroll = usize::MAX;
                }
            }
            FocusArea::RequestBody => app.request_scroll = usize::MAX,
            FocusArea::Sidebar => {
                app.sidebar_state.selected = sidebar_len(app).saturating_sub(1);
            }
            _ => {}
        },

        // ── Previous filter match ────────────────────────────────────
        (KeyModifiers::SHIFT, KeyCode::Char('N')) => {
            if app.focus == FocusArea::ResponseBody && !app.active_tab().response_filter.is_empty()
            {
                let tab = app.active_tab_mut();
                tab.response_filter_match_idx = tab.response_filter_match_idx.saturating_sub(1);
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
                        app.request_scroll = 0;
                    }
                }
                FocusArea::ResponseTabs | FocusArea::ResponseBody => {
                    let protocol = app.active_tab().request.protocol.clone();
                    let tabs = ResponseTabKind::for_protocol(&protocol);
                    if idx < tabs.len() {
                        let tab = app.active_tab_mut();
                        tab.response_tab = tabs[idx];
                        tab.sse_show_accumulated = tabs[idx] == ResponseTabKind::SseStream;
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

        // ── Ctrl+O: Import modal ────────────────────────────────────
        // (Ctrl+I is indistinguishable from Tab in most terminals.)
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
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

/// Open the request search modal with an empty query.
fn open_search_modal(app: &mut App) {
    app.mode = AppMode::Modal(ModalKind::Search);
    app.search_query.clear();
    app.search_results.clear();
    app.search_selected = 0;
}

/// Sidebar `a`: add an empty request to collection `coll_idx` (named "New
/// Request", "New Request 2"… so names stay unique) and open it in a tab.
fn add_new_request(app: &mut App, coll_idx: usize) {
    const BASE: &str = "New Request";
    let name = std::iter::once(BASE.to_string())
        .chain((2..=10_000).map(|n| format!("{BASE} {n}")))
        .find(|n| !request_name_taken(app, coll_idx, n, None))
        .unwrap_or_else(|| BASE.to_string());
    let req =
        hitt_core::core::request::Request::new(name, hitt_core::core::request::HttpMethod::GET, "");
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
