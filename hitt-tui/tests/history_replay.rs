use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use hitt_core::core::history::HistoryEntry;
use hitt_core::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt_core::storage::config::AppConfig;
use hitt_tui::app::{App, FocusArea, NavMode, NotificationKind, SidebarSection};
use hitt_tui::event::{execute_command, handle_event, AppEvent};

/// App whose collections (and so `history.json`) live in a temp dir.
fn test_app_with(tweak: impl FnOnce(&mut AppConfig)) -> (App, TempDir) {
    let tmp = TempDir::new().unwrap();
    let mut config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    tweak(&mut config);
    (App::new(config).unwrap(), tmp)
}

fn test_app() -> (App, TempDir) {
    test_app_with(|_| {})
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Focus the sidebar History section in Panel mode, entry `selected` selected.
fn focus_history(app: &mut App, selected: usize) {
    app.focus = FocusArea::Sidebar;
    app.nav_mode = NavMode::Panel;
    app.sidebar_state.section = SidebarSection::History;
    app.sidebar_state.selected = selected;
}

fn snapshot_entry(url: &str) -> HistoryEntry {
    let mut req = Request::new("create", HttpMethod::POST, url);
    req.headers.push(KeyValuePair::new("X-Replay", "yes"));
    req.body = Some(RequestBody::Json(r#"{"a":1}"#.into()));
    let mut entry = HistoryEntry::new(HttpMethod::POST, url);
    entry.status = Some(201);
    entry.duration_ms = Some(12);
    entry.response_body = Some(r#"{"stored":true}"#.into());
    entry.response_headers = vec![KeyValuePair::new("Content-Type", "application/json")];
    entry.request = Some(req);
    entry
}

/// Serve `count` requests with a fixed JSON body; returns the port and a
/// handle yielding each raw request head.
fn spawn_server(count: usize) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();
    let port = std_listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
        let mut heads = Vec::new();
        for _ in 0..count {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            let mut buf = [0u8; 4096];
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                head.extend_from_slice(&buf[..n]);
            }
            let body = r#"{"fresh":true}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
            stream.shutdown().await.ok();
            heads.push(String::from_utf8_lossy(&head).into_owned());
        }
        heads
    });
    (port, handle)
}

// ════════════════════════════════════════════════════════════════════════════
// Limit
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn history_limit_from_config_is_enforced() {
    let (mut app, _tmp) = test_app_with(|c| c.history_limit = 3);
    for i in 0..5 {
        app.record_history(HistoryEntry::new(HttpMethod::GET, format!("http://x/{i}")));
    }
    assert_eq!(app.history.len(), 3);
    assert_eq!(app.history.entries()[0].url, "http://x/4");
}

#[tokio::test]
async fn set_history_limit_truncates_existing_entries() {
    let (mut app, _tmp) = test_app();
    for i in 0..4 {
        app.record_history(HistoryEntry::new(HttpMethod::GET, format!("http://x/{i}")));
    }
    execute_command(&mut app, "set history_limit 2")
        .await
        .unwrap();
    assert_eq!(app.config.history_limit, 2);
    assert_eq!(app.history.len(), 2);

    execute_command(&mut app, "set history_limit 0")
        .await
        .unwrap();
    assert_eq!(app.config.history_limit, 2, "0 is rejected");
}

// ════════════════════════════════════════════════════════════════════════════
// Reopen (Enter)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn enter_on_history_opens_snapshot_and_stored_response_without_sending() {
    let (mut app, _tmp) = test_app();
    app.history.add(snapshot_entry("http://127.0.0.1:1/users"));
    focus_history(&mut app, 0);

    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    let tab = app.active_tab();
    assert_eq!(tab.request.method, HttpMethod::POST);
    assert_eq!(tab.request.headers[0].key, "X-Replay");
    assert!(matches!(tab.request.body, Some(RequestBody::Json(_))));
    let resp = tab.response.as_ref().expect("stored response shown");
    assert_eq!(resp.status, 201);
    assert_eq!(resp.body_text(), Some(r#"{"stored":true}"#));
    assert!(tab.pending_request.is_none(), "not sent");
    assert_eq!(app.history.len(), 1, "nothing new recorded");
}

#[tokio::test]
async fn enter_on_old_entry_falls_back_to_method_and_url() {
    let (mut app, _tmp) = test_app();
    app.history
        .add(HistoryEntry::new(HttpMethod::PUT, "http://old.test/a"));
    focus_history(&mut app, 0);

    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();

    let tab = app.active_tab();
    assert_eq!(tab.request.method, HttpMethod::PUT);
    assert_eq!(tab.request.url, "http://old.test/a");
    assert!(tab.response.is_none(), "no status recorded -> no response");
}

// ════════════════════════════════════════════════════════════════════════════
// Replay (r / Ctrl+R / :replay)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn r_on_history_replays_the_snapshot() {
    let (port, server) = spawn_server(1);
    let (mut app, _tmp) = test_app();
    app.history
        .add(snapshot_entry(&format!("http://127.0.0.1:{port}/users")));
    focus_history(&mut app, 0);

    handle_event(&mut app, key(KeyCode::Char('r')))
        .await
        .unwrap();

    let heads = server.await.unwrap();
    assert!(heads[0].starts_with("POST /users"), "{}", heads[0]);
    assert!(heads[0].to_lowercase().contains("x-replay: yes"));
    assert_eq!(app.tabs.len(), 2);
    let resp = app.active_tab().response.as_ref().unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body_text(), Some(r#"{"fresh":true}"#));
    assert_eq!(app.history.len(), 2, "replay is recorded");
    assert!(app.history.entries()[0].request.is_some());
}

#[tokio::test]
async fn ctrl_r_on_history_replays() {
    let (port, server) = spawn_server(1);
    let (mut app, _tmp) = test_app();
    app.history
        .add(snapshot_entry(&format!("http://127.0.0.1:{port}/x")));
    focus_history(&mut app, 0);

    handle_event(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
    )
    .await
    .unwrap();

    server.await.unwrap();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab().response.as_ref().unwrap().status, 200);
}

#[tokio::test]
async fn replay_command_uses_one_based_index() {
    let (port, server) = spawn_server(1);
    let (mut app, _tmp) = test_app();
    app.history
        .add(snapshot_entry(&format!("http://127.0.0.1:{port}/older")));
    app.history.add(snapshot_entry("http://127.0.0.1:1/newest"));

    execute_command(&mut app, "replay 2").await.unwrap();

    let heads = server.await.unwrap();
    assert!(heads[0].starts_with("POST /older"), "{}", heads[0]);
    assert!(app.active_tab().request.url.ends_with("/older"));
}

#[tokio::test]
async fn replay_command_rejects_bad_index_and_empty_history() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "replay").await.unwrap();
    let n = app.notification.as_ref().unwrap();
    assert_eq!(n.kind, NotificationKind::Warning);
    assert!(n.message.contains("empty"));

    app.history.add(snapshot_entry("http://127.0.0.1:1/a"));
    for bad in ["replay 0", "replay 5", "replay abc"] {
        execute_command(&mut app, bad).await.unwrap();
        assert!(app
            .notification
            .as_ref()
            .unwrap()
            .message
            .contains("Usage: :replay"));
    }
    assert_eq!(app.tabs.len(), 1);
}

// ════════════════════════════════════════════════════════════════════════════
// Persistence
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn history_persists_across_sessions_and_clears() {
    let (mut app, tmp) = test_app();
    let path = tmp.path().join("history.json");
    assert_eq!(app.history_path(), path);

    app.record_history(snapshot_entry("http://saved.test/a"));
    assert!(path.exists());

    // A new session over the same dir restores it, snapshot included.
    let config = app.config.clone();
    let mut next = App::new(config).unwrap();
    next.load_history();
    assert_eq!(next.history.len(), 1);
    assert_eq!(
        next.history.entries()[0].request.as_ref().unwrap().headers[0].key,
        "X-Replay"
    );

    execute_command(&mut next, "clearhistory").await.unwrap();
    assert!(next.history.is_empty());
    assert!(!path.exists());
}

#[test]
fn persist_history_false_does_not_touch_disk() {
    let (mut app, tmp) = test_app_with(|c| c.persist_history = false);
    app.record_history(snapshot_entry("http://nope.test"));
    assert!(!tmp.path().join("history.json").exists());
    app.load_history();
    assert_eq!(app.history.len(), 1, "load is a no-op too");
}
