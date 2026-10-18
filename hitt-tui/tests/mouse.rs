//! Mouse handling: events are fed through `handle_event` after rendering with a
//! `TestBackend`, so the clickable regions come from the real layout.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tempfile::TempDir;

use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::request::{HttpMethod, KeyValuePair, Protocol, Request, RequestBody};
use hitt_core::protocols::sse::{SseEvent, SseSession};
use hitt_core::protocols::websocket::{MessageDirection, WebSocketSession, WsContent, WsMessage};
use hitt_core::storage::config::AppConfig;
use hitt_tui::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, RequestTabKind, ResponseTabKind, SidebarSection,
};
use hitt_tui::event::{handle_event, handle_ws_protocol_event, AppEvent, WsEventData};

const W: u16 = 120;
const H: u16 = 40;

fn test_app() -> (App, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    (App::new(config).unwrap(), tmp)
}

fn draw(app: &mut App) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    terminal
        .draw(|frame| hitt_tui::ui::layout::render(app, frame))
        .unwrap();
    terminal
}

/// Text rendered in `rect` (single row).
fn text_at(terminal: &Terminal<TestBackend>, rect: Rect) -> String {
    let buf = terminal.backend().buffer();
    (rect.x..rect.x + rect.width)
        .map(|x| buf[(x, rect.y)].symbol().to_string())
        .collect()
}

fn mouse(kind: MouseEventKind, col: u16, row: u16) -> AppEvent {
    AppEvent::Mouse(MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

async fn click(app: &mut App, col: u16, row: u16) {
    handle_event(
        app,
        mouse(MouseEventKind::Down(MouseButton::Left), col, row),
    )
    .await
    .unwrap();
}

async fn click_rect(app: &mut App, rect: Rect) {
    click(app, rect.x + rect.width / 2, rect.y + rect.height / 2).await;
}

async fn wheel(app: &mut App, col: u16, row: u16, up: bool) {
    let kind = if up {
        MouseEventKind::ScrollUp
    } else {
        MouseEventKind::ScrollDown
    };
    handle_event(app, mouse(kind, col, row)).await.unwrap();
}

async fn press(app: &mut App, code: KeyCode) {
    handle_event(app, AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .await
        .unwrap();
}

fn center(rect: Rect) -> (u16, u16) {
    (rect.x + rect.width / 2, rect.y + rect.height / 2)
}

/// An app with one expanded collection holding `n` requests `Req 0..n`.
fn app_with_requests(n: usize) -> (App, TempDir) {
    let (mut app, tmp) = test_app();
    let mut coll = Collection::new("API");
    for i in 0..n {
        coll.add_request(Request::new(
            format!("Req {i}"),
            HttpMethod::GET,
            format!("/r{i}"),
        ));
    }
    app.sidebar_state.expanded.insert(coll.id);
    app.collections.push(coll);
    (app, tmp)
}

// ════════════════════════════════════════════════════════════════════════════
// Modals swallow clicks
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn click_behind_confirm_cancels_and_does_not_reach_panels() {
    let (mut app, _tmp) = app_with_requests(1);
    app.focus = FocusArea::Sidebar;
    app.sidebar_state.selected = 1;
    press(&mut app, KeyCode::Char('x')).await;
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    draw(&mut app);

    // The method selector sits behind / outside the dialog.
    let method = app.regions.method_selector.unwrap();
    let (x, y) = center(method);
    assert!(!hit(app.regions.overlay.unwrap(), x, y));
    click(&mut app, x, y).await;
    assert_eq!(app.mode, AppMode::Normal, "outside click closes the dialog");
    assert!(app.pending_confirm.is_none());
    assert_eq!(app.active_tab().request.method, HttpMethod::GET);

    // A later `y` is not a confirmation and is not typed anywhere.
    press(&mut app, KeyCode::Char('y')).await;
    assert_eq!(app.collections[0].request_count(), 1);
    assert!(app.active_tab().request.url.is_empty());
}

#[tokio::test]
async fn confirm_buttons_are_clickable() {
    let (mut app, _tmp) = app_with_requests(1);
    app.focus = FocusArea::Sidebar;
    app.sidebar_state.selected = 1;

    press(&mut app, KeyCode::Char('x')).await;
    let terminal = draw(&mut app);
    let no = app.regions.confirm_no.expect("no button region");
    assert!(text_at(&terminal, no).contains("[N]o"));
    click_rect(&mut app, no).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.collections[0].request_count(), 1);

    press(&mut app, KeyCode::Char('x')).await;
    let terminal = draw(&mut app);
    let yes = app.regions.confirm_yes.expect("yes button region");
    assert!(text_at(&terminal, yes).contains("[Y]es"));
    click_rect(&mut app, yes).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.collections[0].request_count(), 0);
}

#[tokio::test]
async fn click_inside_help_does_not_reach_url_bar() {
    let (mut app, _tmp) = test_app();
    press(&mut app, KeyCode::Char('?')).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Help));
    draw(&mut app);

    let (x, y) = center(app.regions.url_bar.unwrap());
    assert!(
        hit(app.regions.overlay.unwrap(), x, y),
        "help covers the URL"
    );
    click(&mut app, x, y).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Help));
    assert_ne!(app.focus, FocusArea::UrlBar);

    // Wheel scrolls the help text.
    wheel(&mut app, x, y, false).await;
    assert!(app.help_scroll > 0);
    wheel(&mut app, x, y, true).await;
    assert_eq!(app.help_scroll, 0);

    // Outside click closes it.
    click(&mut app, 0, 0).await;
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn click_outside_command_palette_closes_it() {
    let (mut app, _tmp) = test_app();
    press(&mut app, KeyCode::Char(':')).await;
    assert_eq!(app.mode, AppMode::Command);
    draw(&mut app);
    click(&mut app, W - 1, H - 2).await;
    assert_eq!(app.mode, AppMode::Normal);
}

fn hit(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

// ════════════════════════════════════════════════════════════════════════════
// Modal lists
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn click_search_result_opens_it() {
    let (mut app, _tmp) = app_with_requests(3);
    press(&mut app, KeyCode::Char('/')).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Search));
    for c in "Req".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    assert_eq!(app.search_results.len(), 3);
    draw(&mut app);
    let wanted = app.search_results[2].name.clone();
    let (rect, idx) = app.regions.modal_items[2];
    assert_eq!(idx, 2);
    click_rect(&mut app, rect).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.active_tab().request.name, wanted);
}

#[tokio::test]
async fn click_search_result_already_open_switches_instead_of_duplicating() {
    let (mut app, _tmp) = app_with_requests(3);
    for _ in 0..2 {
        press(&mut app, KeyCode::Char('/')).await;
        for c in "Req".chars() {
            press(&mut app, KeyCode::Char(c)).await;
        }
        draw(&mut app);
        let (rect, _) = app.regions.modal_items[1];
        click_rect(&mut app, rect).await;
    }
    let wanted = app.active_tab().request.id;
    let open = app.tabs.iter().filter(|t| t.request.id == wanted).count();
    assert_eq!(open, 1, "the same request must not be opened twice");
}

#[tokio::test]
async fn click_theme_applies_it() {
    let (mut app, _tmp) = test_app();
    app.theme_before_preview = Some(app.theme.clone());
    app.mode = AppMode::Modal(ModalKind::ThemePicker);
    draw(&mut app);
    let (rect, idx) = app.regions.modal_items[2];
    click_rect(&mut app, rect).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.config.theme, hitt_tui::ui::theme::AVAILABLE_THEMES[idx]);
}

#[tokio::test]
async fn click_env_editor_row_selects_it() {
    let (mut app, _tmp) = test_app();
    let mut env = Environment::new("dev");
    for i in 0..4 {
        env.add_variable(format!("k{i}"), format!("v{i}"));
    }
    app.environments.push(env);
    app.active_env = Some(0);
    hitt_tui::event::open_env_editor(&mut app, hitt_tui::editor::VarScope::Environment);
    draw(&mut app);
    let rect = app
        .regions
        .modal_items
        .iter()
        .find(|(_, i)| *i == 3)
        .unwrap()
        .0;
    click_rect(&mut app, rect).await;
    assert_eq!(app.editor.env_selected, 3);
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));
}

#[tokio::test]
async fn click_diff_row_toggles_mark() {
    let (mut app, _tmp) = test_app();
    for body in ["a", "b"] {
        app.new_tab();
        app.active_tab_mut().response = Some(text_response(body));
    }
    app.mode = AppMode::Modal(ModalKind::DiffSelector);
    draw(&mut app);
    let (rect, _) = app.regions.modal_items[1];
    click_rect(&mut app, rect).await;
    assert_eq!(app.diff_state.selected, 1);
    assert_eq!(app.diff_state.marked.len(), 1);
    click_rect(&mut app, rect).await;
    assert!(app.diff_state.marked.is_empty());
}

fn text_response(body: &str) -> hitt_core::core::response::Response {
    hitt_core::core::response::Response {
        id: uuid::Uuid::new_v4(),
        status: 200,
        status_text: "OK".into(),
        headers: Vec::new(),
        body: hitt_core::core::response::ResponseBody::Text(body.into()),
        cookies: Vec::new(),
        timing: hitt_core::core::response::RequestTiming::default(),
        size: hitt_core::core::response::ResponseSize::default(),
        assertion_results: Vec::new(),
        timestamp: chrono::Utc::now(),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Request / response tab bars, header tabs, env indicator
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn request_subtab_regions_match_labels() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut()
        .request
        .headers
        .push(KeyValuePair::new("Accept", "*/*"));
    let terminal = draw(&mut app);
    let labels = ["Params", "Auth", "Headers (1)", "Body", "Assertions"];
    assert_eq!(app.regions.request_tabs.len(), labels.len());
    for ((rect, kind), label) in app.regions.request_tabs.clone().iter().zip(labels) {
        assert_eq!(rect.height, 1);
        assert_eq!(text_at(&terminal, *rect).trim(), label);
        click_rect(&mut app, *rect).await;
        assert_eq!(app.active_tab().request_tab, *kind);
        assert_eq!(app.focus, FocusArea::RequestTabs);
    }

    // Blank space right of the last label does nothing.
    let bar = app.regions.request_tab_bar.unwrap();
    app.active_tab_mut().request_tab = RequestTabKind::Params;
    click(&mut app, bar.x + bar.width - 1, bar.y).await;
    assert_eq!(app.active_tab().request_tab, RequestTabKind::Params);
}

#[tokio::test]
async fn header_tabs_are_label_sized_and_scroll_stays_stable() {
    let (mut app, _tmp) = test_app();
    for i in 0..15 {
        app.new_tab();
        app.active_tab_mut().request.name = format!("Tab{i:02}");
    }
    draw(&mut app);
    let tabs = app.regions.header_tabs.clone();
    assert!(tabs.iter().all(|(r, _)| r.height == 1));

    // Blank space right of the last tab doesn't select it.
    let (last_rect, _) = *tabs.last().unwrap();
    let bar = app.regions.header_tab_bar.unwrap();
    let blank_x = last_rect.x + last_rect.width;
    if blank_x < bar.x + bar.width {
        app.switch_tab(tabs[0].1);
        click(&mut app, bar.x + bar.width - 1, bar.y).await;
        assert_eq!(app.active_tab, tabs[0].1);
    }
    // Rows below the labels are not tab regions.
    app.switch_tab(tabs.last().unwrap().1);
    click(&mut app, tabs[0].0.x, tabs[0].0.y + 1).await;
    assert_eq!(app.active_tab, tabs.last().unwrap().1);

    // Clicking an already visible tab keeps the bar where it is.
    draw(&mut app);
    let before: Vec<usize> = app.regions.header_tabs.iter().map(|(_, i)| *i).collect();
    {
        let rect = app.regions.header_tabs[0].0;
        click_rect(&mut app, rect).await;
    }
    assert_eq!(app.active_tab, before[0]);
    draw(&mut app);
    let after: Vec<usize> = app.regions.header_tabs.iter().map(|(_, i)| *i).collect();
    assert_eq!(before, after);
}

#[tokio::test]
async fn env_indicator_only_text_is_clickable() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("dev"));
    app.environments.push(Environment::new("prod"));
    let terminal = draw(&mut app);
    let env = app.regions.env_selector.unwrap();
    assert_eq!(text_at(&terminal, env).trim(), "ENV: No Env");
    // Blank space left of the text: nothing.
    click(&mut app, env.x - 1, env.y).await;
    assert_eq!(app.active_env, None);
    click_rect(&mut app, env).await;
    assert_eq!(app.active_env, Some(0));
}

// ════════════════════════════════════════════════════════════════════════════
// Sidebar
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn sidebar_keeps_selection_visible_and_clicks_map_to_it() {
    let (mut app, _tmp) = app_with_requests(61);
    app.focus = FocusArea::Sidebar;
    app.nav_mode = NavMode::Panel;
    for _ in 0..60 {
        press(&mut app, KeyCode::Char('j')).await;
        draw(&mut app);
    }
    assert_eq!(app.sidebar_state.selected, 60);
    let (rect, _) = *app
        .regions
        .sidebar_items
        .iter()
        .find(|(_, i)| *i == 60)
        .expect("selected row is visible");
    let terminal = draw(&mut app);
    assert!(text_at(&terminal, rect).contains("Req 59"));

    // Clicking the row opens exactly that request.
    let (top_rect, top_idx) = app.regions.sidebar_items[0];
    assert!(top_idx > 0, "list is scrolled");
    click_rect(&mut app, rect).await;
    assert_eq!(app.active_tab().request.name, "Req 59");
    assert!(text_at(&terminal, top_rect).contains(&format!("Req {}", top_idx - 1)));
}

#[tokio::test]
async fn sidebar_section_click_resets_selection_and_sets_panel_mode() {
    let (mut app, _tmp) = app_with_requests(60);
    app.sidebar_state.selected = 55;
    draw(&mut app);
    assert!(app.sidebar_state.scroll_offset > 0);
    let (rect, _) = *app
        .regions
        .sidebar_section_tabs
        .iter()
        .find(|(_, s)| *s == SidebarSection::History)
        .unwrap();
    click_rect(&mut app, rect).await;
    assert_eq!(app.sidebar_state.section, SidebarSection::History);
    assert_eq!(app.sidebar_state.selected, 0);
    assert_eq!(app.sidebar_state.scroll_offset, 0);
    assert_eq!(app.focus, FocusArea::Sidebar);
    assert_eq!(app.nav_mode, NavMode::Panel);
}

#[tokio::test]
async fn sidebar_wheel_and_blank_rows_never_select_missing_items() {
    let (mut app, _tmp) = app_with_requests(2);
    draw(&mut app);
    // Header + 2 requests: only 3 rows are registered.
    assert_eq!(app.regions.sidebar_items.len(), 3);
    let sidebar = app.regions.sidebar.unwrap();
    let (x, y) = center(sidebar);
    for _ in 0..10 {
        wheel(&mut app, x, y, false).await;
    }
    assert_eq!(app.sidebar_state.selected, 2);

    // Blank row below the items: focus only, selection unchanged.
    let (last, _) = *app.regions.sidebar_items.last().unwrap();
    let tabs_before = app.tabs.len();
    click(&mut app, last.x + 1, last.y + 3).await;
    assert_eq!(app.sidebar_state.selected, 2);
    assert_eq!(app.tabs.len(), tabs_before);
    assert_eq!(app.focus, FocusArea::Sidebar);
}

// ════════════════════════════════════════════════════════════════════════════
// Request tables, insert mode
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn click_param_row_selects_it() {
    let (mut app, _tmp) = test_app();
    for i in 0..3 {
        app.active_tab_mut()
            .request
            .params
            .push(KeyValuePair::new(format!("p{i}"), "v"));
    }
    draw(&mut app);
    let (rect, _) = *app
        .regions
        .request_rows
        .iter()
        .find(|(_, i)| *i == 2)
        .unwrap();
    click_rect(&mut app, rect).await;
    assert_eq!(app.editor.params_selected, 2);
    assert_eq!(app.focus, FocusArea::RequestBody);
    assert_eq!(app.nav_mode, NavMode::Panel);

    // Wheel moves the selection, clamped to the table.
    let (x, y) = center(app.regions.request_body.unwrap());
    wheel(&mut app, x, y, false).await;
    assert_eq!(app.editor.params_selected, 2);
    wheel(&mut app, x, y, true).await;
    assert_eq!(app.editor.params_selected, 1);
}

#[tokio::test]
async fn click_away_from_url_leaves_insert_mode() {
    let (mut app, _tmp) = test_app();
    draw(&mut app);
    {
        let rect = app.regions.url_bar.unwrap();
        click_rect(&mut app, rect).await;
    }
    assert_eq!(app.mode, AppMode::Insert);
    assert_eq!(app.focus, FocusArea::UrlBar);

    {
        let rect = app.regions.response_body.unwrap();
        click_rect(&mut app, rect).await;
    }
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.nav_mode, NavMode::Panel);
    assert_eq!(app.focus, FocusArea::ResponseBody);

    // Typing now doesn't reach the URL.
    press(&mut app, KeyCode::Char('z')).await;
    assert!(app.active_tab().request.url.is_empty());
}

#[tokio::test]
async fn click_url_while_editing_body_closes_the_editor() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.body = Some(RequestBody::Json(String::new()));
    app.active_tab_mut().request_tab = RequestTabKind::Body;
    app.focus = FocusArea::RequestBody;
    app.nav_mode = NavMode::Panel;
    press(&mut app, KeyCode::Char('i')).await;
    assert_eq!(app.mode, AppMode::Insert);
    assert!(app.editor.body.is_some());
    press(&mut app, KeyCode::Char('{')).await;
    draw(&mut app);

    {
        let rect = app.regions.url_bar.unwrap();
        click_rect(&mut app, rect).await;
    }
    assert!(app.editor.body.is_none(), "body editor closed");
    assert!(
        matches!(&app.active_tab().request.body, Some(RequestBody::Json(t)) if t == "{"),
        "typed text kept"
    );
    assert_eq!(app.focus, FocusArea::UrlBar);
    assert_eq!(app.mode, AppMode::Insert);
}

// ════════════════════════════════════════════════════════════════════════════
// WebSocket / SSE scrolling
// ════════════════════════════════════════════════════════════════════════════

fn ws_message(i: usize) -> WsMessage {
    WsMessage {
        direction: MessageDirection::Received,
        content: WsContent::Text(format!("msg {i}")),
        timestamp: chrono::Utc::now(),
    }
}

fn app_with_ws(messages: usize) -> (App, TempDir) {
    let (mut app, tmp) = test_app();
    let tab = app.active_tab_mut();
    tab.request.protocol = Protocol::WebSocket;
    tab.response_tab = ResponseTabKind::WsMessages;
    let mut session = WebSocketSession::new("ws://localhost");
    for i in 0..messages {
        session.push_message(ws_message(i));
    }
    tab.ws_session = Some(session);
    (app, tmp)
}

#[tokio::test]
async fn ws_scroll_has_no_dead_zone_and_follows_bottom() {
    let (mut app, _tmp) = app_with_ws(100);
    draw(&mut app);
    let scroll = app.active_tab().ws_message_scroll;
    let max = 100 - scroll.page;
    assert!(scroll.page > 0);
    assert_eq!(scroll.offset, max, "starts at the bottom");

    let (x, y) = center(app.regions.response_body.unwrap());
    wheel(&mut app, x, y, true).await;
    assert_eq!(app.active_tab().ws_message_scroll.offset, max - 3);
    assert!(!app.active_tab().ws_message_scroll.follow);

    // New messages don't move a scrolled-up view.
    let session_id = app.active_tab().ws_session.as_ref().unwrap().id;
    handle_ws_protocol_event(
        &mut app,
        session_id,
        WsEventData::MessageReceived(ws_message(100)),
    );
    draw(&mut app);
    assert_eq!(app.active_tab().ws_message_scroll.offset, max - 3);

    // Wheel down stops at the bottom and re-enables following.
    for _ in 0..10 {
        wheel(&mut app, x, y, false).await;
    }
    let new_max = 101 - app.active_tab().ws_message_scroll.page;
    assert_eq!(app.active_tab().ws_message_scroll.offset, new_max);
    assert!(app.active_tab().ws_message_scroll.follow);
    handle_ws_protocol_event(
        &mut app,
        session_id,
        WsEventData::MessageReceived(ws_message(101)),
    );
    draw(&mut app);
    assert_eq!(app.active_tab().ws_message_scroll.offset, new_max + 1);
}

#[tokio::test]
async fn ws_keys_k_j_g_shift_g() {
    let (mut app, _tmp) = app_with_ws(100);
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;
    draw(&mut app);
    let max = 100 - app.active_tab().ws_message_scroll.page;

    press(&mut app, KeyCode::Char('k')).await;
    assert_eq!(app.active_tab().ws_message_scroll.offset, max - 1);
    press(&mut app, KeyCode::Char('j')).await;
    press(&mut app, KeyCode::Char('j')).await;
    assert_eq!(app.active_tab().ws_message_scroll.offset, max);
    press(&mut app, KeyCode::Char('g')).await;
    assert_eq!(app.active_tab().ws_message_scroll.offset, 0);
    handle_event(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
    )
    .await
    .unwrap();
    assert_eq!(app.active_tab().ws_message_scroll.offset, max);
    assert!(app.active_tab().ws_message_scroll.follow);
}

#[tokio::test]
async fn sse_wheel_scrolls_events() {
    let (mut app, _tmp) = test_app();
    let tab = app.active_tab_mut();
    tab.request.protocol = Protocol::Sse;
    tab.response_tab = ResponseTabKind::SseEvents;
    let mut session = SseSession::new("http://localhost/events");
    for i in 0..80 {
        session.push_event(SseEvent {
            event_type: None,
            data: format!("event {i}"),
            id: None,
            timestamp: chrono::Utc::now(),
        });
    }
    tab.sse_session = Some(session);
    draw(&mut app);
    let max = 80 - app.active_tab().sse_event_scroll.page;
    assert_eq!(app.active_tab().sse_event_scroll.offset, max);

    let (x, y) = center(app.regions.response_body.unwrap());
    wheel(&mut app, x, y, true).await;
    assert_eq!(app.active_tab().sse_event_scroll.offset, max - 3);
    for _ in 0..5 {
        wheel(&mut app, x, y, false).await;
    }
    assert_eq!(app.active_tab().sse_event_scroll.offset, max);
}

// ════════════════════════════════════════════════════════════════════════════
// Clicks on non-interactive areas leave insert mode
// ════════════════════════════════════════════════════════════════════════════

/// A point of `bar` outside every rect in `items` (e.g. the empty part of a
/// tab strip right of the last tab).
fn blank_point(bar: Rect, items: &[Rect]) -> (u16, u16) {
    (bar.x..bar.x + bar.width)
        .rev()
        .map(|x| (x, bar.y))
        .find(|&(x, y)| !items.iter().any(|r| hit(*r, x, y)))
        .expect("bar has blank space")
}

/// Click the URL (INSERT), then click `target`; typing must not reach the URL.
async fn assert_click_leaves_url_insert(
    app: &mut App,
    target: impl Fn(&App) -> (u16, u16),
    what: &str,
) {
    app.mode = AppMode::Normal;
    app.active_tab_mut().request.url.clear();
    draw(app);
    let url = app.regions.url_bar.unwrap();
    click_rect(app, url).await;
    assert_eq!(app.mode, AppMode::Insert, "{what}: URL click enters insert");
    draw(app);
    let (x, y) = target(app);
    click(app, x, y).await;
    assert_eq!(app.mode, AppMode::Normal, "{what}: click leaves insert");
    press(app, KeyCode::Char('z')).await;
    assert!(
        !app.active_tab().request.url.contains('z'),
        "{what}: typing still reached the URL"
    );
}

#[tokio::test]
async fn clicks_on_blank_areas_leave_url_insert_mode() {
    let (mut app, _tmp) = test_app();
    app.new_tab(); // two header tabs, so the strip has labels and blank space

    assert_click_leaves_url_insert(
        &mut app,
        |app| center(app.regions.status_bar.unwrap()),
        "status bar",
    )
    .await;
    assert_click_leaves_url_insert(
        &mut app,
        |app| {
            let labels: Vec<Rect> = app.regions.header_tabs.iter().map(|(r, _)| *r).collect();
            blank_point(app.regions.header_tab_bar.unwrap(), &labels)
        },
        "header tab strip",
    )
    .await;
    assert_click_leaves_url_insert(
        &mut app,
        |app| {
            let tabs: Vec<Rect> = app.regions.request_tabs.iter().map(|(r, _)| *r).collect();
            blank_point(app.regions.request_tab_bar.unwrap(), &tabs)
        },
        "request tab strip",
    )
    .await;
    assert_click_leaves_url_insert(
        &mut app,
        |app| {
            let tabs: Vec<Rect> = app.regions.response_tabs.iter().map(|(r, _)| *r).collect();
            blank_point(app.regions.response_tab_bar.unwrap(), &tabs)
        },
        "response tab strip",
    )
    .await;
}

#[tokio::test]
async fn env_and_method_clicks_leave_url_insert_mode() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("dev"));

    assert_click_leaves_url_insert(
        &mut app,
        |app| center(app.regions.env_selector.unwrap()),
        "env selector",
    )
    .await;
    assert_eq!(app.active_env, Some(0), "env click still cycles");

    assert_click_leaves_url_insert(
        &mut app,
        |app| center(app.regions.method_selector.unwrap()),
        "method selector",
    )
    .await;
    assert_eq!(
        app.active_tab().request.method,
        HttpMethod::POST,
        "method click still cycles"
    );
}

#[tokio::test]
async fn clicking_the_url_again_keeps_insert_mode() {
    let (mut app, _tmp) = test_app();
    draw(&mut app);
    let url = app.regions.url_bar.unwrap();
    click_rect(&mut app, url).await;
    press(&mut app, KeyCode::Char('a')).await;
    click_rect(&mut app, url).await;
    assert_eq!(app.mode, AppMode::Insert);
    press(&mut app, KeyCode::Char('b')).await;
    assert_eq!(app.active_tab().request.url, "ab");
}
