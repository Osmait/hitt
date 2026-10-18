//! Collection runner in the TUI (`:run`, sidebar `R`, runner modal) and the
//! `hitt test` CLI subcommand, against a local HTTP server.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedReceiver;

use hitt_core::core::collection::{Collection, CollectionItem};
use hitt_core::core::environment::Environment;
use hitt_core::core::request::{HttpMethod, Request};
use hitt_core::storage::collections_store::CollectionsStore;
use hitt_core::storage::config::AppConfig;
use hitt_core::testing::assertion_engine::Assertion;
use hitt_tui::app::{App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind};
use hitt_tui::cli::{Commands, Reporter, TestArgs};
use hitt_tui::event::{execute_command, handle_event, AppEvent};

/// Local server: `/status/NNN` answers NNN, `/hang` never answers, anything
/// else 200. The body echoes the request head.
async fn spawn_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut buf = [0u8; 2048];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                if path == "/hang" {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    return;
                }
                let status = path
                    .strip_prefix("/status/")
                    .and_then(|s| s.get(..3))
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(200);
                let resp = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{head}",
                    head.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });
    base
}

fn test_app() -> (App, TempDir, UnboundedReceiver<AppEvent>) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    let mut app = App::new(config).unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    (app, tmp, rx)
}

fn asserting(name: &str, url: String, status: u16) -> Request {
    let mut req = Request::new(name, HttpMethod::GET, url);
    req.assertions = vec![Assertion::status_equals(status)];
    req
}

/// "API": `ok` (passes), folder "Users" with `bad` (fails) and `users ok`.
fn sample_collection(base: &str) -> Collection {
    let mut coll = Collection::new("API");
    coll.add_request(asserting("ok", "{{base}}/a".to_string(), 200));
    coll.add_folder("Users").extend([
        CollectionItem::Request(Box::new(asserting(
            "bad",
            format!("{base}/status/500"),
            200,
        ))),
        CollectionItem::Request(Box::new(asserting("users ok", format!("{base}/u"), 200))),
    ]);
    coll
}

fn dev_env(base: &str) -> Environment {
    let mut env = Environment::new("dev");
    env.add_variable("base", base);
    env
}

/// Feed background events into the app until the run is over.
async fn pump_until_done(app: &mut App, rx: &mut UnboundedReceiver<AppEvent>) {
    while app.runner.run.as_ref().is_some_and(|r| r.running) {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("runner never finished")
            .expect("channel closed");
        handle_event(app, event).await.unwrap();
    }
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn screen_text(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| hitt_tui::ui::layout::render(app, frame))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

#[tokio::test]
async fn run_command_runs_collection_in_background_and_renders_report() {
    let base = spawn_server().await;
    let (mut app, _tmp, mut rx) = test_app();
    app.collections.push(sample_collection(&base));
    app.environments.push(dev_env(&base));
    app.active_env = Some(0);

    execute_command(&mut app, "run API").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Runner));
    let run = app.runner.run.as_ref().unwrap();
    assert!(run.running);
    assert_eq!(run.rows.len(), 3);
    assert_eq!(run.environment.as_deref(), Some("dev"));

    // Renders while running without panicking.
    let running = screen_text(&mut app, 120, 40);
    assert!(running.contains("Collection Runner: API"));

    pump_until_done(&mut app, &mut rx).await;
    let run = app.runner.run.as_ref().unwrap();
    let summary = run.summary.as_ref().unwrap();
    assert_eq!(summary.total, 3);
    assert_eq!(summary.passed, 2);
    assert_eq!(summary.failed, 1);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );

    let text = screen_text(&mut app, 120, 40);
    assert!(text.contains("2 passed"), "{text}");
    assert!(text.contains("1 failed"));
    assert!(text.contains("Users/bad"));
    assert!(text.contains("500"));
    assert!(text.contains("✓"));
    assert!(text.contains("✗"));
    assert!(text.contains("Some requests failed"));
    assert!(text.contains("x:export"));
}

#[tokio::test]
async fn enter_opens_request_with_response_and_esc_closes() {
    let base = spawn_server().await;
    let (mut app, _tmp, mut rx) = test_app();
    app.collections.push(sample_collection(&base));
    app.environments.push(dev_env(&base));
    app.active_env = Some(0);

    execute_command(&mut app, "run API").await.unwrap();
    pump_until_done(&mut app, &mut rx).await;

    // The selection lands on the first failure; j/k move it.
    assert_eq!(app.runner.run.as_ref().unwrap().selected, 1);
    handle_event(&mut app, key(KeyCode::Char('j')))
        .await
        .unwrap();
    assert_eq!(app.runner.run.as_ref().unwrap().selected, 2);
    handle_event(&mut app, key(KeyCode::Char('k')))
        .await
        .unwrap();
    assert_eq!(app.runner.run.as_ref().unwrap().selected, 1);
    let tabs_before = app.tabs.len();
    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();
    assert_eq!(app.tabs.len(), tabs_before + 1);
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.focus, FocusArea::ResponseBody);
    let tab = app.active_tab();
    assert_eq!(tab.request.name, "bad");
    assert_eq!(tab.collection_index, Some(0));
    let resp = tab.response.as_ref().unwrap();
    assert_eq!(resp.status, 500);
    assert_eq!(resp.assertion_results.len(), 1);
    assert!(!resp.assertion_results[0].passed);

    // Esc closes a finished run but keeps it for `:runexport`.
    execute_command(&mut app, "run API").await.unwrap();
    pump_until_done(&mut app, &mut rx).await;
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.runner.run.is_some());
}

#[tokio::test]
async fn esc_cancels_running_run_then_closes() {
    let base = spawn_server().await;
    let (mut app, _tmp, mut rx) = test_app();
    let mut coll = Collection::new("Slow");
    coll.add_request(Request::new(
        "hang",
        HttpMethod::GET,
        format!("{base}/hang"),
    ));
    coll.add_request(Request::new("after", HttpMethod::GET, format!("{base}/a")));
    app.collections.push(coll);

    execute_command(&mut app, "run Slow").await.unwrap();
    // Wait until the first request is in flight.
    while app.runner.run.as_ref().unwrap().current.is_none() {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        handle_event(&mut app, event).await.unwrap();
    }

    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Runner));
    let run = app.runner.run.as_ref().unwrap();
    assert!(!run.running);
    assert!(run.cancelled);
    assert!(run.summary.as_ref().unwrap().cancelled);
    assert!(screen_text(&mut app, 120, 40).contains("Cancelled after 0/2"));

    handle_event(&mut app, key(KeyCode::Char('q')))
        .await
        .unwrap();
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn sidebar_shift_r_runs_selected_folder() {
    let base = spawn_server().await;
    let (mut app, _tmp, mut rx) = test_app();
    let coll = sample_collection(&base);
    let coll_id = coll.id;
    app.collections.push(coll);
    app.sidebar_state.expanded.insert(coll_id);
    app.focus = FocusArea::Sidebar;
    app.nav_mode = NavMode::Panel;
    // Rows: collection, ok, Users folder.
    app.sidebar_state.selected = 2;

    handle_event(
        &mut app,
        AppEvent::Key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT)),
    )
    .await
    .unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Runner));
    let run = app.runner.run.as_ref().unwrap();
    assert_eq!(run.rows.len(), 2);
    assert_eq!(run.folder.as_deref(), Some("Users"));

    pump_until_done(&mut app, &mut rx).await;
    let summary = app.runner.run.as_ref().unwrap().summary.clone().unwrap();
    assert_eq!(summary.folder.as_deref(), Some("Users"));
    assert_eq!(summary.failed, 1);
    assert!(screen_text(&mut app, 120, 40).contains("Collection Runner: API / Users"));
}

#[tokio::test]
async fn run_command_errors_and_bail() {
    let base = spawn_server().await;
    let (mut app, _tmp, mut rx) = test_app();

    execute_command(&mut app, "run").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );

    app.collections.push(sample_collection(&base));
    execute_command(&mut app, "run Nope").await.unwrap();
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("not found"));
    execute_command(&mut app, "run API Missing").await.unwrap();
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("Folder 'Missing'"));
    assert!(app.runner.run.is_none());

    // Folder by name + --bail stops at the failing request.
    execute_command(&mut app, "run API Users --bail")
        .await
        .unwrap();
    pump_until_done(&mut app, &mut rx).await;
    let summary = app.runner.run.as_ref().unwrap().summary.clone().unwrap();
    // The request after the failure never ran: it is reported as skipped.
    assert_eq!(summary.total, 2);
    assert_eq!(summary.skipped, 1);
    assert!(summary.bailed);
}

#[tokio::test]
async fn runexport_writes_json_and_junit() {
    let base = spawn_server().await;
    let (mut app, tmp, mut rx) = test_app();

    execute_command(&mut app, "runexport out.json")
        .await
        .unwrap();
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("No collection run"));

    app.collections.push(sample_collection(&base));
    app.environments.push(dev_env(&base));
    app.active_env = Some(0);
    execute_command(&mut app, "run API").await.unwrap();
    pump_until_done(&mut app, &mut rx).await;

    let json_path = tmp.path().join("report.json");
    execute_command(&mut app, &format!("runexport {}", json_path.display()))
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
    assert_eq!(json["total"], 3);
    assert_eq!(json["failed"], 1);

    // `x` prompt inside the runner modal.
    let xml_path = tmp.path().join("report.xml");
    handle_event(&mut app, key(KeyCode::Char('x')))
        .await
        .unwrap();
    assert!(app.runner.run.as_ref().unwrap().export_input.is_some());
    app.runner.run.as_mut().unwrap().export_input = Some(String::new());
    for c in xml_path.display().to_string().chars() {
        handle_event(&mut app, key(KeyCode::Char(c))).await.unwrap();
    }
    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();
    let xml = std::fs::read_to_string(&xml_path).unwrap();
    assert!(xml.contains("<testsuites"));
    assert!(xml.contains("failures=\"1\""));
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Runner));

    execute_command(&mut app, "runexport report.csv")
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

// ── hitt test (CLI) ────────────────────────────────────────────────────────

fn cli_setup(base: &str) -> (TempDir, AppConfig) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    store.save_collection(&sample_collection(base)).unwrap();
    store.save_environment(&dev_env(base)).unwrap();
    (tmp, config)
}

fn test_args(
    folder: Option<&str>,
    reporter: Reporter,
    output: Option<std::path::PathBuf>,
) -> TestArgs {
    TestArgs {
        collection: "api".into(),
        folder: folder.map(Into::into),
        env: Some("dev".into()),
        bail: false,
        reporter,
        output,
        delay: 0,
        iterations: 1,
    }
}

#[tokio::test]
async fn cli_test_fails_on_assertion_failure_and_writes_junit() {
    let base = spawn_server().await;
    let (tmp, config) = cli_setup(&base);
    let out = tmp.path().join("junit.xml");

    let err = hitt_tui::cli::run(
        Commands::Test(test_args(None, Reporter::Junit, Some(out.clone()))),
        &config,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("1 of 3"));
    let xml = std::fs::read_to_string(out).unwrap();
    assert!(xml.contains("tests=\"3\" failures=\"1\""));
}

#[tokio::test]
async fn cli_test_succeeds_when_all_pass() {
    let base = spawn_server().await;
    let (tmp, config) = cli_setup(&base);
    let mut coll = Collection::new("Green");
    coll.add_request(asserting("ok", format!("{base}/a"), 200));
    CollectionsStore::new(config.collections_dir.clone())
        .unwrap()
        .save_collection(&coll)
        .unwrap();

    let mut args = test_args(None, Reporter::Json, Some(tmp.path().join("r.json")));
    args.collection = "Green".into();
    args.env = None;
    hitt_tui::cli::run(Commands::Test(args), &config)
        .await
        .unwrap();

    let err = hitt_tui::cli::run(
        Commands::Test(test_args(Some("Nope"), Reporter::Text, None)),
        &config,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Folder 'Nope'"));

    let mut args = test_args(None, Reporter::Text, None);
    args.env = Some("missing".into());
    let err = hitt_tui::cli::run(Commands::Test(args), &config)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Environment 'missing'"));
}

#[tokio::test]
async fn run_command_accepts_unquoted_multi_word_collection_name() {
    let base = spawn_server().await;
    let (mut app, _tmp, mut rx) = test_app();
    let mut coll = sample_collection(&base);
    coll.name = "My Big API".into();
    app.collections.push(coll);
    app.environments.push(dev_env(&base));
    app.active_env = Some(0);

    execute_command(&mut app, "run my big api").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Runner));
    pump_until_done(&mut app, &mut rx).await;
    assert_eq!(
        app.runner
            .run
            .as_ref()
            .unwrap()
            .summary
            .as_ref()
            .unwrap()
            .total,
        3
    );

    // Trailing words after the collection name are the folder.
    execute_command(&mut app, "run My Big API Nope")
        .await
        .unwrap();
    let note = app.notification.as_ref().unwrap();
    assert_eq!(note.kind, NotificationKind::Error);
    assert!(note.message.contains("Nope"), "{}", note.message);
}
