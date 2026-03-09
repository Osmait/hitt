use anyhow::Result;
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::core::chain::{ChainExecutionState, RequestChain};
use crate::core::client::HttpClient;
use crate::core::collection::Collection;
use crate::core::environment::Environment;
use crate::core::history::HistoryStore;
use crate::core::request::{HttpMethod, Request};
use crate::core::response::Response;
use crate::core::variables::VariableResolver;
use crate::protocols::sse::{SseCommand, SseSession};
use crate::protocols::websocket::{WsCommand, WebSocketSession};
use crate::storage::config::AppConfig;
use crate::ui::theme::Theme;

/// Tracks the screen regions for mouse hit-testing.
/// Updated every render frame by layout.rs.
#[derive(Debug, Clone, Default)]
pub struct ClickableRegions {
    pub sidebar: Option<Rect>,
    pub url_bar: Option<Rect>,
    pub method_selector: Option<Rect>,
    pub send_button: Option<Rect>,
    pub header_tab_bar: Option<Rect>,
    pub request_tab_bar: Option<Rect>,
    pub request_tabs: Vec<(Rect, RequestTabKind)>,
    pub request_body: Option<Rect>,
    pub response_tab_bar: Option<Rect>,
    pub response_tabs: Vec<(Rect, ResponseTabKind)>,
    pub response_body: Option<Rect>,
    pub ws_input_bar: Option<Rect>,
    pub header_tabs: Vec<(Rect, usize)>,       // (rect, tab_index)
    pub new_tab_button: Option<Rect>,
    pub env_selector: Option<Rect>,
    pub sidebar_section_tabs: Vec<(Rect, SidebarSection)>,
    pub sidebar_items: Vec<Rect>,              // one per visible sidebar row
    pub search_results_items: Vec<Rect>,       // one per visible search result
    pub status_bar: Option<Rect>,
}

impl ClickableRegions {
    pub fn clear(&mut self) {
        self.sidebar = None;
        self.url_bar = None;
        self.method_selector = None;
        self.send_button = None;
        self.header_tab_bar = None;
        self.request_tab_bar = None;
        self.request_tabs.clear();
        self.request_body = None;
        self.response_tab_bar = None;
        self.response_tabs.clear();
        self.response_body = None;
        self.ws_input_bar = None;
        self.header_tabs.clear();
        self.new_tab_button = None;
        self.env_selector = None;
        self.sidebar_section_tabs.clear();
        self.sidebar_items.clear();
        self.search_results_items.clear();
        self.status_bar = None;
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
    pub http_client: HttpClient,
    pub should_quit: bool,
    pub sidebar_state: SidebarState,
    pub command_input: String,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_selected: usize,
    pub help_scroll: usize,
    pub loading: bool,
    pub regions: ClickableRegions,
    pub response_scroll: usize,
    pub rename_input: String,
    pub modal_input: String,
    pub collection_picker_selected: usize,
    // Event sender for protocol bridge tasks
    pub event_sender: Option<mpsc::UnboundedSender<crate::event::AppEvent>>,
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self> {
        let theme = Theme::default();
        let http_client = HttpClient::new()?;

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
            history: HistoryStore::new(1000),
            config,
            theme,
            notification: None,
            active_chain: None,
            active_chain_def: None,
            active_chain_coll_idx: None,
            chain_scroll: 0,
            http_client,
            should_quit: false,
            sidebar_state: SidebarState::default(),
            command_input: String::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            help_scroll: 0,
            loading: false,
            regions: ClickableRegions::default(),
            response_scroll: 0,
            rename_input: String::new(),
            modal_input: String::new(),
            collection_picker_selected: 0,
            event_sender: None,
        })
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

    pub fn new_tab(&mut self) {
        self.tabs.push(RequestTab::new());
        self.active_tab = self.tabs.len() - 1;
    }

    pub fn close_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.tabs.remove(self.active_tab);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            }
        }
    }

    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
    }

    pub fn prev_tab(&mut self) {
        if self.active_tab == 0 {
            self.active_tab = self.tabs.len() - 1;
        } else {
            self.active_tab -= 1;
        }
    }

    pub fn active_environment(&self) -> Option<&Environment> {
        self.active_env.and_then(|i| self.environments.get(i))
    }

    pub fn build_resolver(&self) -> VariableResolver {
        let tab = self.active_tab();
        let collection_vars: &[crate::core::request::KeyValuePair] = if let Some(coll_idx) = tab.collection_index {
            if let Some(coll) = self.collections.get(coll_idx) {
                &coll.variables
            } else {
                &[]
            }
        } else {
            &[]
        };

        let chain_vars = self
            .active_chain
            .as_ref()
            .map(|c| &c.extracted_variables);

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
            crate::core::request::Protocol::Http => self.send_http_request().await,
            crate::core::request::Protocol::WebSocket => self.toggle_ws_connection().await,
            crate::core::request::Protocol::Sse => self.toggle_sse_connection().await,
            _ => Ok(()),
        }
    }

    async fn send_http_request(&mut self) -> Result<()> {
        self.loading = true;
        let tab = &self.tabs[self.active_tab];
        let request = tab.request.clone();
        let resolver = self.build_resolver();

        match self.http_client.send(&request, &resolver).await {
            Ok(mut response) => {
                // Run assertions
                let results = crate::testing::assertion_engine::AssertionEngine::run_assertions(
                    &request.assertions,
                    &response,
                );
                response.assertion_results = results;

                // Add to history
                let entry = crate::core::history::HistoryEntry {
                    id: Uuid::new_v4(),
                    method: request.method,
                    url: request.url.clone(),
                    status: Some(response.status),
                    duration_ms: Some(response.timing.total.as_millis() as u64),
                    size_bytes: Some(response.size.total()),
                    timestamp: response.timestamp,
                    collection_id: None,
                    request_id: Some(request.id),
                    response_body: response.body_text().map(|s| s.to_string()),
                    request_body: None,
                };
                self.history.add(entry);

                self.tabs[self.active_tab].response = Some(response);
                self.tabs[self.active_tab].response_tab = ResponseTabKind::Body;
            }
            Err(e) => {
                self.notify(format!("Request failed: {}", e), NotificationKind::Error);
            }
        }
        self.loading = false;
        Ok(())
    }

    pub async fn toggle_ws_connection(&mut self) -> Result<()> {
        use crate::protocols::websocket::{WsStatus, WsCommand as WsCmd};

        let tab = &self.tabs[self.active_tab];
        // If already connected/connecting -> disconnect
        if let Some(ref session) = tab.ws_session {
            match &session.status {
                WsStatus::Connected { .. } | WsStatus::Connecting => {
                    if let Some(ref tx) = self.tabs[self.active_tab].ws_cmd_sender {
                        let _ = tx.send(WsCmd::Disconnect);
                    }
                    self.tabs[self.active_tab].ws_cmd_sender = None;
                    if let Some(ref mut s) = self.tabs[self.active_tab].ws_session {
                        s.status = WsStatus::Disconnected;
                    }
                    self.notify("Disconnected".into(), NotificationKind::Info);
                    return Ok(());
                }
                _ => {}
            }
        }

        // Connect
        let url = self.tabs[self.active_tab].request.url.trim().to_string();
        if url.is_empty() {
            self.notify("Enter a WebSocket URL first".into(), NotificationKind::Warning);
            return Ok(());
        }

        let mut session = WebSocketSession::new(&url);
        session.status = crate::protocols::websocket::WsStatus::Connecting;
        let session_id = session.id;
        self.tabs[self.active_tab].ws_session = Some(session);
        self.tabs[self.active_tab].ws_message_scroll = 0;
        self.tabs[self.active_tab].ws_message_input.clear();
        self.tabs[self.active_tab].response_tab = ResponseTabKind::WsMessages;

        // Spawn bridge task
        let (ws_event_tx, mut ws_event_rx) = mpsc::unbounded_channel();
        if let Some(ref app_tx) = self.event_sender {
            let app_tx = app_tx.clone();
            tokio::spawn(async move {
                while let Some(ws_evt) = ws_event_rx.recv().await {
                    let data = match ws_evt {
                        crate::protocols::websocket::WsEvent::Connected => crate::event::WsEventData::Connected,
                        crate::protocols::websocket::WsEvent::Disconnected => crate::event::WsEventData::Disconnected,
                        crate::protocols::websocket::WsEvent::MessageReceived(msg) => crate::event::WsEventData::MessageReceived(msg),
                        crate::protocols::websocket::WsEvent::Error(e) => crate::event::WsEventData::Error(e),
                    };
                    if app_tx.send(crate::event::AppEvent::WebSocketEvent { session_id, event: data }).is_err() {
                        break;
                    }
                }
            });
        }

        let headers: Vec<crate::core::request::KeyValuePair> = self.tabs[self.active_tab].request.headers.iter()
            .filter(|h| h.enabled)
            .cloned()
            .collect();

        match crate::protocols::websocket::connect(&url, &headers, ws_event_tx).await {
            Ok(cmd_tx) => {
                self.tabs[self.active_tab].ws_cmd_sender = Some(cmd_tx);
            }
            Err(e) => {
                self.notify(format!("WebSocket connect failed: {}", e), NotificationKind::Error);
            }
        }

        self.focus = FocusArea::ResponseBody;
        Ok(())
    }

    pub async fn toggle_sse_connection(&mut self) -> Result<()> {
        use crate::protocols::sse::SseStatus;

        let tab = &self.tabs[self.active_tab];
        // If already connected/connecting -> disconnect
        if let Some(ref session) = tab.sse_session {
            match &session.status {
                SseStatus::Connected | SseStatus::Connecting => {
                    if let Some(ref tx) = self.tabs[self.active_tab].sse_cmd_sender {
                        let _ = tx.send(SseCommand::Disconnect);
                    }
                    self.tabs[self.active_tab].sse_cmd_sender = None;
                    if let Some(ref mut s) = self.tabs[self.active_tab].sse_session {
                        s.status = SseStatus::Disconnected;
                    }
                    self.notify("Disconnected".into(), NotificationKind::Info);
                    return Ok(());
                }
                _ => {}
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
        self.tabs[self.active_tab].sse_event_scroll = 0;
        self.tabs[self.active_tab].sse_show_accumulated = false;
        self.tabs[self.active_tab].response_tab = ResponseTabKind::SseEvents;

        // Spawn bridge task
        let (sse_event_tx, mut sse_event_rx) = mpsc::unbounded_channel();
        if let Some(ref app_tx) = self.event_sender {
            let app_tx = app_tx.clone();
            tokio::spawn(async move {
                while let Some(sse_out) = sse_event_rx.recv().await {
                    let data = match sse_out {
                        crate::protocols::sse::SseOutput::Connected => crate::event::SseEventData::Connected,
                        crate::protocols::sse::SseOutput::Disconnected => crate::event::SseEventData::Disconnected,
                        crate::protocols::sse::SseOutput::Event(evt) => crate::event::SseEventData::Event(evt),
                        crate::protocols::sse::SseOutput::Error(e) => crate::event::SseEventData::Error(e),
                    };
                    if app_tx.send(crate::event::AppEvent::SseEvent { session_id, event: data }).is_err() {
                        break;
                    }
                }
            });
        }

        let headers: Vec<crate::core::request::KeyValuePair> = self.tabs[self.active_tab].request.headers.iter()
            .filter(|h| h.enabled)
            .cloned()
            .collect();

        match crate::protocols::sse::connect(&url, &headers, sse_event_tx).await {
            Ok(cmd_tx) => {
                self.tabs[self.active_tab].sse_cmd_sender = Some(cmd_tx);
            }
            Err(e) => {
                self.notify(format!("SSE connect failed: {}", e), NotificationKind::Error);
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

    pub fn cycle_focus_forward(&mut self) {
        self.focus = match self.focus {
            FocusArea::Sidebar => FocusArea::UrlBar,
            FocusArea::UrlBar => FocusArea::RequestTabs,
            FocusArea::RequestTabs => FocusArea::RequestBody,
            FocusArea::RequestBody => FocusArea::ResponseBody,
            FocusArea::ResponseBody => FocusArea::ResponseTabs,
            FocusArea::ResponseTabs => FocusArea::Sidebar,
            other => other,
        };
    }

    pub fn cycle_focus_backward(&mut self) {
        self.focus = match self.focus {
            FocusArea::Sidebar => FocusArea::ResponseTabs,
            FocusArea::UrlBar => FocusArea::Sidebar,
            FocusArea::RequestTabs => FocusArea::UrlBar,
            FocusArea::RequestBody => FocusArea::RequestTabs,
            FocusArea::ResponseBody => FocusArea::RequestBody,
            FocusArea::ResponseTabs => FocusArea::ResponseBody,
            other => other,
        };
    }

    // ── Global navigation helpers ────────────────────────────────

    /// Set focus to a panel and track last-focused right-column area.
    pub fn navigate_to_panel(&mut self, target: FocusArea) {
        self.focus = target;
        if matches!(target, FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody) {
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
            if matches!(self.focus, FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody) {
                self.last_right_focus = self.focus;
            }
            self.focus = FocusArea::Sidebar;
        }
    }

    /// Global nav: move down in the right column (UrlBar → RequestBody → ResponseBody).
    /// Sidebar stays on Sidebar.
    pub fn global_nav_down(&mut self) {
        self.focus = match self.focus {
            FocusArea::UrlBar => FocusArea::RequestBody,
            FocusArea::RequestBody => FocusArea::ResponseBody,
            FocusArea::RequestTabs => FocusArea::RequestBody,
            FocusArea::ResponseTabs => FocusArea::ResponseBody,
            other => other,
        };
        if matches!(self.focus, FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody) {
            self.last_right_focus = self.focus;
        }
    }

    /// Global nav: move up in the right column (ResponseBody → RequestBody → UrlBar).
    /// Sidebar stays on Sidebar.
    pub fn global_nav_up(&mut self) {
        self.focus = match self.focus {
            FocusArea::ResponseBody => FocusArea::RequestBody,
            FocusArea::RequestBody => FocusArea::UrlBar,
            FocusArea::ResponseTabs => FocusArea::ResponseBody,
            FocusArea::RequestTabs => FocusArea::UrlBar,
            other => other,
        };
        if matches!(self.focus, FocusArea::UrlBar | FocusArea::RequestBody | FocusArea::ResponseBody) {
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
    pub ws_message_scroll: usize,
    pub sse_event_scroll: usize,
    pub sse_show_accumulated: bool,
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
            ws_message_scroll: 0,
            sse_event_scroll: 0,
            sse_show_accumulated: false,
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
            ws_message_scroll: 0,
            sse_event_scroll: 0,
            sse_show_accumulated: false,
        }
    }

    pub fn title(&self) -> String {
        if self.request.name.is_empty() {
            format!("{} {}", self.request.method, self.request.url)
        } else {
            self.request.name.clone()
        }
    }
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

    pub fn for_protocol(protocol: &crate::core::request::Protocol) -> &'static [Self] {
        match protocol {
            crate::core::request::Protocol::WebSocket => &[Self::WsMessages, Self::WsInfo],
            crate::core::request::Protocol::Sse => &[Self::SseEvents, Self::SseStream, Self::SseInfo],
            _ => Self::all(),
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
            Self::WsInfo => "Info",
            Self::SseEvents => "Events",
            Self::SseStream => "Stream",
            Self::SseInfo => "Info",
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

#[derive(Debug, Clone, PartialEq)]
pub enum ModalKind {
    Search,
    EnvironmentEdit,
    Help,
    Confirm(String),
    Import,
    Export,
    LoadTestConfig,
    DiffSelector,
    CurlImport,
    RenameTab,
    CollectionPicker,
    RenameCollection(usize),
    RenameRequest { coll_idx: usize, request_id: uuid::Uuid },
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
