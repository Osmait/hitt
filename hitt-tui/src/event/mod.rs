mod actions;
mod chain;
mod command_mode;
mod commands;
mod editors;
mod environments;
mod import_export;
mod insert_mode;
mod key_handlers;
mod modal_handlers;
mod mouse_handlers;
mod navigation;
mod persistence;
mod protocols;
mod runner;
mod sidebar;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app::{App, AppMode, ModalKind, NotificationKind};
use hitt_core::core::request::Request;
use hitt_core::core::response::Response;
use hitt_core::protocols::sse::SseEvent as ProtocolSseEvent;
use hitt_core::protocols::websocket::WsMessage;

// Re-export sub-module items used by the rest of the crate.
pub use self::chain::{chain_failed_steps, start_chain_execution};
pub use self::commands::execute_command;
pub use self::editors::{abandon_editing, active_env_is_session_only, open_env_editor};
pub use self::protocols::{handle_sse_protocol_event, handle_ws_protocol_event};
pub use self::runner::{start_collection_run, CollectionRunEvent};
pub use self::sidebar::{build_sidebar_items, SidebarItem};

// Import sub-module items used within mod.rs itself.
use self::chain::handle_chain_step_event;
use self::command_mode::handle_command_mode;
use self::editors::handle_paste;
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
    /// A background HTTP request started by `App::send_request` finished.
    HttpResponse {
        tab_id: Uuid,
        token: Uuid,
        request: Box<Request>,
        result: Result<Box<Response>, String>,
    },
    /// A background `:loadtest` run finished.
    LoadTestComplete(String, NotificationKind),
    Notification(String, NotificationKind),
    WebSocketEvent {
        session_id: Uuid,
        event: WsEventData,
    },
    SseEvent {
        session_id: Uuid,
        event: SseEventData,
    },
    /// Progress of the chain run `run_id` (stale runs are ignored).
    ChainStepComplete {
        run_id: Uuid,
        event: ChainStepEvent,
    },
    /// Progress of a background collection run (`:run`).
    CollectionRun {
        run_id: Uuid,
        event: CollectionRunEvent,
    },
    /// Bracketed paste: the whole pasted text at once.
    Paste(String),
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
                            Event::Paste(text) => event_tx.send(AppEvent::Paste(text)),
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
        AppEvent::Key(key) => {
            let before = OverlaySnapshot::take(app);
            handle_key(app, key).await?;
            before.restore_after(app);
        }
        AppEvent::Mouse(mouse) => {
            let before = OverlaySnapshot::take(app);
            handle_mouse(app, mouse).await?;
            before.restore_after(app);
        }
        AppEvent::Tick => {
            app.clear_expired_notification();
        }
        AppEvent::Resize(_, _) => {}
        AppEvent::HttpResponse {
            tab_id,
            token,
            request,
            result,
        } => {
            app.finish_http_request(tab_id, token, &request, result);
        }
        AppEvent::LoadTestComplete(msg, kind) => {
            app.load_test_running = false;
            app.notify(msg, kind);
        }
        AppEvent::Notification(msg, kind) => {
            app.notify(msg, kind);
        }
        AppEvent::WebSocketEvent { session_id, event } => {
            handle_ws_protocol_event(app, session_id, event);
        }
        AppEvent::SseEvent { session_id, event } => {
            handle_sse_protocol_event(app, session_id, event);
        }
        AppEvent::ChainStepComplete { run_id, event } => {
            handle_chain_step_event(app, run_id, event);
        }
        AppEvent::CollectionRun { run_id, event } => {
            self::runner::handle_collection_run_event(app, run_id, event);
        }
        AppEvent::Paste(text) => {
            handle_paste(app, &text);
        }
    }
    Ok(())
}

/// True if a key with these modifiers should insert its character into a text
/// input. Shifted characters arrive with `SHIFT`; Ctrl/Alt chords are commands.
pub(super) fn is_text_input(modifiers: KeyModifiers) -> bool {
    modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT
}

fn is_overlay(mode: &AppMode) -> bool {
    matches!(mode, AppMode::Modal(_) | AppMode::Command)
}

/// Remembers the nav mode while a modal / command palette is open so closing
/// it returns the user to the nav mode they came from.
struct OverlaySnapshot {
    was_overlay: bool,
    nav_mode: crate::app::NavMode,
}

impl OverlaySnapshot {
    fn take(app: &App) -> Self {
        Self {
            was_overlay: is_overlay(&app.mode),
            nav_mode: app.nav_mode,
        }
    }

    fn restore_after(self, app: &mut App) {
        let now_overlay = is_overlay(&app.mode);
        if !self.was_overlay && now_overlay {
            app.overlay_nav_mode = Some(self.nav_mode);
        } else if self.was_overlay && !now_overlay {
            if let Some(nav) = app.overlay_nav_mode.take() {
                if app.mode == AppMode::Normal {
                    app.nav_mode = nav;
                }
            }
        }
        // A confirm action only lives as long as its dialog.
        if !matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))) {
            app.pending_confirm = None;
        }
    }
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
