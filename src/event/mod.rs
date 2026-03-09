mod actions;
mod chain;
mod command_mode;
mod commands;
mod import_export;
mod insert_mode;
mod key_handlers;
mod modal_handlers;
mod mouse_handlers;
mod navigation;
mod persistence;
mod protocols;
mod sidebar;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app::{App, AppMode, NotificationKind};
use crate::protocols::sse::SseEvent as ProtocolSseEvent;
use crate::protocols::websocket::WsMessage;

// Re-export sub-module items used by the rest of the crate.
pub use self::chain::start_chain_execution;
pub use self::commands::execute_command;
pub use self::protocols::{handle_sse_protocol_event, handle_ws_protocol_event};
pub use self::sidebar::{build_sidebar_items, SidebarItem};

// Import sub-module items used within mod.rs itself.
use self::chain::handle_chain_step_event;
use self::command_mode::handle_command_mode;
use self::insert_mode::handle_insert_mode;
use self::key_handlers::handle_normal_mode;
use self::modal_handlers::handle_modal_mode;
use self::mouse_handlers::handle_mouse;
use self::navigation::{handle_chain_editor_mode, handle_proxy_mode};

#[derive(Debug, Clone)]
pub enum WsEventData {
    Connected,
    Disconnected,
    MessageReceived(WsMessage),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum SseEventData {
    Connected,
    Disconnected,
    Event(ProtocolSseEvent),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ChainStepEvent {
    Running {
        step_index: usize,
    },
    Success {
        step_index: usize,
        status: u16,
        duration_ms: u64,
        extracted: std::collections::HashMap<String, String>,
    },
    Failed {
        step_index: usize,
        error: String,
    },
    Skipped {
        step_index: usize,
        reason: String,
    },
    Complete,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
    Resize(u16, u16),
    RequestComplete,
    Notification(String, NotificationKind),
    WebSocketEvent {
        session_id: Uuid,
        event: WsEventData,
    },
    SseEvent {
        session_id: Uuid,
        event: SseEventData,
    },
    ChainStepComplete(ChainStepEvent),
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl EventHandler {
    #[must_use]
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();

        // Use a dedicated OS thread for blocking crossterm I/O.
        std::thread::spawn(move || {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let sent = match evt {
                            Event::Key(key) => event_tx.send(AppEvent::Key(key)),
                            Event::Mouse(mouse) => {
                                // Filter out noisy move/drag events.
                                match mouse.kind {
                                    crossterm::event::MouseEventKind::Moved
                                    | crossterm::event::MouseEventKind::Drag(_) => Ok(()),
                                    _ => event_tx.send(AppEvent::Mouse(mouse)),
                                }
                            }
                            Event::Resize(w, h) => event_tx.send(AppEvent::Resize(w, h)),
                            _ => Ok(()),
                        };
                        if sent.is_err() {
                            break; // channel closed, app is shutting down
                        }
                    }
                } else if event_tx.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        Self { rx, tx }
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    #[must_use]
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }
}

#[allow(clippy::missing_errors_doc)]
pub async fn handle_event(app: &mut App, event: AppEvent) -> Result<()> {
    match event {
        AppEvent::Key(key) => handle_key(app, key).await?,
        AppEvent::Mouse(mouse) => handle_mouse(app, mouse).await?,
        AppEvent::Tick => {
            app.clear_expired_notification();
        }
        AppEvent::Resize(_, _) | AppEvent::RequestComplete => {}
        AppEvent::Notification(msg, kind) => {
            app.notify(msg, kind);
        }
        AppEvent::WebSocketEvent { session_id, event } => {
            handle_ws_protocol_event(app, session_id, event);
        }
        AppEvent::SseEvent { session_id, event } => {
            handle_sse_protocol_event(app, session_id, event);
        }
        AppEvent::ChainStepComplete(chain_event) => {
            handle_chain_step_event(app, chain_event);
        }
    }
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    // Global keybindings (work in any mode)
    if let (KeyModifiers::CONTROL, KeyCode::Char('c')) = (key.modifiers, key.code) {
        app.should_quit = true;
        return Ok(());
    }

    match &app.mode {
        AppMode::Normal => handle_normal_mode(app, key).await,
        AppMode::Insert => handle_insert_mode(app, key).await,
        AppMode::Command => handle_command_mode(app, key).await,
        AppMode::Modal(kind) => {
            let kind = kind.clone();
            handle_modal_mode(app, key, &kind).await
        }
        AppMode::ChainEditor => handle_chain_editor_mode(app, key).await,
        AppMode::ProxyInspector => handle_proxy_mode(app, key).await,
    }
}
