use uuid::Uuid;

use crate::app::{App, NotificationKind, ResponseTabKind};
use hitt_core::core::request::HttpMethod;
use hitt_core::protocols::sse::SseStatus;
use hitt_core::protocols::websocket::WsStatus;

use super::{SseEventData, WsEventData};

pub fn handle_ws_protocol_event(app: &mut App, session_id: Uuid, event: WsEventData) {
    // Find the tab that owns this session
    let Some(tab_idx) = app
        .tabs
        .iter()
        .position(|t| t.ws_session.as_ref().map(|s| s.id) == Some(session_id))
    else {
        return;
    };
    let tab = &mut app.tabs[tab_idx];
    let Some(session) = tab.ws_session.as_mut() else {
        return;
    };
    // The user already disconnected / cancelled this session: a handshake
    // outcome that was in flight at that moment is stale.
    if session.status == WsStatus::Disconnected && !matches!(event, WsEventData::MessageReceived(_))
    {
        return;
    }
    let was_connecting = session.status == WsStatus::Connecting;
    match event {
        WsEventData::Connected => {
            session.status = WsStatus::Connected {
                connected_at: chrono::Utc::now(),
            };
            app.notify("WebSocket connected".into(), NotificationKind::Success);
        }
        WsEventData::Disconnected => {
            session.status = WsStatus::Disconnected;
            tab.ws_cmd_sender = None;
        }
        WsEventData::MessageReceived(msg) => {
            // The view follows new messages while `ws_message_scroll.follow` is set.
            session.push_message(msg);
        }
        WsEventData::Error(e) => {
            session.status = WsStatus::Error(e.clone());
            tab.ws_cmd_sender = None;
            if was_connecting {
                app.notify(
                    format!("WebSocket connect failed: {e}"),
                    NotificationKind::Error,
                );
            }
        }
    }
}

pub fn handle_sse_protocol_event(app: &mut App, session_id: Uuid, event: SseEventData) {
    // Find the tab that owns this session
    let Some(tab_idx) = app
        .tabs
        .iter()
        .position(|t| t.sse_session.as_ref().map(|s| s.id) == Some(session_id))
    else {
        return;
    };
    let tab = &mut app.tabs[tab_idx];
    let Some(session) = tab.sse_session.as_mut() else {
        return;
    };
    // The user already disconnected / cancelled this session: a handshake
    // outcome that was in flight at that moment is stale.
    if session.status == SseStatus::Disconnected && !matches!(event, SseEventData::Event(_)) {
        return;
    }
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
            // The view follows new events while `sse_event_scroll.follow` is set.
            session.push_event(evt);
        }
        SseEventData::Error(e) => {
            session.status = SseStatus::Error(e);
            tab.sse_cmd_sender = None;
        }
    }
}

/// `m`: next HTTP method, then WebSocket, SSE and back to GET. Marks the
/// tab dirty (the method / protocol is saved with the request).
pub(super) fn cycle_protocol_method(tab: &mut crate::app::RequestTab) {
    use hitt_core::core::request::Protocol;
    if !matches!(tab.request.protocol, Protocol::Grpc { .. }) {
        tab.dirty = true;
    }
    match tab.request.protocol {
        Protocol::Http => {
            let methods = HttpMethod::all();
            let idx = methods
                .iter()
                .position(|m| *m == tab.request.method)
                .unwrap_or(0);
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
        Protocol::Grpc { .. } => {}
    }
}

/// The active tab's live list scroll (WS messages / SSE events) and its
/// length, or `None` when the active tab is not a WebSocket/SSE tab.
fn stream_scroll(app: &mut App) -> Option<(&mut crate::app::ListScroll, usize)> {
    use hitt_core::core::request::Protocol;
    let tab = app.active_tab_mut();
    match tab.request.protocol {
        Protocol::WebSocket => {
            let len = tab.ws_session.as_ref().map_or(0, |s| s.messages.len());
            Some((&mut tab.ws_message_scroll, len))
        }
        Protocol::Sse => {
            // The accumulated-text view scrolls wrapped rows, counted by the
            // renderer on the last frame (it clamps again on the next one).
            let len = if tab.response_tab == ResponseTabKind::SseStream {
                tab.sse_stream_rows
            } else {
                tab.sse_session.as_ref().map_or(0, |s| s.events.len())
            };
            Some((&mut tab.sse_event_scroll, len))
        }
        _ => None,
    }
}

/// Scroll the active WS/SSE list by `lines`. Returns false if the active tab
/// is not a WebSocket/SSE tab (the caller scrolls the HTTP response instead).
pub(super) fn scroll_stream_list(app: &mut App, lines: usize, down: bool) -> bool {
    let Some((scroll, len)) = stream_scroll(app) else {
        return false;
    };
    if down {
        scroll.down(lines, len);
    } else {
        scroll.up(lines, len);
    }
    true
}

/// Jump the active WS/SSE list to its top or bottom (bottom re-enables
/// auto-follow). Returns false if the active tab is not a WebSocket/SSE tab.
pub(super) fn scroll_stream_list_edge(app: &mut App, bottom: bool) -> bool {
    let Some((scroll, len)) = stream_scroll(app) else {
        return false;
    };
    if bottom {
        scroll.bottom(len);
    } else {
        scroll.top();
    }
    true
}
