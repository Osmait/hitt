use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::core::request::KeyValuePair;

const MAX_MESSAGES: usize = 1000;

#[derive(Debug, Clone)]
pub struct WebSocketSession {
    pub id: Uuid,
    pub url: String,
    pub status: WsStatus,
    pub messages: Vec<WsMessage>,
    pub headers: Vec<KeyValuePair>,
    pub auto_reconnect: bool,
    pub ping_interval: Option<Duration>,
}

impl WebSocketSession {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            url: url.into(),
            status: WsStatus::Disconnected,
            messages: Vec::new(),
            headers: Vec::new(),
            auto_reconnect: false,
            ping_interval: None,
        }
    }

    pub fn push_message(&mut self, msg: WsMessage) {
        if self.messages.len() >= MAX_MESSAGES {
            self.messages.drain(..MAX_MESSAGES / 4);
        }
        self.messages.push(msg);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WsStatus {
    Disconnected,
    Connecting,
    Connected { connected_at: DateTime<Utc> },
    Reconnecting { attempt: u32 },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct WsMessage {
    pub direction: MessageDirection,
    pub content: WsContent,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone)]
pub enum WsContent {
    Text(String),
    Binary(Vec<u8>),
}

impl WsContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            WsContent::Text(s) => Some(s),
            WsContent::Binary(_) => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            WsContent::Text(s) => s.clone(),
            WsContent::Binary(b) => format!("[Binary: {} bytes]", b.len()),
        }
    }
}

#[derive(Debug)]
pub enum WsCommand {
    Connect,
    Disconnect,
    SendText(String),
    SendBinary(Vec<u8>),
    Ping,
}

#[derive(Debug)]
pub enum WsEvent {
    Connected,
    Disconnected,
    MessageReceived(WsMessage),
    Error(String),
}

/// How long the WebSocket handshake (TCP + TLS + HTTP upgrade) may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Build the handshake request: the URL plus enabled user headers, and hitt's
/// `User-Agent` unless the user set one.
fn build_handshake_request(
    url: &str,
    headers: &[KeyValuePair],
) -> anyhow::Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};

    let mut request = url.into_client_request()?;
    let has_user_agent = headers
        .iter()
        .any(|h| h.enabled && h.key.eq_ignore_ascii_case("user-agent"));
    if !has_user_agent {
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::USER_AGENT,
            HeaderValue::from_str(&crate::core::client::user_agent())?,
        );
    }
    for header in headers
        .iter()
        .filter(|h| h.enabled && !h.key.trim().is_empty())
    {
        let name = HeaderName::from_bytes(header.key.trim().as_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid header name '{}': {e}", header.key))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|e| anyhow::anyhow!("Invalid value for header '{}': {e}", header.key))?;
        request.headers_mut().insert(name, value);
    }
    Ok(request)
}

/// Start a WebSocket connection in the background and return its command sender.
///
/// Returns immediately; the handshake runs on a spawned task and its outcome
/// arrives on `event_tx` (`Connected` or `Error`). Sending
/// `WsCommand::Disconnect` or dropping the returned sender while the
/// handshake is still pending cancels it without emitting further events.
#[allow(clippy::unused_async)]
pub async fn connect(
    url: &str,
    headers: &[KeyValuePair],
    event_tx: mpsc::UnboundedSender<WsEvent>,
) -> anyhow::Result<mpsc::UnboundedSender<WsCommand>> {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WsCommand>();

    let url = url.trim().to_string();
    let headers = headers.to_vec();

    tokio::spawn(async move {
        let request = match build_handshake_request(&url, &headers) {
            Ok(request) => request,
            Err(e) => {
                let _ = event_tx.send(WsEvent::Error(format!("Invalid request: {e}")));
                return;
            }
        };

        let handshake =
            tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request));
        tokio::pin!(handshake);
        // Commands sent while connecting are replayed once connected.
        let mut queued = Vec::new();
        let connect_result = loop {
            tokio::select! {
                result = &mut handshake => break result,
                cmd = cmd_rx.recv() => match cmd {
                    // Cancelled (or the owner went away) before the handshake finished.
                    Some(WsCommand::Disconnect) | None => return,
                    Some(WsCommand::Connect) => {}
                    Some(other) => queued.push(other),
                },
            }
        };

        let Ok(connect_result) = connect_result else {
            let _ = event_tx.send(WsEvent::Error(format!(
                "Connection timed out ({}s)",
                CONNECT_TIMEOUT.as_secs()
            )));
            return;
        };

        match connect_result {
            Ok((ws_stream, _)) => {
                let _ = event_tx.send(WsEvent::Connected);
                let (mut write, mut read) = ws_stream.split();

                // Spawn reader
                let event_tx_clone = event_tx.clone();
                let reader = tokio::spawn(async move {
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                let _ = event_tx_clone.send(WsEvent::MessageReceived(WsMessage {
                                    direction: MessageDirection::Received,
                                    content: WsContent::Text(text.clone()),
                                    timestamp: Utc::now(),
                                }));
                            }
                            Ok(Message::Binary(data)) => {
                                let _ = event_tx_clone.send(WsEvent::MessageReceived(WsMessage {
                                    direction: MessageDirection::Received,
                                    content: WsContent::Binary(data.clone()),
                                    timestamp: Utc::now(),
                                }));
                            }
                            Ok(Message::Close(_)) => {
                                let _ = event_tx_clone.send(WsEvent::Disconnected);
                                break;
                            }
                            Err(e) => {
                                let _ = event_tx_clone.send(WsEvent::Error(e.to_string()));
                                break;
                            }
                            _ => {}
                        }
                    }
                });

                // Handle commands (queued during the handshake first)
                let mut queued = queued.into_iter();
                while let Some(cmd) = match queued.next() {
                    Some(cmd) => Some(cmd),
                    None => cmd_rx.recv().await,
                } {
                    match cmd {
                        WsCommand::SendText(text) => {
                            if let Err(e) = write.send(Message::Text(text)).await {
                                let _ = event_tx.send(WsEvent::Error(e.to_string()));
                                break;
                            }
                        }
                        WsCommand::SendBinary(data) => {
                            if let Err(e) = write.send(Message::Binary(data)).await {
                                let _ = event_tx.send(WsEvent::Error(e.to_string()));
                                break;
                            }
                        }
                        WsCommand::Ping => {
                            if let Err(e) = write.send(Message::Ping(vec![])).await {
                                let _ = event_tx.send(WsEvent::Error(e.to_string()));
                                break;
                            }
                        }
                        WsCommand::Disconnect => {
                            let _ = write.send(Message::Close(None)).await;
                            let _ = event_tx.send(WsEvent::Disconnected);
                            break;
                        }
                        WsCommand::Connect => {} // Already connected
                    }
                }

                reader.abort();
            }
            Err(e) => {
                let _ = event_tx.send(WsEvent::Error(e.to_string()));
            }
        }
    });

    Ok(cmd_tx)
}
