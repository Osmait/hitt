//! Regression tests for QA-reported rendering / performance bugs: live SSE
//! stream view, selected-row contrast, sticky list scrolling, cut-off
//! status texts, scroll bounds (timing, help, diff) and stale key hints.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tempfile::TempDir;

use hitt_core::core::chain::{ChainExecutionState, ChainStepStatus, RequestChain};
use hitt_core::core::collection::{Collection, CollectionItem};
use hitt_core::core::environment::Environment;
use hitt_core::core::history::HistoryEntry;
use hitt_core::core::request::{HttpMethod, KeyValuePair, Protocol, Request};
use hitt_core::core::response::{RequestTiming, Response, ResponseBody, ResponseSize};
use hitt_core::protocols::sse::{SseEvent, SseSession, SseStatus};
use hitt_core::protocols::websocket::{
    MessageDirection, WebSocketSession, WsContent, WsMessage, WsStatus,
};
use hitt_core::storage::config::AppConfig;
use hitt_tui::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, ResponseTabKind, SearchResult, SidebarSection,
};
use hitt_tui::event::{handle_event, AppEvent};
use hitt_tui::ui::theme::{contrast_ratio, Theme, AVAILABLE_THEMES};
use hitt_tui::ui::widgets::diff_viewer::DiffView;
use hitt_tui::ui::widgets::runner::{RunRow, RunView};

fn test_app() -> (App, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    (App::new(config).unwrap(), tmp)
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

async fn press(app: &mut App, event: AppEvent) {
    handle_event(app, event).await.unwrap();
}

fn render(app: &mut App, w: u16, h: u16) -> Buffer {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| hitt_tui::ui::layout::render(app, f))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn rows(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

fn draw(app: &mut App, w: u16, h: u16) -> String {
    rows(&render(app, w, h)).join("\n")
}

fn status_bar(app: &mut App) -> String {
    draw(app, 120, 24).lines().last().unwrap().to_string()
}

fn response(status: u16, timing: RequestTiming) -> Response {
    Response {
        id: uuid::Uuid::new_v4(),
        status,
        status_text: "OK".into(),
        headers: vec![KeyValuePair::new("content-type", "application/json")],
        body: ResponseBody::Json("{\"ok\": true}".into()),
        cookies: Vec::new(),
        timing,
        size: ResponseSize {
            headers: 120,
            body: 1100,
        },
        assertion_results: Vec::new(),
        timestamp: chrono::Utc::now(),
    }
}

fn ms(tenths: u64) -> Duration {
    Duration::from_micros(tenths * 100)
}

// ── 1: the accumulated SSE text renders only its visible rows ──────────────

fn sse_app(text_bytes: usize) -> (App, TempDir) {
    let (mut app, tmp) = test_app();
    let tab = app.active_tab_mut();
    tab.request.protocol = Protocol::Sse;
    tab.response_tab = ResponseTabKind::SseStream;
    let mut session = SseSession::new("https://stream.example/recentchange");
    session.status = SseStatus::Connected;
    let mut i = 0;
    while session.accumulated_text.len() < text_bytes {
        // Wikimedia-like JSON lines (~300 bytes), some with non-ASCII text.
        writeln!(
            session.accumulated_text,
            "{{\"$schema\":\"/mediawiki/recentchange/1.0.0\",\"id\":{i},\"title\":\"Página {i} — ü\",\"comment\":\"{}\"}}",
            "edit ".repeat(40)
        )
        .unwrap();
        i += 1;
    }
    session
        .accumulated_text
        .push_str("LAST LINE OF THE STREAM\n");
    tab.sse_session = Some(session);
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;
    (app, tmp)
}

#[tokio::test]
async fn sse_stream_view_renders_a_large_text_quickly_and_follows_the_end() {
    let (mut app, _tmp) = sse_app(1024 * 1024);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    // Warm up once, then every frame (debug build, 1 MB of mostly non-ASCII
    // text) must stay under 50 ms.
    terminal
        .draw(|f| hitt_tui::ui::layout::render(&mut app, f))
        .unwrap();
    for _ in 0..5 {
        let start = Instant::now();
        terminal
            .draw(|f| hitt_tui::ui::layout::render(&mut app, f))
            .unwrap();
        let took = start.elapsed();
        assert!(took < Duration::from_millis(50), "frame took {took:?}");
    }
    let screen = rows(terminal.backend().buffer()).join("\n");
    assert!(screen.contains("LAST LINE OF THE STREAM"), "{screen}");
    // Past the old u16 scroll cap: far more rows than 65 535.
    assert!(app.active_tab().sse_stream_rows > 10_000);
}

#[tokio::test]
async fn sse_stream_view_scrolls_up_and_stays_put_while_text_arrives() {
    let (mut app, _tmp) = sse_app(64 * 1024);
    draw(&mut app, 80, 24);
    press(&mut app, key(KeyCode::Char('k'))).await;
    draw(&mut app, 80, 24);
    let scroll = app.active_tab().sse_event_scroll;
    assert!(!scroll.follow, "k leaves follow mode");
    assert_eq!(
        scroll.offset,
        app.active_tab().sse_stream_rows - scroll.page - 1
    );

    // New text doesn't move the view while not following.
    let text_rows = |screen: String| -> Vec<String> {
        screen
            .lines()
            .skip_while(|l| !l.contains("Accumulated Text"))
            .map(String::from)
            .collect()
    };
    let before = text_rows(draw(&mut app, 80, 24));
    app.active_tab_mut()
        .sse_session
        .as_mut()
        .unwrap()
        .push_event(SseEvent {
            event_type: None,
            data: "NEW EVENT".into(),
            id: None,
            timestamp: chrono::Utc::now(),
        });
    assert_eq!(text_rows(draw(&mut app, 80, 24)), before);

    // G follows the end again.
    press(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
    )
    .await;
    assert!(draw(&mut app, 80, 24).contains("NEW EVENT"));
}

// ── 2: selected rows are readable in every theme ───────────────────────────

/// Every non-blank cell drawn on the selection background inside `area`
/// must have a 4.5:1 contrast with it. Returns how many cells were checked.
fn assert_selected_cells_readable(buf: &Buffer, area: Rect, theme: &Theme, what: &str) -> usize {
    let bg = theme.colors.accent;
    let mut checked = 0;
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let cell = &buf[(x, y)];
            if cell.bg != bg || cell.symbol().trim().is_empty() {
                continue;
            }
            let ratio = contrast_ratio(cell.fg, cell.bg).unwrap_or(0.0);
            assert!(
                ratio >= 4.5,
                "{}: {what}: {:?} on {:?} = {ratio:.2}:1 at ({x},{y}) '{}'",
                theme.name,
                cell.fg,
                cell.bg,
                cell.symbol()
            );
            checked += 1;
        }
    }
    checked
}

fn sidebar_fixture(app: &mut App) {
    let mut coll = Collection::new("API");
    coll.add_request(Request::new("Users", HttpMethod::GET, "http://x/users"));
    coll.add_request(Request::new("Drop", HttpMethod::DELETE, "http://x/drop"));
    let mut ws = Request::new("Live", HttpMethod::GET, "ws://x/live");
    ws.protocol = Protocol::WebSocket;
    coll.add_request(ws);
    let folder_id = uuid::Uuid::new_v4();
    coll.items.push(CollectionItem::Folder {
        id: folder_id,
        name: "Admin".into(),
        items: vec![CollectionItem::Request(Box::new(Request::new(
            "Patch",
            HttpMethod::PATCH,
            "http://x/p",
        )))],
        auth: None,
        description: None,
    });
    let mut chain = RequestChain::new("Flow");
    chain.add_step(uuid::Uuid::new_v4());
    coll.chains.push(chain);
    app.sidebar_state.expanded.insert(coll.id);
    app.sidebar_state.expanded.insert(folder_id);
    app.collections.push(coll);
    for status in [200, 404, 500] {
        let mut entry = HistoryEntry::new(HttpMethod::POST, format!("http://x/{status}"));
        entry.status = Some(status);
        entry.duration_ms = Some(12);
        app.history.add(entry);
    }
}

#[tokio::test]
async fn sidebar_selected_rows_are_readable_in_every_theme() {
    for name in AVAILABLE_THEMES {
        let (mut app, _tmp) = test_app();
        sidebar_fixture(&mut app);
        app.theme = Theme::load(name).unwrap();
        // The sidebar unfocused (no URL tooltip over the rows).
        app.focus = FocusArea::UrlBar;
        let sidebar = Rect::new(1, 5, 23, 17);
        for (section, len) in [
            (SidebarSection::Collections, 6),
            (SidebarSection::History, 3),
            (SidebarSection::Chains, 1),
        ] {
            app.sidebar_state.section = section;
            for selected in 0..len {
                app.sidebar_state.selected = selected;
                let buf = render(&mut app, 80, 24);
                let what = format!("{section:?} row {selected}");
                let checked = assert_selected_cells_readable(&buf, sidebar, &app.theme, &what);
                assert!(checked > 0, "{name}: {what}: no selected row drawn");
            }
        }
    }
}

fn run_view(rows: usize) -> RunView {
    RunView {
        id: uuid::Uuid::new_v4(),
        coll_idx: 0,
        collection: "API".into(),
        folder: None,
        environment: None,
        rows: (0..rows)
            .map(|i| RunRow {
                request_id: uuid::Uuid::new_v4(),
                name: format!("req{i}"),
                method: if i % 2 == 0 { "GET" } else { "DELETE" }.into(),
                folder: None,
                iteration: 1,
                result: None,
            })
            .collect(),
        current: Some(0),
        running: true,
        cancelled: false,
        selected: 0,
        follow: false,
        started: Instant::now(),
        started_at: chrono::Utc::now(),
        elapsed: None,
        summary: None,
        abort: None,
        export_input: None,
    }
}

#[tokio::test]
async fn runner_and_chain_selected_rows_are_readable_in_every_theme() {
    for name in AVAILABLE_THEMES {
        let (mut app, _tmp) = test_app();
        app.theme = Theme::load(name).unwrap();
        let screen = Rect::new(0, 0, 80, 24);

        app.runner.run = Some(run_view(3));
        app.mode = AppMode::Modal(ModalKind::Runner);
        for selected in 0..3 {
            app.runner.run.as_mut().unwrap().selected = selected;
            let buf = render(&mut app, 80, 24);
            // Only the modal (the status bar badge also uses the accent).
            let modal = app.regions.overlay.unwrap();
            let checked = assert_selected_cells_readable(&buf, modal, &app.theme, "runner");
            assert!(checked > 0);
        }

        // Chain runner: the current (running) step.
        let mut coll = Collection::new("C");
        let req = Request::new("Login", HttpMethod::POST, "http://x/login");
        let mut chain = RequestChain::new("Flow");
        chain.add_step(req.id);
        coll.add_request(req);
        app.collections.push(coll);
        let mut state = ChainExecutionState::new(&chain);
        state.running = true;
        state.step_statuses[0] = ChainStepStatus::Running;
        app.active_chain = Some(state);
        app.active_chain_def = Some(chain);
        app.active_chain_coll_idx = Some(0);
        app.mode = AppMode::ChainEditor;
        let buf = render(&mut app, 80, 24);
        let body = Rect::new(25, 3, 55, screen.height - 4);
        assert!(assert_selected_cells_readable(&buf, body, &app.theme, "chain") > 0);
    }
}

// ── 3: clicking / selecting a visible row never re-scrolls the list ────────

#[tokio::test]
async fn request_table_keeps_its_offset_when_a_visible_row_is_selected() {
    let (mut app, _tmp) = test_app();
    for i in 1..=10 {
        app.active_tab_mut()
            .request
            .params
            .push(KeyValuePair::new(format!("p{i}"), "v"));
    }
    app.focus = FocusArea::RequestBody;
    app.nav_mode = NavMode::Panel;
    app.editor.params_selected = 9;
    let screen = draw(&mut app, 80, 24);
    let first = app.regions.request_rows[0].1;
    assert!(
        screen.contains("p10") && !screen.contains("p1 "),
        "{screen}"
    );

    app.editor.params_selected = 6; // p7, already visible
    draw(&mut app, 80, 24);
    assert_eq!(app.regions.request_rows[0].1, first);
}

#[tokio::test]
async fn diff_selector_keeps_its_offset_when_a_visible_row_is_selected() {
    let (mut app, _tmp) = test_app();
    for i in 0..30 {
        let mut entry = HistoryEntry::new(HttpMethod::GET, format!("http://x/{i}"));
        entry.status = Some(200);
        entry.response_body = Some(format!("body {i}"));
        app.history.add(entry);
    }
    app.mode = AppMode::Modal(ModalKind::DiffSelector);
    app.diff_state.selected = 29;
    draw(&mut app, 80, 24);
    let first = app.regions.modal_items[0].1;
    assert!(first > 0);

    app.diff_state.selected = first + 1;
    draw(&mut app, 80, 24);
    assert_eq!(app.regions.modal_items[0].1, first);
}

#[tokio::test]
async fn search_and_runner_keep_their_offset_when_a_visible_row_is_selected() {
    let (mut app, _tmp) = test_app();
    app.search_results = (0..40)
        .map(|i| SearchResult {
            name: format!("req{i}"),
            method: Some(HttpMethod::GET),
            url: format!("http://x/{i}"),
            collection_name: Some("API".into()),
            request_id: uuid::Uuid::new_v4(),
            collection_index: 0,
        })
        .collect();
    app.mode = AppMode::Modal(ModalKind::Search);
    app.search_selected = 39;
    draw(&mut app, 80, 24);
    let first = app.regions.modal_items[0].1;
    assert!(first > 0);
    app.search_selected = first + 1;
    draw(&mut app, 80, 24);
    assert_eq!(app.regions.modal_items[0].1, first);

    app.runner.run = Some(run_view(40));
    app.mode = AppMode::Modal(ModalKind::Runner);
    app.runner.run.as_mut().unwrap().selected = 39;
    draw(&mut app, 80, 24);
    let first = app.regions.modal_items[0].1;
    assert!(first > 0);
    app.runner.run.as_mut().unwrap().selected = first + 1;
    draw(&mut app, 80, 24);
    assert_eq!(app.regions.modal_items[0].1, first);
}

// ── 4 / 7: timing tab scrolls; total uses the phases' precision ────────────

#[tokio::test]
async fn timing_tab_scrolls_to_a_total_that_covers_the_phases() {
    let (mut app, _tmp) = test_app();
    // 129.8 ms to first byte + 0.4 ms download, measured total 129.9 ms.
    let timing = RequestTiming::measured(ms(1298), ms(4), ms(1299));
    app.active_tab_mut().response = Some(response(200, timing));
    app.active_tab_mut().response_tab = ResponseTabKind::Timing;
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;

    let screen = draw(&mut app, 80, 24);
    assert!(
        !screen.contains("Total:"),
        "too short to show it unscrolled"
    );
    press(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
    )
    .await;
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains("Total:            130.2ms"), "{screen}");
}

// ── 5: status texts are cut with an ellipsis, never glued to the tabs ──────

#[tokio::test]
async fn response_status_is_reserved_before_the_tabs_at_80_columns() {
    let (mut app, _tmp) = test_app();
    let timing = RequestTiming::measured(ms(1298), ms(4), ms(1302));
    app.active_tab_mut().response = Some(response(200, timing));
    let screen = draw(&mut app, 80, 24);
    let bar = screen
        .lines()
        .find(|l| l.contains("Body") && l.contains("Hdrs"))
        .unwrap();
    assert!(bar.contains(" 200 OK  130.2ms "), "{bar}");

    app.active_tab_mut().response = None;
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains(" No response yet "), "{screen}");

    // SSE: the hint is cut with `…` and a gap stays before the tabs.
    app.active_tab_mut().request.protocol = Protocol::Sse;
    app.active_tab_mut().response_tab = ResponseTabKind::SseEvents;
    let screen = draw(&mut app, 70, 24);
    let bar = screen.lines().find(|l| l.contains("Evts")).unwrap();
    assert!(bar.contains("… "), "{bar}");
    assert!(bar.contains(" SSE — Press"), "{bar}");
}

#[tokio::test]
async fn long_environment_names_are_cut_with_an_ellipsis() {
    let (mut app, _tmp) = test_app();
    app.environments
        .push(Environment::new("Test Environment Long"));
    app.active_env = Some(0);
    let header = draw(&mut app, 80, 24).lines().next().unwrap().to_string();
    assert!(header.contains("ENV: Test Environ…"), "{header}");
    assert!(header.ends_with("… "), "{header}");
}

// ── 8: chain runner icons and the cancelled header ─────────────────────────

#[tokio::test]
async fn chain_runner_shows_the_running_step_and_cancelled_state() {
    let (mut app, _tmp) = test_app();
    let mut coll = Collection::new("C");
    let req = Request::new("Login", HttpMethod::POST, "http://x/login");
    let mut chain = RequestChain::new("Flow");
    chain.add_step(req.id);
    chain.add_step(req.id);
    coll.add_request(req);
    app.collections.push(coll);
    let mut state = ChainExecutionState::new(&chain);
    state.running = true;
    state.step_statuses[0] = ChainStepStatus::Running;
    app.active_chain = Some(state);
    app.active_chain_def = Some(chain);
    app.active_chain_coll_idx = Some(0);
    app.mode = AppMode::ChainEditor;

    let screen = draw(&mut app, 80, 24);
    let step = screen.lines().find(|l| l.contains("Login")).unwrap();
    assert!(
        ["| ", "/ ", "- ", "\\ "]
            .iter()
            .any(|s| step.contains(&format!(" {s}POST"))),
        "{step}"
    );

    press(&mut app, key(KeyCode::Esc)).await;
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains("Cancelled"), "{screen}");
    assert!(!screen.contains("Complete"), "{screen}");
}

// ── 9: the last help line is reachable ─────────────────────────────────────

#[tokio::test]
async fn help_g_reaches_the_last_line_at_80x24() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Modal(ModalKind::Help);
    press(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
    )
    .await;
    let screen = draw(&mut app, 80, 24);
    let lines: Vec<&str> = screen.lines().collect();
    // The row just above the modal's bottom border holds the footer's end.
    let bottom = lines.iter().rposition(|l| l.contains("╰")).unwrap();
    assert!(lines[bottom - 1].contains("close"), "{screen}");
    assert!(screen.contains("everforest"), "{screen}");
}

// ── 10: diff view G keeps a full page, J pages by 15 ───────────────────────

#[tokio::test]
async fn diff_view_end_keeps_a_full_page() {
    let (mut app, _tmp) = test_app();
    let numbered = |step: usize| {
        (0..100).fold(String::new(), |mut text, i| {
            writeln!(text, "line {}", i * step).unwrap();
            text
        })
    };
    let (old, new) = (numbered(1), numbered(2));
    app.diff_state.view = Some(DiffView {
        result: hitt_core::testing::diff::diff_text(&old, &new),
        left_label: "a".into(),
        right_label: "b".into(),
    });
    app.mode = AppMode::Modal(ModalKind::DiffSelector);
    draw(&mut app, 80, 24);
    let lines = app.diff_state.view.as_ref().unwrap().result.lines.len();
    let page = app.diff_state.view_height.get();
    assert!(page > 1);

    press(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('J'), KeyModifiers::SHIFT)),
    )
    .await;
    assert_eq!(app.diff_state.scroll, 15);
    press(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
    )
    .await;
    assert_eq!(app.diff_state.scroll, lines - page);
    press(&mut app, key(KeyCode::Char('j'))).await;
    assert_eq!(app.diff_state.scroll, lines - page);
}

// ── 11: status-bar hints follow sub-states ─────────────────────────────────

#[tokio::test]
async fn status_hints_follow_sub_states() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.active_env = Some(0);
    app.mode = AppMode::Modal(ModalKind::EnvironmentEdit);
    assert!(status_bar(&mut app).contains("a/i/x"));
    press(&mut app, key(KeyCode::Char('a'))).await;
    assert!(app.editor.env_form().is_some());
    let bar = status_bar(&mut app);
    assert!(
        bar.contains("Tab:key/value") && !bar.contains("a/i/x"),
        "{bar}"
    );

    let (mut app, _tmp) = test_app();
    let mut run = run_view(2);
    run.running = false;
    run.export_input = Some("report.json".into());
    app.runner.run = Some(run);
    app.mode = AppMode::Modal(ModalKind::Runner);
    let bar = status_bar(&mut app);
    assert!(
        bar.contains("Type a path") && !bar.contains("x:export"),
        "{bar}"
    );

    // Unconnected WS / SSE tabs don't offer q:disconnect.
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;
    for protocol in [Protocol::WebSocket, Protocol::Sse] {
        app.active_tab_mut().request.protocol = protocol;
        assert!(!status_bar(&mut app).contains("q:disconnect"));
    }
    let mut session = SseSession::new("http://x/sse");
    session.status = SseStatus::Connected;
    app.active_tab_mut().sse_session = Some(session);
    assert!(status_bar(&mut app).contains("q:disconnect"));
}

// ── 12: the WS message list keeps a few lines at 80x24 ─────────────────────

#[tokio::test]
async fn ws_messages_show_several_lines_at_80x24() {
    let (mut app, _tmp) = test_app();
    let tab = app.active_tab_mut();
    tab.request.protocol = Protocol::WebSocket;
    tab.response_tab = ResponseTabKind::WsMessages;
    let mut session = WebSocketSession::new("ws://x/live");
    session.status = WsStatus::Connected {
        connected_at: chrono::Utc::now(),
    };
    for i in 0..10 {
        session.push_message(WsMessage {
            direction: MessageDirection::Received,
            content: WsContent::Text(format!("message {i}")),
            timestamp: chrono::Utc::now(),
        });
    }
    tab.ws_session = Some(session);
    let screen = draw(&mut app, 80, 24);
    let shown = screen.lines().filter(|l| l.contains("message ")).count();
    assert!(shown >= 4, "{shown} message lines\n{screen}");
    assert!(screen.contains("message 9"), "{screen}");
}
