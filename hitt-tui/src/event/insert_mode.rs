use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, FocusArea, ModalKind, NavMode};
use crate::editor::TextInput;

use super::editors::{begin_request_edit, edit_url, handle_request_editor_insert};
use super::is_text_input;

/// Handle `i` in normal mode: start editing the focused input.
///
/// Areas without an editor of their own fall back to the URL bar. Add an arm
/// here when a panel gets an inline editor.
pub(super) fn enter_insert_mode(app: &mut App) {
    let is_ws = matches!(
        app.active_tab().request.protocol,
        hitt_core::core::request::Protocol::WebSocket
    );
    match app.focus {
        // Nothing to type into on the sidebar: `i` opens the import dialog.
        FocusArea::Sidebar => {
            app.modal_input.clear();
            app.mode = AppMode::Modal(ModalKind::Import);
            return;
        }
        // Params / headers / body / auth / assertions editors open their own
        // form or textarea (and switch to insert mode themselves).
        FocusArea::RequestBody => {
            app.nav_mode = NavMode::Panel;
            begin_request_edit(app);
            return;
        }
        FocusArea::UrlBar => {}
        FocusArea::ResponseBody if is_ws => {}
        _ => app.navigate_to_panel(FocusArea::UrlBar),
    }
    // Start typing at the end of the URL.
    app.editor.url_cursor = None;
    app.nav_mode = NavMode::Panel;
    app.mode = AppMode::Insert;
}

pub(super) async fn handle_insert_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    // An open request editor (field form / body textarea) takes every key.
    if handle_request_editor_insert(app, key) {
        return Ok(());
    }

    // WS message input when focused on ResponseBody with WebSocket protocol
    if app.focus == FocusArea::ResponseBody {
        if let hitt_core::core::request::Protocol::WebSocket = app.active_tab().request.protocol {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    app.mode = AppMode::Normal;
                    app.nav_mode = NavMode::Panel;
                }
                (m, KeyCode::Char(c)) if is_text_input(m) => {
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
                            session
                                .messages
                                .push(hitt_core::protocols::websocket::WsMessage {
                                    direction:
                                        hitt_core::protocols::websocket::MessageDirection::Sent,
                                    content: hitt_core::protocols::websocket::WsContent::Text(
                                        text.clone(),
                                    ),
                                    timestamp: chrono::Utc::now(),
                                });
                            tab.ws_message_scroll.bottom(session.messages.len());
                        }
                        // Send via cmd channel
                        if let Some(ref tx) = tab.ws_cmd_sender {
                            let _ =
                                tx.send(hitt_core::protocols::websocket::WsCommand::SendText(text));
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

    let on_url = app.focus == FocusArea::UrlBar;
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.mode = AppMode::Normal;
            app.nav_mode = NavMode::Panel;
            app.editor.url_cursor = None;
            app.editor.url_scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if on_url {
                app.mode = AppMode::Normal;
                app.editor.url_cursor = None;
                app.send_request().await?;
            }
        }
        // Send without leaving the URL (same binding as in normal mode).
        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
            if on_url {
                app.send_request().await?;
            }
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            app.mode = AppMode::Normal;
            app.editor.url_cursor = None;
            app.cycle_focus_forward();
        }
        _ if on_url => url_key(app, key),
        _ => {}
    }
    Ok(())
}

/// Line-editing keys for the URL bar, applied at the URL cursor.
fn url_key(app: &mut App, key: KeyEvent) {
    match (key.modifiers, key.code) {
        (m, KeyCode::Char(c)) if is_text_input(m) => edit_url(app, |u| u.insert_char(c)),
        (_, KeyCode::Backspace) => edit_url(app, TextInput::backspace),
        (_, KeyCode::Delete) => edit_url(app, TextInput::delete),
        (_, KeyCode::Left) => edit_url(app, TextInput::left),
        (_, KeyCode::Right) => edit_url(app, TextInput::right),
        (_, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            edit_url(app, TextInput::home);
        }
        (_, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            edit_url(app, TextInput::end);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => edit_url(app, TextInput::clear_to_start),
        _ => {}
    }
}
