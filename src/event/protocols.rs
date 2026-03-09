use uuid::Uuid;

use crate::app::{App, ResponseTabKind};
use crate::core::request::HttpMethod;
use crate::protocols::sse::SseStatus;
use crate::protocols::websocket::WsStatus;

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

pub(super) fn cycle_protocol_method(tab: &mut crate::app::RequestTab) {
    use crate::core::request::Protocol;
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
