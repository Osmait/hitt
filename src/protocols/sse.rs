use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::core::request::KeyValuePair;

const MAX_EVENTS: usize = 1000;
const MAX_ACCUMULATED_BYTES: usize = 512 * 1024; // 512 KB

#[derive(Debug, Clone)]
pub struct SseSession {
    pub id: Uuid,
    pub url: String,
    pub status: SseStatus,
    pub events: Vec<SseEvent>,
    pub headers: Vec<KeyValuePair>,
    pub auto_reconnect: bool,
    pub accumulated_text: String,
    pub last_event_id: Option<String>,
}

impl SseSession {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            url: url.into(),
            status: SseStatus::Disconnected,
            events: Vec::new(),
            headers: Vec::new(),
            auto_reconnect: true,
            accumulated_text: String::new(),
            last_event_id: None,
        }
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn push_event(&mut self, evt: SseEvent) {
        self.accumulated_text.push_str(&evt.data);
        self.accumulated_text.push('\n');
        if self.accumulated_text.len() > MAX_ACCUMULATED_BYTES {
            let drain = self.accumulated_text.len() - MAX_ACCUMULATED_BYTES;
            let boundary = self.accumulated_text[drain..]
                .find('\n')
                .map_or(drain, |p| drain + p + 1);
            self.accumulated_text.drain(..boundary);
        }
        if self.events.len() >= MAX_EVENTS {
            self.events.drain(..MAX_EVENTS / 4);
        }
        self.events.push(evt);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SseStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug)]
pub enum SseCommand {
    Connect,
    Disconnect,
}

#[derive(Debug)]
pub enum SseOutput {
    Connected,
    Event(SseEvent),
    Error(String),
    Disconnected,
}

pub fn connect(
    url: &str,
    headers: &[KeyValuePair],
    event_tx: mpsc::UnboundedSender<SseOutput>,
) -> anyhow::Result<mpsc::UnboundedSender<SseCommand>> {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SseCommand>();

    let url = url.to_string();
    let headers = headers.to_vec();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut request = client.get(&url).header("Accept", "text/event-stream");

        for header in &headers {
            if header.enabled {
                request = request.header(&header.key, &header.value);
            }
        }

        let send_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            request.send(),
        )
        .await;

        let Ok(send_result) = send_result else {
            let _ = event_tx.send(SseOutput::Error("Connection timed out (30s)".into()));
            return;
        };

        match send_result {
            Ok(response) => {
                if !response.status().is_success() {
                    let _ = event_tx.send(SseOutput::Error(format!("HTTP {}", response.status())));
                    return;
                }

                let _ = event_tx.send(SseOutput::Connected);

                let mut stream = response.bytes_stream();
                let mut buffer = String::new();

                loop {
                    tokio::select! {
                        chunk = stream.next() => {
                            match chunk {
                                Some(Ok(bytes)) => {
                                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                                    // Parse SSE events from buffer
                                    while let Some(event) = parse_sse_event(&mut buffer) {
                                        let _ = event_tx.send(SseOutput::Event(event));
                                    }
                                }
                                Some(Err(e)) => {
                                    let _ = event_tx.send(SseOutput::Error(e.to_string()));
                                    break;
                                }
                                None => {
                                    let _ = event_tx.send(SseOutput::Disconnected);
                                    break;
                                }
                            }
                        }
                        cmd = cmd_rx.recv() => {
                            match cmd {
                                Some(SseCommand::Disconnect) | None => {
                                    let _ = event_tx.send(SseOutput::Disconnected);
                                    break;
                                }
                                Some(SseCommand::Connect) => {} // Already connected
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let _ = event_tx.send(SseOutput::Error(e.to_string()));
            }
        }
    });

    Ok(cmd_tx)
}

fn parse_sse_event(buffer: &mut String) -> Option<SseEvent> {
    // SSE events are separated by double newlines
    let separator = if buffer.contains("\n\n") {
        "\n\n"
    } else if buffer.contains("\r\n\r\n") {
        "\r\n\r\n"
    } else {
        return None;
    };

    let pos = buffer.find(separator)?;
    let event_text = buffer[..pos].to_string();
    *buffer = buffer[pos + separator.len()..].to_string();

    let mut event_type = None;
    let mut data_lines = Vec::new();
    let mut id = None;

    for line in event_text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_type = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("id:") {
            id = Some(value.trim().to_string());
        }
        // Ignore retry: and comments (lines starting with :)
    }

    if data_lines.is_empty() && event_type.is_none() {
        return None;
    }

    Some(SseEvent {
        event_type,
        data: data_lines.join("\n"),
        id,
        timestamp: Utc::now(),
    })
}
