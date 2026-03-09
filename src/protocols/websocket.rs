use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::core::request::KeyValuePair;

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
            _ => None,
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

pub async fn connect(
    url: &str,
    headers: &[KeyValuePair],
    event_tx: mpsc::UnboundedSender<WsEvent>,
) -> anyhow::Result<mpsc::UnboundedSender<WsCommand>> {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WsCommand>();

    let url = url.to_string();
    let headers = headers.to_vec();

    tokio::spawn(async move {
        // Validate URI before connecting
        let url = url.trim().to_string();
        if let Err(e) = url.parse::<http::Uri>() {
            let _ = event_tx.send(WsEvent::Error(format!("Invalid URI: {}", e)));
            return;
        }

        let connect_result = tokio_tungstenite::connect_async(&url).await;

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
                                    content: WsContent::Text(text.to_string()),
                                    timestamp: Utc::now(),
                                }));
                            }
                            Ok(Message::Binary(data)) => {
                                let _ = event_tx_clone.send(WsEvent::MessageReceived(WsMessage {
                                    direction: MessageDirection::Received,
                                    content: WsContent::Binary(data.to_vec()),
                                    timestamp: Utc::now(),
                                }));
                            }
                            Ok(Message::Close(_)) => {
                                let _ = event_tx_clone.send(WsEvent::Disconnected);
                                break;
                            }
                            Ok(Message::Ping(_)) => {} // tungstenite auto-responds
                            Ok(Message::Pong(_)) => {}
                            Err(e) => {
                                let _ = event_tx_clone.send(WsEvent::Error(e.to_string()));
                                break;
                            }
                            _ => {}
                        }
                    }
                });

                // Handle commands
                while let Some(cmd) = cmd_rx.recv().await {
                    match cmd {
                        WsCommand::SendText(text) => {
                            if let Err(e) = write.send(Message::Text(text.into())).await {
                                let _ = event_tx.send(WsEvent::Error(e.to_string()));
                                break;
                            }
                        }
                        WsCommand::SendBinary(data) => {
                            if let Err(e) = write.send(Message::Binary(data.into())).await {
                                let _ = event_tx.send(WsEvent::Error(e.to_string()));
                                break;
                            }
                        }
                        WsCommand::Ping => {
                            if let Err(e) = write.send(Message::Ping(vec![].into())).await {
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
