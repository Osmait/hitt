use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, FocusArea, NavMode};

pub(super) async fn handle_insert_mode(app: &mut App, key: KeyEvent) -> Result<()> {
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
                            session
                                .messages
                                .push(crate::protocols::websocket::WsMessage {
                                    direction: crate::protocols::websocket::MessageDirection::Sent,
                                    content: crate::protocols::websocket::WsContent::Text(
                                        text.clone(),
                                    ),
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
        (KeyModifiers::NONE, KeyCode::Char(c)) => {
            if app.focus == FocusArea::UrlBar {
                app.active_tab_mut().request.url.push(c);
                app.active_tab_mut().dirty = true;
            }
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            if app.focus == FocusArea::UrlBar {
                app.active_tab_mut().request.url.pop();
                app.active_tab_mut().dirty = true;
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if app.focus == FocusArea::UrlBar {
                app.mode = AppMode::Normal;
                app.send_request().await?;
            }
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            app.mode = AppMode::Normal;
            app.cycle_focus_forward();
        }
        _ => {}
    }
    Ok(())
}
