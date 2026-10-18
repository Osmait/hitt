//! WebSocket and SSE session management via a polling API so UniFFI
//! doesn't need callback interfaces. Swift drives a `Timer` that calls
//! `ws_poll` / `sse_poll` at ~100ms intervals and drains pending events.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use hitt_core::core::request::KeyValuePair;
use hitt_core::protocols::sse::{SseCommand, SseOutput};
use hitt_core::protocols::websocket::{
    MessageDirection, WsCommand, WsContent, WsEvent as CoreWsEvent,
};

// ---------------------------------------------------------------------------
// FFI-serializable events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub enum WsFfiEvent {
    Opened,
    Message {
        direction: String, // "in" | "out"
        text: String,
        timestamp_ms: u64,
    },
    Binary {
        direction: String,
        size: usize,
        timestamp_ms: u64,
    },
    Error {
        message: String,
    },
    Closed,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub enum SseFfiEvent {
    Opened,
    Event {
        event_type: Option<String>,
        id: Option<String>,
        data: String,
        timestamp_ms: u64,
    },
    Error {
        message: String,
    },
    Closed,
}

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

struct WsSession {
    cmd_tx: mpsc::UnboundedSender<WsCommand>,
    events: Arc<Mutex<VecDeque<WsFfiEvent>>>,
}

struct SseSession {
    cmd_tx: mpsc::UnboundedSender<SseCommand>,
    events: Arc<Mutex<VecDeque<SseFfiEvent>>>,
}

/// Session registry held by `HittEngine`.
pub struct StreamingSessions {
    ws: Arc<Mutex<HashMap<Uuid, WsSession>>>,
    sse: Arc<Mutex<HashMap<Uuid, SseSession>>>,
}

impl Default for StreamingSessions {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingSessions {
    pub fn new() -> Self {
        Self {
            ws: Arc::new(Mutex::new(HashMap::new())),
            sse: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // ---- WebSocket ----

    pub async fn ws_connect(&self, url: &str, headers: &[KeyValuePair]) -> anyhow::Result<String> {
        let id = Uuid::new_v4();
        let events: Arc<Mutex<VecDeque<WsFfiEvent>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<CoreWsEvent>();

        let cmd_tx = hitt_core::protocols::websocket::connect(url, headers, event_tx).await?;

        let events_clone = events.clone();
        tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                let ffi_ev = match ev {
                    CoreWsEvent::Connected => WsFfiEvent::Opened,
                    CoreWsEvent::Disconnected => WsFfiEvent::Closed,
                    CoreWsEvent::Error(msg) => WsFfiEvent::Error { message: msg },
                    CoreWsEvent::MessageReceived(msg) => {
                        let ts = msg.timestamp.timestamp_millis() as u64;
                        let dir = match msg.direction {
                            MessageDirection::Sent => "out",
                            MessageDirection::Received => "in",
                        }
                        .to_string();
                        match msg.content {
                            WsContent::Text(s) => WsFfiEvent::Message {
                                direction: dir,
                                text: s,
                                timestamp_ms: ts,
                            },
                            WsContent::Binary(b) => WsFfiEvent::Binary {
                                direction: dir,
                                size: b.len(),
                                timestamp_ms: ts,
                            },
                        }
                    }
                };
                events_clone.lock().await.push_back(ffi_ev);
            }
        });

        self.ws
            .lock()
            .await
            .insert(id, WsSession { cmd_tx, events });
        Ok(id.to_string())
    }

    pub async fn ws_send(&self, session_id: &str, text: String) -> anyhow::Result<()> {
        let id = Uuid::parse_str(session_id)?;
        let sessions = self.ws.lock().await;
        let s = sessions
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Unknown WS session"))?;
        s.cmd_tx.send(WsCommand::SendText(text.clone()))?;
        // Also record the outgoing message locally so the UI sees it immediately.
        let ts = chrono::Utc::now().timestamp_millis() as u64;
        s.events.lock().await.push_back(WsFfiEvent::Message {
            direction: "out".into(),
            text,
            timestamp_ms: ts,
        });
        Ok(())
    }

    pub async fn ws_poll(&self, session_id: &str) -> anyhow::Result<Vec<WsFfiEvent>> {
        let id = Uuid::parse_str(session_id)?;
        let sessions = self.ws.lock().await;
        let s = sessions
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Unknown WS session"))?;
        let mut buf = s.events.lock().await;
        Ok(buf.drain(..).collect())
    }

    pub async fn ws_disconnect(&self, session_id: &str) -> anyhow::Result<()> {
        let id = Uuid::parse_str(session_id)?;
        let mut sessions = self.ws.lock().await;
        if let Some(s) = sessions.remove(&id) {
            let _ = s.cmd_tx.send(WsCommand::Disconnect);
        }
        Ok(())
    }

    // ---- SSE ----

    pub async fn sse_connect(&self, url: &str, headers: &[KeyValuePair]) -> anyhow::Result<String> {
        let id = Uuid::new_v4();
        let events: Arc<Mutex<VecDeque<SseFfiEvent>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<SseOutput>();

        let cmd_tx = hitt_core::protocols::sse::connect(url, headers, event_tx)?;

        let events_clone = events.clone();
        tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                let ffi_ev = match ev {
                    SseOutput::Connected => SseFfiEvent::Opened,
                    SseOutput::Disconnected => SseFfiEvent::Closed,
                    SseOutput::Error(msg) => SseFfiEvent::Error { message: msg },
                    SseOutput::Event(e) => SseFfiEvent::Event {
                        event_type: e.event_type,
                        id: e.id,
                        data: e.data,
                        timestamp_ms: e.timestamp.timestamp_millis() as u64,
                    },
                };
                events_clone.lock().await.push_back(ffi_ev);
            }
        });

        self.sse
            .lock()
            .await
            .insert(id, SseSession { cmd_tx, events });
        Ok(id.to_string())
    }

    pub async fn sse_poll(&self, session_id: &str) -> anyhow::Result<Vec<SseFfiEvent>> {
        let id = Uuid::parse_str(session_id)?;
        let sessions = self.sse.lock().await;
        let s = sessions
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Unknown SSE session"))?;
        let mut buf = s.events.lock().await;
        Ok(buf.drain(..).collect())
    }

    pub async fn sse_disconnect(&self, session_id: &str) -> anyhow::Result<()> {
        let id = Uuid::parse_str(session_id)?;
        let mut sessions = self.sse.lock().await;
        if let Some(s) = sessions.remove(&id) {
            let _ = s.cmd_tx.send(SseCommand::Disconnect);
        }
        Ok(())
    }
}
