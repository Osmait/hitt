use anyhow::Result;
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ui::theme::Theme;
use hitt_core::core::chain::{ChainExecutionState, RequestChain};
use hitt_core::core::client::HttpClient;
use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::history::HistoryStore;
use hitt_core::core::request::{HttpMethod, Request};
use hitt_core::core::response::Response;
use hitt_core::core::variables::VariableResolver;
use hitt_core::protocols::sse::{SseCommand, SseSession};
use hitt_core::protocols::websocket::{WebSocketSession, WsCommand};
use hitt_core::storage::config::AppConfig;

/// Tracks the screen regions for mouse hit-testing.
/// Updated every render frame by layout.rs.
#[derive(Debug, Clone, Default)]
pub struct ClickableRegions {
    pub sidebar: Option<Rect>,
    pub url_bar: Option<Rect>,
    pub method_selector: Option<Rect>,
    pub header_tab_bar: Option<Rect>,
    pub request_tab_bar: Option<Rect>,
    /// One rect per request sub-tab label (padding included, divider excluded).
    pub request_tabs: Vec<(Rect, RequestTabKind)>,
    pub request_body: Option<Rect>,
    /// Visible rows of the active request table (params, headers, form body,
    /// assertions): `(rect, row index)`.
    pub request_rows: Vec<(Rect, usize)>,
    pub response_tab_bar: Option<Rect>,
    pub response_tabs: Vec<(Rect, ResponseTabKind)>,
    pub response_body: Option<Rect>,
    pub ws_input_bar: Option<Rect>,
    pub header_tabs: Vec<(Rect, usize)>, // (label rect, tab_index)
    /// The `ENV: <name>` text of the environment indicator.
    pub env_selector: Option<Rect>,
    pub sidebar_section_tabs: Vec<(Rect, SidebarSection)>,
    /// One rect per visible sidebar entry that exists: `(rect, item index)`.
    pub sidebar_items: Vec<(Rect, usize)>,
    pub status_bar: Option<Rect>,
    /// Open overlay (modal, command palette, chain runner). While set, the
    /// panels behind ignore the mouse and a click outside closes it.
    pub overlay: Option<Rect>,
    /// Clickable rows of the open modal list: `(rect, item index)`.
    pub modal_items: Vec<(Rect, usize)>,
    pub confirm_yes: Option<Rect>,
    pub confirm_no: Option<Rect>,
    /// First header tab shown in the (scrolled) tab bar. Kept across frames
    /// (not reset by `clear`) so selecting a visible tab doesn't scroll the bar.
    pub header_tab_start: usize,
}

impl ClickableRegions {
    pub fn clear(&mut self) {
        let header_tab_start = self.header_tab_start;
        *self = Self {
            header_tab_start,
            ..Self::default()
        };
    }
}

/// Scroll state of a live message list (WebSocket messages, SSE events).
///
/// `offset` is the first visible row. While `follow` is set the view sticks to
/// the bottom as new entries arrive. `page` is the number of visible rows,
/// recorded by the renderer so key/mouse handlers can clamp immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListScroll {
    pub offset: usize,
    pub follow: bool,
    pub page: usize,
}

impl Default for ListScroll {
    fn default() -> Self {
        Self {
            offset: 0,
            follow: true,
            page: 0,
        }
    }
}

impl ListScroll {
    /// Largest valid `offset` for `len` entries.
    pub fn max_offset(&self, len: usize) -> usize {
        len.saturating_sub(self.page.max(1))
    }

    pub fn up(&mut self, lines: usize, len: usize) {
        let max = self.max_offset(len);
        let from = if self.follow {
            max
        } else {
            self.offset.min(max)
        };
        self.offset = from.saturating_sub(lines);
        self.follow = false;
    }

    pub fn down(&mut self, lines: usize, len: usize) {
        let max = self.max_offset(len);
        self.offset = self.offset.saturating_add(lines).min(max);
        self.follow = self.offset >= max;
    }

    pub fn top(&mut self) {
        self.offset = 0;
        self.follow = false;
    }

    pub fn bottom(&mut self, len: usize) {
        self.offset = self.max_offset(len);
        self.follow = true;
    }

    /// Record the visible height and return the clamped first visible row.
    /// Reaching the bottom (e.g. the list fits) turns `follow` back on.
    pub fn view_start(&mut self, len: usize, page: usize) -> usize {
        self.page = page;
        let max = self.max_offset(len);
        if self.follow || self.offset >= max {
            self.offset = max;
            self.follow = true;
        }
        self.offset
    }
}

pub struct App {
    pub mode: AppMode,
    pub nav_mode: NavMode,
    pub focus: FocusArea,
    pub last_right_focus: FocusArea,
    pub tabs: Vec<RequestTab>,
    pub active_tab: usize,
    pub collections: Vec<Collection>,
    pub environments: Vec<Environment>,
    pub active_env: Option<usize>,
    pub history: HistoryStore,
    pub config: AppConfig,
    pub theme: Theme,
    pub notification: Option<Notification>,
    pub active_chain: Option<ChainExecutionState>,
    pub active_chain_def: Option<RequestChain>,
    pub active_chain_coll_idx: Option<usize>,
    pub chain_scroll: usize,
    /// Focus to restore when the chain runner is closed.
    pub chain_return_focus: FocusArea,
    /// Chain targeted by `:addstep` (set by `:newchain`, `:chain`, sidebar run, import).
    pub selected_chain: Option<Uuid>,
    pub http_client: HttpClient,
    pub should_quit: bool,
    pub sidebar_state: SidebarState,
    pub command_input: String,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_selected: usize,
    /// First visible search result, kept between frames (sticky scrolling).
    pub search_scroll: std::cell::Cell<usize>,
    pub help_scroll: usize,
    /// True while any tab has an HTTP request in flight (see `RequestTab::pending_request`).
    pub loading: bool,
    pub load_test_running: bool,
    pub regions: ClickableRegions,
    pub response_scroll: usize,
    pub request_scroll: usize,
    /// True while the response filter bar is taking input (the filter text
    /// itself is per tab: `RequestTab::response_filter`).
    pub response_filter_active: bool,
    pub rename_input: String,
    pub modal_input: String,
    pub collection_picker_selected: usize,
    pub theme_picker_selected: usize,
    pub theme_before_preview: Option<Theme>,
    /// Action to run when the open `ModalKind::Confirm` dialog is accepted.
    pub pending_confirm: Option<ConfirmAction>,
    /// Mode to return to when the confirm dialog closes (`None` = Normal),
    /// e.g. the environment editor after "Delete environment?".
    pub confirm_return_mode: Option<AppMode>,
    /// Nav mode to restore when a modal / command palette closes.
    pub overlay_nav_mode: Option<NavMode>,
    // Diff selector / viewer modal state
    pub diff_state: crate::ui::widgets::diff_viewer::DiffSelectorState,
    // Event sender for protocol bridge tasks
    pub event_sender: Option<mpsc::UnboundedSender<crate::event::AppEvent>>,
    /// Selection and in-progress edits of the request / environment editors.
    pub editor: crate::editor::EditorState,
    /// Environments loaded from `.env` files (`--env`, `:env-file`): never persisted.
    pub session_env_ids: std::collections::HashSet<Uuid>,
    /// Where `config` is written when the app changes it (e.g. the last active
    /// environment). `None` (the `App::new` default, used by tests) keeps
    /// changes in memory only.
    pub config_path: Option<std::path::PathBuf>,
    /// Collection runner (`:run`, sidebar `R`): current or last run.
    pub runner: crate::ui::widgets::runner::RunnerState,
    /// Id of the current chain run; step events from other runs are ignored.
    pub chain_run_id: Option<Uuid>,
    /// Abort handle of the background chain task (Esc in the chain runner).
    pub chain_abort: Option<tokio::task::AbortHandle>,
    /// The last chain run was stopped by the user (header shows "Cancelled").
    pub chain_cancelled: bool,
    /// Previously executed `:` commands (oldest first), for Up/Down recall.
    pub command_history: Vec<String>,
    /// Position in `command_history` while browsing it with Up/Down.
    pub command_history_idx: Option<usize>,
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self> {
        let theme = Theme::default();
        let http_client = HttpClient::from_config(&config)?;

        Ok(Self {
            mode: AppMode::Normal,
            nav_mode: NavMode::Global,
            focus: FocusArea::Sidebar,
            last_right_focus: FocusArea::UrlBar,
            tabs: vec![RequestTab::new()],
            active_tab: 0,
            collections: Vec::new(),
            environments: Vec::new(),
            active_env: None,
            history: HistoryStore::new(config.history_limit),
            config,
            theme,
            notification: None,
            active_chain: None,
            active_chain_def: None,
            active_chain_coll_idx: None,
            chain_scroll: 0,
            chain_return_focus: FocusArea::Sidebar,
            selected_chain: None,
            http_client,
            should_quit: false,
            sidebar_state: SidebarState::default(),
            command_input: String::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_scroll: std::cell::Cell::new(0),
            help_scroll: 0,
            loading: false,
            load_test_running: false,
            regions: ClickableRegions::default(),
            response_scroll: 0,
            request_scroll: 0,
            response_filter_active: false,
            rename_input: String::new(),
            modal_input: String::new(),
            collection_picker_selected: 0,
            theme_picker_selected: 0,
            theme_before_preview: None,
            pending_confirm: None,
            confirm_return_mode: None,
            overlay_nav_mode: None,
            diff_state: crate::ui::widgets::diff_viewer::DiffSelectorState::default(),
            event_sender: None,
            editor: crate::editor::EditorState::default(),
            session_env_ids: std::collections::HashSet::new(),
            config_path: None,
            runner: crate::ui::widgets::runner::RunnerState::default(),
            chain_run_id: None,
            chain_abort: None,
            chain_cancelled: false,
            command_history: Vec::new(),
            command_history_idx: None,
        })
    }

    /// Rebuild the HTTP client from `self.config` so changes to `timeout_ms`,
    /// `follow_redirects` or `verify_ssl` take effect for subsequent requests.
    /// On failure the previous client is kept and the error is returned.
    pub fn rebuild_http_client(&mut self) -> Result<()> {
        self.http_client = HttpClient::from_config(&self.config)?;
        Ok(())
    }

    pub fn event_tx(&self) -> mpsc::UnboundedSender<crate::event::AppEvent> {
        self.event_sender.clone().expect("event_sender not set")
    }

    pub fn active_tab(&self) -> &RequestTab {
        &self.tabs[self.active_tab]
    }

    pub fn active_tab_mut(&mut self) -> &mut RequestTab {
        &mut self.tabs[self.active_tab]
    }

    /// Open a new empty tab and focus its URL bar so the user can type right away.
    pub fn new_tab(&mut self) {
        self.tabs.push(RequestTab::new());
        self.active_tab = self.tabs.len() - 1;
        self.response_scroll = 0;
        self.request_scroll = 0;
        self.navigate_to_panel(FocusArea::UrlBar);
    }

    pub fn close_tab(&mut self) {
        if self.tabs.len() > 1 {
            let mut tab = self.tabs.remove(self.active_tab);
            tab.abort_pending_request();
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            }
            self.response_scroll = 0;
            self.request_scroll = 0;
            self.refresh_loading();
        }
    }

    /// Tabs with unsaved edits.
    pub fn dirty_tab_count(&self) -> usize {
        self.tabs.iter().filter(|t| t.dirty).count()
    }

    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            if index != self.active_tab {
                self.reset_view_for_tab_switch();
            }
            self.active_tab = index;
        }
    }

    pub fn next_tab(&mut self) {
        self.switch_tab((self.active_tab + 1) % self.tabs.len());
    }

    pub fn prev_tab(&mut self) {
        let idx = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
        self.switch_tab(idx);
    }

    /// Scroll offsets and filter input are not per tab: reset them when the
    /// active tab changes so the new tab starts at the top.
    fn reset_view_for_tab_switch(&mut self) {
        self.response_scroll = 0;
        self.request_scroll = 0;
        self.response_filter_active = false;
    }

    /// Activate the tab already showing request `request_id` of collection
    /// `coll_idx`, if any. Returns true if such a tab exists.
    pub fn focus_open_request(&mut self, coll_idx: usize, request_id: Uuid) -> bool {
        let Some(idx) = self
            .tabs
            .iter()
            .position(|t| t.collection_index == Some(coll_idx) && t.request.id == request_id)
        else {
            return false;
        };
        self.switch_tab(idx);
        true
    }

    pub fn active_environment(&self) -> Option<&Environment> {
        self.active_env.and_then(|i| self.environments.get(i))
    }

    /// Write `config.last_environment` to the config file at `config_path`
    /// (no-op when unset). Only that field is updated so in-memory overrides
    /// (e.g. `--theme`) are not persisted; an unreadable file is left alone.
    fn persist_last_environment(&self) {
        let last = self.config.last_environment.clone();
        self.persist_config_change(|on_disk| on_disk.last_environment = last);
    }

    /// Persist the current theme to the config file (see `persist_config_change`).
    pub fn persist_theme(&self) {
        let theme = self.config.theme.clone();
        self.persist_config_change(|on_disk| on_disk.theme = theme);
    }

    /// Apply `change` to the config file on disk without writing other
    /// runtime overrides (e.g. `--theme`). No-op without `config_path`, so
    /// tests never touch the real config.
    pub fn persist_config_change(&self, change: impl FnOnce(&mut AppConfig)) {
        let Some(path) = &self.config_path else {
            return;
        };
        let mut on_disk = if path.exists() {
            match AppConfig::load_from(path) {
                Ok(config) => config,
                Err(e) => {
                    tracing::warn!("Not saving config, file unreadable: {}", e);
                    return;
                }
            }
        } else {
            self.config.clone()
        };
        change(&mut on_disk);
        if let Err(e) = on_disk.save_to(path) {
            tracing::warn!("Failed to save config: {}", e);
        }
    }

    /// Record the active environment as `last_environment` and persist the
    /// config. Session-only environments (`.env` files) are not remembered.
    pub fn remember_active_env(&mut self) {
        let name = match self.active_environment() {
            Some(env) if self.session_env_ids.contains(&env.id) => return,
            Some(env) => Some(env.name.clone()),
            None => None,
        };
        if self.config.last_environment != name {
            self.config.last_environment = name;
            self.persist_last_environment();
        }
    }

    /// Activate `last_environment` (falling back to `default_environment`)
    /// if an environment with that name is loaded.
    pub fn restore_last_environment(&mut self) {
        let find = |name: &Option<String>| {
            name.as_ref().and_then(|n| {
                self.environments
                    .iter()
                    .position(|e| e.name == *n)
                    .or_else(|| {
                        self.environments
                            .iter()
                            .position(|e| e.name.eq_ignore_ascii_case(n))
                    })
            })
        };
        if let Some(idx) =
            find(&self.config.last_environment).or_else(|| find(&self.config.default_environment))
        {
            self.active_env = Some(idx);
        }
    }

    pub fn build_resolver(&self) -> VariableResolver {
        let tab = self.active_tab();
        let collection_vars: &[hitt_core::core::request::KeyValuePair] =
            if let Some(coll_idx) = tab.collection_index {
                if let Some(coll) = self.collections.get(coll_idx) {
                    &coll.variables
                } else {
                    &[]
                }
            } else {
                &[]
            };

        let chain_vars = self.active_chain.as_ref().map(|c| &c.extracted_variables);

        VariableResolver::from_context(
            chain_vars,
            collection_vars,
            self.active_environment(),
            None,
            None,
        )
    }

    pub async fn send_request(&mut self) -> Result<()> {
        let protocol = self.tabs[self.active_tab].request.protocol.clone();
        match protocol {
            hitt_core::core::request::Protocol::Http => self.send_http_request().await,
            hitt_core::core::request::Protocol::WebSocket => self.toggle_ws_connection().await,
            hitt_core::core::request::Protocol::Sse => self.toggle_sse_connection().await,
            hitt_core::core::request::Protocol::Grpc { .. } => Ok(()),
        }
    }

    /// Start the active tab's HTTP request without blocking the event loop.
    ///
    /// The request runs on a tokio task and its result comes back as
    /// `AppEvent::HttpResponse`, handled by `finish_http_request`. Without an
    /// event loop attached (tests / headless use) the request runs inline.
    async fn send_http_request(&mut self) -> Result<()> {
        if self.tabs[self.active_tab].pending_request.is_some() {
            self.notify(
                "Request already in progress (Esc to cancel)".into(),
                NotificationKind::Warning,
            );
            return Ok(());
        }
        let request = self.tabs[self.active_tab].request.clone();
        if request.url.trim().is_empty() {
            self.notify("Enter a URL first".into(), NotificationKind::Warning);
            return Ok(());
        }
        let resolver = self.build_resolver();
        let client = self.http_client.clone();
        let tab_id = self.tabs[self.active_tab].id;
        let token = Uuid::new_v4();
        // Send with folder/collection auth applied; history keeps the request as edited.
        let mut wire_request = request.clone();
        if let Some(coll) = self.tabs[self.active_tab]
            .collection_index
            .and_then(|i| self.collections.get(i))
        {
            wire_request.auth =
                hitt_core::testing::collection_runner::effective_auth_in(coll, &request);
        }
        // Unknown `{{vars}}` are sent literally; say so (the request still goes out).
        let unresolved = unresolved_variables(&wire_request, &resolver);
        if !unresolved.is_empty() {
            self.notify(
                format!("Unresolved variables: {}", unresolved.join(", ")),
                NotificationKind::Warning,
            );
        }

        if let Some(tx) = self.event_sender.clone() {
            let task_request = request;
            let handle = tokio::spawn(async move {
                let result = client
                    .send(&wire_request, &resolver)
                    .await
                    .map(Box::new)
                    .map_err(|e| format!("{e:#}"));
                let _ = tx.send(crate::event::AppEvent::HttpResponse {
                    tab_id,
                    token,
                    request: Box::new(task_request),
                    result,
                });
            });
            self.tabs[self.active_tab].pending_request = Some(PendingRequest {
                token,
                started_at: std::time::Instant::now(),
                abort: Some(handle.abort_handle()),
            });
            self.refresh_loading();
        } else {
            self.tabs[self.active_tab].pending_request = Some(PendingRequest {
                token,
                started_at: std::time::Instant::now(),
                abort: None,
            });
            let result = client
                .send(&wire_request, &resolver)
                .await
                .map(Box::new)
                .map_err(|e| format!("{e:#}"));
            self.finish_http_request(tab_id, token, &request, result);
        }
        Ok(())
    }

    /// Apply a finished HTTP request to the tab that started it.
    ///
    /// Results for closed tabs, cancelled or superseded requests are dropped.
    pub fn finish_http_request(
        &mut self,
        tab_id: Uuid,
        token: Uuid,
        request: &Request,
        result: std::result::Result<Box<Response>, String>,
    ) {
        let Some(tab_idx) = self.tabs.iter().position(|t| t.id == tab_id) else {
            return;
        };
        if self.tabs[tab_idx].pending_request.as_ref().map(|p| p.token) != Some(token) {
            return;
        }
        self.tabs[tab_idx].pending_request = None;
        self.refresh_loading();

        match result {
            Ok(response) => {
                let mut response = *response;
                // Run assertions
                let results = hitt_core::testing::assertion_engine::AssertionEngine::run_assertions(
                    &request.assertions,
                    &response,
                );
                response.assertion_results = results;

                // Add to history, with a snapshot of the request for replay.
                self.record_history(hitt_core::core::history::HistoryEntry::from_exchange(
                    request, &response,
                ));

                self.tabs[tab_idx].response = Some(response);
                self.tabs[tab_idx].last_error = None;
                self.tabs[tab_idx].response_tab = ResponseTabKind::Body;
                if tab_idx == self.active_tab {
                    self.response_scroll = 0;
                }
            }
            Err(e) => {
                // Drop the previous response so the failure can't pass for
                // a success; the response area shows the full error instead.
                let tab = &mut self.tabs[tab_idx];
                tab.response = None;
                tab.last_error = Some(e.clone());
                if tab_idx == self.active_tab {
                    self.response_scroll = 0;
                }
                self.notify(format!("Request failed: {e}"), NotificationKind::Error);
            }
        }
    }

    // ── Request history (persistence + replay) ───────────────────

    /// `history.json`, next to the collections directory.
    pub fn history_path(&self) -> std::path::PathBuf {
        hitt_core::storage::history_store::history_file(&self.config.collections_dir)
    }

    /// Load the persisted history (no-op when `persist_history` is off).
    pub fn load_history(&mut self) {
        if !self.config.persist_history {
            return;
        }
        let limit = self.config.history_limit;
        match hitt_core::storage::history_store::load_history(&self.history_path(), limit) {
            Ok(entries) => self.history = HistoryStore::with_entries(limit, entries),
            Err(e) => tracing::warn!("Failed to load history: {e:#}"),
        }
    }

    /// Write the history file (no-op when `persist_history` is off).
    pub fn save_history(&self) {
        if !self.config.persist_history {
            return;
        }
        let path = self.history_path();
        if let Err(e) =
            hitt_core::storage::history_store::save_history(&path, self.history.entries())
        {
            tracing::warn!("Failed to save history: {e:#}");
        }
    }

    /// Add an entry (the store enforces `history_limit`) and persist.
    pub fn record_history(&mut self, entry: hitt_core::core::history::HistoryEntry) {
        self.history.add(entry);
        self.save_history();
    }

    /// Clear the in-memory history and delete the history file.
    pub fn clear_history(&mut self) {
        self.history.clear();
        if let Err(e) = hitt_core::storage::history_store::clear_history(&self.history_path()) {
            tracing::warn!("Failed to delete history file: {e:#}");
        }
    }

    /// Open history entry `index` (0 = newest) in a new tab without sending:
    /// the request snapshot (or method + URL for old entries) plus the stored
    /// response, if any. Returns false if the index is out of range.
    pub fn open_history_entry(&mut self, index: usize) -> bool {
        let Some(entry) = self.history.entries().get(index) else {
            return false;
        };
        let (request, collection_index) = if entry.request.is_some() {
            (entry.to_request(), None)
        } else {
            // Old entries without a snapshot: prefer the live collection request.
            entry
                .request_id
                .and_then(|rid| {
                    self.collections.iter().enumerate().find_map(|(ci, coll)| {
                        coll.find_request(&rid).map(|r| (r.clone(), Some(ci)))
                    })
                })
                .unwrap_or_else(|| (entry.to_request(), None))
        };
        let response = entry.to_response();
        let mut tab = RequestTab::from_request(request, collection_index);
        tab.response = response;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.response_scroll = 0;
        self.request_scroll = 0;
        self.focus = FocusArea::UrlBar;
        true
    }

    /// Open history entry `index` in a new tab and send it right away.
    pub async fn replay_history_entry(&mut self, index: usize) -> Result<bool> {
        if !self.open_history_entry(index) {
            return Ok(false);
        }
        // Drop the stale stored response so the tab shows the new one.
        self.active_tab_mut().response = None;
        self.send_request().await?;
        Ok(true)
    }

    /// Cancel the active tab's in-flight HTTP request or pending WebSocket/SSE
    /// handshake. Returns false if nothing was pending.
    pub fn cancel_active_request(&mut self) -> bool {
        if self.cancel_pending_connection() {
            return true;
        }
        if !self.tabs[self.active_tab].abort_pending_request() {
            return false;
        }
        self.refresh_loading();
        self.notify("Request cancelled".into(), NotificationKind::Info);
        true
    }

    /// Cancel the active tab's WebSocket/SSE connection if it is still
    /// connecting. Returns false if no handshake was pending.
    fn cancel_pending_connection(&mut self) -> bool {
        use hitt_core::protocols::sse::SseStatus;
        use hitt_core::protocols::websocket::{WsCommand as WsCmd, WsStatus};

        let tab = &mut self.tabs[self.active_tab];
        if let Some(session) = tab.ws_session.as_mut() {
            if session.status == WsStatus::Connecting {
                // Disconnect (or dropping the sender) aborts the handshake task.
                if let Some(tx) = tab.ws_cmd_sender.take() {
                    let _ = tx.send(WsCmd::Disconnect);
                }
                session.status = WsStatus::Disconnected;
                self.notify("Connection cancelled".into(), NotificationKind::Info);
                return true;
            }
        }
        if let Some(session) = tab.sse_session.as_mut() {
            if session.status == SseStatus::Connecting {
                if let Some(tx) = tab.sse_cmd_sender.take() {
                    let _ = tx.send(SseCommand::Disconnect);
                }
                session.status = SseStatus::Disconnected;
                self.notify("Connection cancelled".into(), NotificationKind::Info);
                return true;
            }
        }
        false
    }

    /// Recompute the global `loading` flag from per-tab pending requests.
    pub fn refresh_loading(&mut self) {
        self.loading = self.tabs.iter().any(|t| t.pending_request.is_some());
    }

    /// Connect the active tab's WebSocket, or disconnect / cancel it if it is
    /// already connected / connecting.
    ///
    /// Never blocks on the network: the handshake runs on a background task
    /// (see `hitt_core::protocols::websocket::connect`) and its outcome arrives
    /// as `AppEvent::WebSocketEvent`. Until then the session shows `Connecting`.
    pub async fn toggle_ws_connection(&mut self) -> Result<()> {
        use hitt_core::protocols::websocket::{WsCommand as WsCmd, WsStatus};

        if self.cancel_pending_connection() {
            return Ok(());
        }
        let tab = &self.tabs[self.active_tab];
        // If already connected -> disconnect
        if let Some(ref session) = tab.ws_session {
            if let WsStatus::Connected { .. } = session.status {
                if let Some(tx) = self.tabs[self.active_tab].ws_cmd_sender.take() {
                    let _ = tx.send(WsCmd::Disconnect);
                }
                if let Some(ref mut s) = self.tabs[self.active_tab].ws_session {
                    s.status = WsStatus::Disconnected;
                }
                self.notify("Disconnected".into(), NotificationKind::Info);
                return Ok(());
            }
        }

        // Connect
        let url = self.tabs[self.active_tab].request.url.trim().to_string();
        if url.is_empty() {
            self.notify(
                "Enter a WebSocket URL first".into(),
                NotificationKind::Warning,
            );
            return Ok(());
        }

        let mut session = WebSocketSession::new(&url);
        session.status = hitt_core::protocols::websocket::WsStatus::Connecting;
        let session_id = session.id;
        self.tabs[self.active_tab].ws_session = Some(session);
        self.tabs[self.active_tab].ws_message_scroll = ListScroll::default();
        self.tabs[self.active_tab].ws_message_input.clear();
        self.tabs[self.active_tab].response_tab = ResponseTabKind::WsMessages;

        // Spawn bridge task
        let (ws_event_tx, mut ws_event_rx) = mpsc::unbounded_channel();
        if let Some(ref app_tx) = self.event_sender {
            let app_tx = app_tx.clone();
            tokio::spawn(async move {
                while let Some(ws_evt) = ws_event_rx.recv().await {
                    let data = match ws_evt {
                        hitt_core::protocols::websocket::WsEvent::Connected => {
                            crate::event::WsEventData::Connected
                        }
                        hitt_core::protocols::websocket::WsEvent::Disconnected => {
                            crate::event::WsEventData::Disconnected
                        }
                        hitt_core::protocols::websocket::WsEvent::MessageReceived(msg) => {
                            crate::event::WsEventData::MessageReceived(msg)
                        }
                        hitt_core::protocols::websocket::WsEvent::Error(e) => {
                            crate::event::WsEventData::Error(e)
                        }
                    };
                    if app_tx
                        .send(crate::event::AppEvent::WebSocketEvent {
                            session_id,
                            event: data,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }

        let headers: Vec<hitt_core::core::request::KeyValuePair> = self.tabs[self.active_tab]
            .request
            .headers
            .iter()
            .filter(|h| h.enabled)
            .cloned()
            .collect();

        // Returns as soon as the handshake task is spawned.
        match hitt_core::protocols::websocket::connect(&url, &headers, ws_event_tx).await {
            Ok(cmd_tx) => {
                self.tabs[self.active_tab].ws_cmd_sender = Some(cmd_tx);
                self.notify("Connecting… (Esc to cancel)".into(), NotificationKind::Info);
            }
            Err(e) => {
                if let Some(ref mut s) = self.tabs[self.active_tab].ws_session {
                    s.status = hitt_core::protocols::websocket::WsStatus::Error(e.to_string());
                }
                self.notify(
                    format!("WebSocket connect failed: {e}"),
                    NotificationKind::Error,
                );
            }
        }

        self.focus = FocusArea::ResponseBody;
        Ok(())
    }

    #[allow(clippy::unused_async)]
    pub async fn toggle_sse_connection(&mut self) -> Result<()> {
        use hitt_core::protocols::sse::SseStatus;

        if self.cancel_pending_connection() {
            return Ok(());
        }
        let tab = &self.tabs[self.active_tab];
        // If already connected -> disconnect
        if let Some(ref session) = tab.sse_session {
            if session.status == SseStatus::Connected {
                if let Some(tx) = self.tabs[self.active_tab].sse_cmd_sender.take() {
                    let _ = tx.send(SseCommand::Disconnect);
                }
                if let Some(ref mut s) = self.tabs[self.active_tab].sse_session {
                    s.status = SseStatus::Disconnected;
                }
                self.notify("Disconnected".into(), NotificationKind::Info);
                return Ok(());
            }
        }

        // Connect
        let url = self.tabs[self.active_tab].request.url.trim().to_string();
        if url.is_empty() {
            self.notify("Enter an SSE URL first".into(), NotificationKind::Warning);
            return Ok(());
        }

        let mut session = SseSession::new(&url);
        session.status = SseStatus::Connecting;
        let session_id = session.id;
        self.tabs[self.active_tab].sse_session = Some(session);
        self.tabs[self.active_tab].sse_event_scroll = ListScroll::default();
        self.tabs[self.active_tab].sse_show_accumulated = false;
        self.tabs[self.active_tab].response_tab = ResponseTabKind::SseEvents;

        // Spawn bridge task
        let (sse_event_tx, mut sse_event_rx) = mpsc::unbounded_channel();
        if let Some(ref app_tx) = self.event_sender {
            let app_tx = app_tx.clone();
            tokio::spawn(async move {
                while let Some(sse_out) = sse_event_rx.recv().await {
                    let data = match sse_out {
                        hitt_core::protocols::sse::SseOutput::Connected => {
                            crate::event::SseEventData::Connected
                        }
                        hitt_core::protocols::sse::SseOutput::Disconnected => {
                            crate::event::SseEventData::Disconnected
                        }
                        hitt_core::protocols::sse::SseOutput::Event(evt) => {
                            crate::event::SseEventData::Event(evt)
                        }
                        hitt_core::protocols::sse::SseOutput::Error(e) => {
                            crate::event::SseEventData::Error(e)
                        }
                    };
                    if app_tx
                        .send(crate::event::AppEvent::SseEvent {
                            session_id,
                            event: data,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }

        let headers: Vec<hitt_core::core::request::KeyValuePair> = self.tabs[self.active_tab]
            .request
            .headers
            .iter()
            .filter(|h| h.enabled)
            .cloned()
            .collect();

        match hitt_core::protocols::sse::connect(&url, &headers, sse_event_tx) {
            Ok(cmd_tx) => {
                self.tabs[self.active_tab].sse_cmd_sender = Some(cmd_tx);
                self.notify("Connecting… (Esc to cancel)".into(), NotificationKind::Info);
            }
            Err(e) => {
                if let Some(ref mut s) = self.tabs[self.active_tab].sse_session {
                    s.status = SseStatus::Error(e.to_string());
                }
                self.notify(format!("SSE connect failed: {e}"), NotificationKind::Error);
            }
        }

        self.focus = FocusArea::ResponseBody;
        Ok(())
    }

    pub fn notify(&mut self, message: String, kind: NotificationKind) {
        self.notification = Some(Notification {
            message,
            kind,
            created_at: std::time::Instant::now(),
        });
    }

    pub fn clear_expired_notification(&mut self) {
        if let Some(n) = &self.notification {
            if n.created_at.elapsed() > std::time::Duration::from_secs(3) {
                self.notification = None;
            }
        }
    }

    /// Panel-mode Tab order, top to bottom as on screen: URL → request tabs →
    /// request body → response tabs → response body → sidebar.
    pub fn cycle_focus_forward(&mut self) {
        self.focus = match self.focus {
            FocusArea::Sidebar => FocusArea::UrlBar,
            FocusArea::UrlBar => FocusArea::RequestTabs,
            FocusArea::RequestTabs => FocusArea::RequestBody,
            FocusArea::RequestBody => FocusArea::ResponseTabs,
            FocusArea::ResponseTabs => FocusArea::ResponseBody,
            FocusArea::ResponseBody => FocusArea::Sidebar,
            other => other,
        };
    }

    pub fn cycle_focus_backward(&mut self) {
        self.focus = match self.focus {
            FocusArea::Sidebar => FocusArea::ResponseBody,
            FocusArea::UrlBar => FocusArea::Sidebar,
            FocusArea::RequestTabs => FocusArea::UrlBar,
            FocusArea::RequestBody => FocusArea::RequestTabs,
            FocusArea::ResponseTabs => FocusArea::RequestBody,
            FocusArea::ResponseBody => FocusArea::ResponseTabs,
            other => other,
        };
    }

    // ── Global navigation helpers ────────────────────────────────

    /// Set focus to a panel and track last-focused right-column area.
    pub fn navigate_to_panel(&mut self, target: FocusArea) {
        self.focus = target;
        if matches!(
            target,
            FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody
        ) {
            self.last_right_focus = target;
        }
    }

    /// Global nav: Sidebar → last right-column panel.
    pub fn global_nav_right(&mut self) {
        if self.focus == FocusArea::Sidebar {
            self.focus = self.last_right_focus;
        }
    }

    /// Global nav: any right-column area → Sidebar (saves last_right_focus).
    pub fn global_nav_left(&mut self) {
        if self.focus != FocusArea::Sidebar {
            if matches!(
                self.focus,
                FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody
            ) {
                self.last_right_focus = self.focus;
            }
            self.focus = FocusArea::Sidebar;
        }
    }

    /// Global nav: move down in the right column (UrlBar → RequestBody → ResponseBody).
    /// Sidebar stays on Sidebar.
    pub fn global_nav_down(&mut self) {
        self.focus = match self.focus {
            FocusArea::UrlBar | FocusArea::RequestTabs => FocusArea::RequestBody,
            FocusArea::RequestBody | FocusArea::ResponseTabs => FocusArea::ResponseBody,
            other => other,
        };
        if matches!(
            self.focus,
            FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody
        ) {
            self.last_right_focus = self.focus;
        }
    }

    /// Global nav: move up in the right column (ResponseBody → RequestBody → UrlBar).
    /// Sidebar stays on Sidebar.
    pub fn global_nav_up(&mut self) {
        self.focus = match self.focus {
            FocusArea::ResponseBody => FocusArea::RequestBody,
            FocusArea::RequestBody | FocusArea::RequestTabs => FocusArea::UrlBar,
            FocusArea::ResponseTabs => FocusArea::ResponseBody,
            other => other,
        };
        if matches!(
            self.focus,
            FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody
        ) {
            self.last_right_focus = self.focus;
        }
    }

    /// Cycle through 4 major panels: Sidebar → UrlBar → RequestBody → ResponseBody → Sidebar.
    pub fn cycle_major_focus_forward(&mut self) {
        self.focus = match self.focus {
            FocusArea::Sidebar => FocusArea::UrlBar,
            FocusArea::UrlBar => FocusArea::RequestBody,
            FocusArea::RequestBody | FocusArea::RequestTabs => FocusArea::ResponseBody,
            FocusArea::ResponseBody | FocusArea::ResponseTabs => FocusArea::Sidebar,
            other => other,
        };
    }

    /// Reverse cycle through 4 major panels.
    pub fn cycle_major_focus_backward(&mut self) {
        self.focus = match self.focus {
            FocusArea::Sidebar => FocusArea::ResponseBody,
            FocusArea::UrlBar => FocusArea::Sidebar,
            FocusArea::RequestBody | FocusArea::RequestTabs => FocusArea::UrlBar,
            FocusArea::ResponseBody | FocusArea::ResponseTabs => FocusArea::RequestBody,
            other => other,
        };
    }

    /// Snap focus from sub-areas to their parent major panel.
    /// RequestTabs → RequestBody, ResponseTabs → ResponseBody.
    pub fn snap_focus_to_major_panel(&mut self) {
        self.focus = match self.focus {
            FocusArea::RequestTabs => FocusArea::RequestBody,
            FocusArea::ResponseTabs => FocusArea::ResponseBody,
            other => other,
        };
    }
}

/// An HTTP request running in the background for a tab.
#[derive(Debug, Clone)]
pub struct PendingRequest {
    /// Matches the `AppEvent::HttpResponse` token; stale results are ignored.
    pub token: Uuid,
    pub started_at: std::time::Instant,
    /// `None` when the request runs inline (no event loop attached).
    pub abort: Option<tokio::task::AbortHandle>,
}

#[derive(Debug, Clone)]
pub struct RequestTab {
    pub id: Uuid,
    pub request: Request,
    pub response: Option<Response>,
    pub request_tab: RequestTabKind,
    pub response_tab: ResponseTabKind,
    pub collection_index: Option<usize>,
    pub dirty: bool,
    // Per-tab WS/SSE session state
    pub ws_session: Option<WebSocketSession>,
    pub sse_session: Option<SseSession>,
    pub ws_cmd_sender: Option<mpsc::UnboundedSender<WsCommand>>,
    pub sse_cmd_sender: Option<mpsc::UnboundedSender<SseCommand>>,
    pub ws_message_input: String,
    pub ws_message_scroll: ListScroll,
    /// Events list scroll; its `offset` also scrolls the accumulated-text view.
    pub sse_event_scroll: ListScroll,
    pub sse_show_accumulated: bool,
    /// Wrapped row count of the accumulated-text view at its last render
    /// (bounds its scrolling; the renderer keeps it up to date).
    pub sse_stream_rows: usize,
    /// In-flight HTTP request started from this tab, if any.
    pub pending_request: Option<PendingRequest>,
    /// Error of the last send, shown in the response area until the next
    /// successful response (the stale response is cleared on failure).
    pub last_error: Option<String>,
    /// Response body filter (`/` on the response body) and current match.
    pub response_filter: String,
    pub response_filter_match_idx: usize,
}

impl RequestTab {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            request: Request::new("New Request", HttpMethod::GET, ""),
            response: None,
            request_tab: RequestTabKind::Params,
            response_tab: ResponseTabKind::Body,
            collection_index: None,
            dirty: false,
            ws_session: None,
            sse_session: None,
            ws_cmd_sender: None,
            sse_cmd_sender: None,
            ws_message_input: String::new(),
            ws_message_scroll: ListScroll::default(),
            sse_event_scroll: ListScroll::default(),
            sse_show_accumulated: false,
            sse_stream_rows: 0,
            pending_request: None,
            last_error: None,
            response_filter: String::new(),
            response_filter_match_idx: 0,
        }
    }

    pub fn from_request(request: Request, collection_index: Option<usize>) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
            response: None,
            request_tab: RequestTabKind::Params,
            response_tab: ResponseTabKind::Body,
            collection_index,
            dirty: false,
            ws_session: None,
            sse_session: None,
            ws_cmd_sender: None,
            sse_cmd_sender: None,
            ws_message_input: String::new(),
            ws_message_scroll: ListScroll::default(),
            sse_event_scroll: ListScroll::default(),
            sse_show_accumulated: false,
            sse_stream_rows: 0,
            pending_request: None,
            last_error: None,
            response_filter: String::new(),
            response_filter_match_idx: 0,
        }
    }

    /// Abort the in-flight HTTP request, if any. Returns true if one was pending.
    pub fn abort_pending_request(&mut self) -> bool {
        match self.pending_request.take() {
            Some(pending) => {
                if let Some(handle) = pending.abort {
                    handle.abort();
                }
                true
            }
            None => false,
        }
    }

    /// True for a fresh tab the user hasn't touched (safe to reuse).
    pub fn is_blank(&self) -> bool {
        !self.dirty
            && self.collection_index.is_none()
            && self.request.url.is_empty()
            && self.response.is_none()
            && self.ws_session.is_none()
            && self.sse_session.is_none()
            && self.pending_request.is_none()
            && self.last_error.is_none()
    }

    /// Request name (or method + URL), without the unsaved marker (the
    /// header tab bar adds `DIRTY_MARKER` for dirty tabs).
    pub fn title(&self) -> String {
        if self.request.name.is_empty() {
            format!("{} {}", self.request.method, self.request.url)
        } else {
            self.request.name.clone()
        }
    }
}

impl RequestTab {
    /// Prefix shown before the header-bar title of tabs with unsaved edits.
    pub const DIRTY_MARKER: &'static str = "● ";
}

/// Names of `{{variables}}` still unresolved in what `request` would send
/// (URL, enabled params / headers, body, auth), deduplicated in order.
fn unresolved_variables(request: &Request, resolver: &VariableResolver) -> Vec<String> {
    use hitt_core::core::auth::AuthConfig;
    use hitt_core::core::request::RequestBody;

    static VAR_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\{\{([^}]+)\}\}").expect("valid variable regex")
    });

    let mut texts: Vec<&str> = vec![&request.url];
    let enabled = |kv: &&hitt_core::core::request::KeyValuePair| kv.enabled;
    for kv in request
        .params
        .iter()
        .chain(&request.headers)
        .filter(enabled)
    {
        texts.extend([kv.key.as_str(), kv.value.as_str()]);
    }
    match &request.body {
        Some(RequestBody::Json(s) | RequestBody::Raw { content: s, .. }) => texts.push(s),
        Some(RequestBody::GraphQL { query, variables }) => {
            texts.push(query);
            texts.extend(variables.as_deref());
        }
        Some(RequestBody::FormUrlEncoded(pairs) | RequestBody::FormData(pairs)) => {
            for kv in pairs.iter().filter(enabled) {
                texts.extend([kv.key.as_str(), kv.value.as_str()]);
            }
        }
        _ => {}
    }
    match &request.auth {
        Some(AuthConfig::Bearer { token }) => texts.push(token),
        Some(AuthConfig::Basic { username, password }) => {
            texts.extend([username.as_str(), password.as_str()]);
        }
        Some(AuthConfig::ApiKey { key, value, .. }) => {
            texts.extend([key.as_str(), value.as_str()]);
        }
        Some(AuthConfig::OAuth2 {
            access_token_url,
            client_id,
            client_secret,
            scope,
            token,
            ..
        }) => {
            texts.extend([
                access_token_url.as_str(),
                client_id.as_str(),
                client_secret.as_str(),
            ]);
            texts.extend(scope.as_deref());
            texts.extend(token.as_deref());
        }
        _ => {}
    }

    let mut names: Vec<String> = Vec::new();
    for text in texts.into_iter().filter(|t| t.contains("{{")) {
        let resolved = resolver.resolve(text);
        for caps in VAR_RE.captures_iter(&resolved) {
            let name = caps[1].trim().to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

impl Default for RequestTab {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestTabKind {
    Params,
    Auth,
    Headers,
    Body,
    Assertions,
}

impl RequestTabKind {
    pub fn all() -> &'static [Self] {
        &[
            Self::Params,
            Self::Auth,
            Self::Headers,
            Self::Body,
            Self::Assertions,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Params => "Params",
            Self::Auth => "Auth",
            Self::Headers => "Headers",
            Self::Body => "Body",
            Self::Assertions => "Assertions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseTabKind {
    Body,
    Headers,
    Cookies,
    Timing,
    Assertions,
    // WS-specific
    WsMessages,
    WsInfo,
    // SSE-specific
    SseEvents,
    SseStream,
    SseInfo,
}

impl ResponseTabKind {
    pub fn all() -> &'static [Self] {
        &[
            Self::Body,
            Self::Headers,
            Self::Cookies,
            Self::Timing,
            Self::Assertions,
        ]
    }

    pub fn for_protocol(protocol: &hitt_core::core::request::Protocol) -> &'static [Self] {
        match protocol {
            hitt_core::core::request::Protocol::WebSocket => &[Self::WsMessages, Self::WsInfo],
            hitt_core::core::request::Protocol::Sse => {
                &[Self::SseEvents, Self::SseStream, Self::SseInfo]
            }
            hitt_core::core::request::Protocol::Http
            | hitt_core::core::request::Protocol::Grpc { .. } => Self::all(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Headers => "Headers",
            Self::Cookies => "Cookies",
            Self::Timing => "Timing",
            Self::Assertions => "Assertions",
            Self::WsMessages => "Messages",
            Self::WsInfo | Self::SseInfo => "Info",
            Self::SseEvents => "Events",
            Self::SseStream => "Stream",
        }
    }

    /// Compact label used when the response tab bar is too narrow for `label()`.
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Headers => "Hdrs",
            Self::Cookies => "Cook",
            Self::Timing => "Time",
            Self::Assertions => "Tests",
            Self::WsMessages => "Msgs",
            Self::SseEvents => "Evts",
            other => other.label(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
    Global,
    Panel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    Insert,
    Modal(ModalKind),
    Command,
    ChainEditor,
    ProxyInspector,
}

impl std::fmt::Display for AppMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "NORMAL"),
            Self::Insert => write!(f, "INSERT"),
            Self::Modal(kind) => write!(f, "MODAL({kind:?})"),
            Self::Command => write!(f, "COMMAND"),
            Self::ChainEditor => write!(f, "CHAIN"),
            Self::ProxyInspector => write!(f, "PROXY"),
        }
    }
}

/// Destructive action awaiting confirmation in a `ModalKind::Confirm` dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfirmAction {
    DeleteRequest {
        coll_idx: usize,
        request_id: Uuid,
    },
    /// Close the tab with this id (it has unsaved changes).
    CloseTab(Uuid),
    Quit,
    /// Delete the environment with this id (and its file, unless session-only).
    DeleteEnvironment(Uuid),
    /// Delete the collection with this id (and its file).
    DeleteCollection(Uuid),
    /// Delete chain `chain_id` from the collection with id `coll_id`.
    DeleteChain {
        coll_id: Uuid,
        chain_id: Uuid,
    },
    /// Reset the tab with this id to a blank request (it has unsaved changes).
    ResetTab(Uuid),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModalKind {
    Search,
    Help,
    Confirm(String),
    Import,
    Export,
    DiffSelector,
    RenameTab,
    CollectionPicker,
    RenameCollection(usize),
    RenameRequest {
        coll_idx: usize,
        request_id: uuid::Uuid,
    },
    ThemePicker,
    /// Variables of the active environment (or the tab's collection).
    EnvironmentEdit,
    /// Collection runner progress / report (`:run`).
    Runner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    Sidebar,
    UrlBar,
    RequestTabs,
    RequestBody,
    ResponseBody,
    ResponseTabs,
    ChainSteps,
    ProxyList,
}

impl std::fmt::Display for FocusArea {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sidebar => write!(f, "Sidebar"),
            Self::UrlBar => write!(f, "URL Bar"),
            Self::RequestTabs => write!(f, "Request Tabs"),
            Self::RequestBody => write!(f, "Request Body"),
            Self::ResponseBody => write!(f, "Response Body"),
            Self::ResponseTabs => write!(f, "Response Tabs"),
            Self::ChainSteps => write!(f, "Chain Steps"),
            Self::ProxyList => write!(f, "Proxy List"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub kind: NotificationKind,
    pub created_at: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NotificationKind {
    Info,
    Success,
    Warning,
    Error,
}

impl std::fmt::Display for NotificationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Success => write!(f, "SUCCESS"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SidebarState {
    pub selected: usize,
    pub expanded: std::collections::HashSet<Uuid>,
    pub section: SidebarSection,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarSection {
    #[default]
    Collections,
    Chains,
    History,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub name: String,
    pub method: Option<HttpMethod>,
    pub url: String,
    pub collection_name: Option<String>,
    pub request_id: Uuid,
    pub collection_index: usize,
}
