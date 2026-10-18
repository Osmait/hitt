use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use hitt_core::core::auth::AuthConfig;
use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::request::RequestBody;
use hitt_core::storage::config::AppConfig;
use hitt_core::testing::assertion_engine::AssertionKind;
use hitt_tui::app::{App, AppMode, FocusArea, ModalKind, NavMode, RequestTabKind};
use hitt_tui::event::{execute_command, handle_event, AppEvent};

/// App with a temp dir for collections/environments so tests never touch real config.
fn test_app() -> (App, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    let app = App::new(config).unwrap();
    (app, tmp)
}

/// Focus the request area (Panel mode) on `tab`.
fn on_request_tab(app: &mut App, tab: RequestTabKind) {
    app.focus = FocusArea::RequestBody;
    app.nav_mode = NavMode::Panel;
    app.active_tab_mut().request_tab = tab;
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, modifiers))
}

/// A printable key as the terminal sends it (uppercase arrives with SHIFT).
fn char_key(c: char) -> AppEvent {
    let modifiers = if c.is_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    key_mod(KeyCode::Char(c), modifiers)
}

async fn press(app: &mut App, event: AppEvent) {
    handle_event(app, event).await.unwrap();
}

async fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, char_key(c)).await;
    }
}

fn draw(app: &mut App, width: u16, height: u16) -> String {
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

// ════════════════════════════════════════════════════════════════════════════
// Headers / params
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn header_add_edit_toggle_delete() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Headers);

    press(&mut app, key(KeyCode::Char('a'))).await;
    assert_eq!(app.mode, AppMode::Insert);
    type_str(&mut app, "X-Api-Key").await;
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "ABC qd").await;
    press(&mut app, key(KeyCode::Enter)).await;

    assert_eq!(app.mode, AppMode::Normal);
    let headers = &app.active_tab().request.headers;
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].key, "X-Api-Key");
    assert_eq!(headers[0].value, "ABC qd");
    assert!(headers[0].enabled);
    assert!(app.active_tab().dirty);

    // Space toggles enabled.
    press(&mut app, key(KeyCode::Char(' '))).await;
    assert!(!app.active_tab().request.headers[0].enabled);

    // i edits the selected row: change the value.
    press(&mut app, key(KeyCode::Char('i'))).await;
    press(&mut app, key(KeyCode::Tab)).await;
    press(&mut app, key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL)).await;
    type_str(&mut app, "NEW").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.active_tab().request.headers[0].value, "NEW");

    // d deletes the row here instead of opening the diff selector.
    press(&mut app, key(KeyCode::Char('d'))).await;
    assert!(app.active_tab().request.headers.is_empty());
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn param_add_uppercase_and_cancel_new_row() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Params);

    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "Page").await;
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "TWO").await;
    press(&mut app, key(KeyCode::Enter)).await;

    // A second row cancelled with Esc disappears again.
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "tmp").await;
    press(&mut app, key(KeyCode::Esc)).await;

    let params = &app.active_tab().request.params;
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].key, "Page");
    assert_eq!(params[0].value, "TWO");

    // x deletes, j/k on an empty table are harmless.
    press(&mut app, key(KeyCode::Char('x'))).await;
    press(&mut app, key(KeyCode::Char('j'))).await;
    assert!(app.active_tab().request.params.is_empty());
}

#[tokio::test]
async fn editing_field_swallows_shortcut_keys() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Headers);
    press(&mut app, key(KeyCode::Char('a'))).await;
    // q (quit), d (diff), t (new tab), : (command), ? (help) must all be typed.
    type_str(&mut app, "qdt:?wE").await;
    assert_eq!(app.mode, AppMode::Insert);
    assert!(!app.should_quit);
    assert_eq!(app.tabs.len(), 1);
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.active_tab().request.headers[0].key, "qdt:?wE");
}

#[tokio::test]
async fn empty_key_is_rejected_with_error() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Headers);
    press(&mut app, key(KeyCode::Char('a'))).await;
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "value-only").await;
    press(&mut app, key(KeyCode::Enter)).await;
    // Still editing, with an error shown.
    assert_eq!(app.mode, AppMode::Insert);
    assert!(app.editor.form.as_ref().unwrap().error.is_some());
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("Key cannot be empty"));
}

#[tokio::test]
async fn paste_goes_into_focused_field() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Headers);
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "Authorization").await;
    press(&mut app, key(KeyCode::Tab)).await;
    press(&mut app, AppEvent::Paste("Bearer abc.DEF\n".into())).await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.active_tab().request.headers[0].value, "Bearer abc.DEF");
}

#[tokio::test]
async fn global_keys_still_work_outside_editors() {
    let (mut app, _tmp) = test_app();
    // `d` on a text body isn't an editor key: it still opens the diff selector.
    on_request_tab(&mut app, RequestTabKind::Body);
    app.active_tab_mut().request.body = Some(RequestBody::Json("{}".into()));
    press(&mut app, key(KeyCode::Char('d'))).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::DiffSelector));

    // `t` on headers opens a new tab as usual.
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Headers);
    press(&mut app, key(KeyCode::Char('t'))).await;
    assert_eq!(app.tabs.len(), 2);
}

// ════════════════════════════════════════════════════════════════════════════
// Body
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn body_type_change_and_multiline_json_edit() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Body);
    assert!(app.active_tab().request.body.is_none());

    press(&mut app, key(KeyCode::Char('t'))).await;
    assert!(matches!(
        app.active_tab().request.body,
        Some(RequestBody::Json(_))
    ));

    press(&mut app, key(KeyCode::Char('i'))).await;
    assert_eq!(app.mode, AppMode::Insert);
    assert!(app.editor.body.is_some());
    type_str(&mut app, "{").await;
    press(&mut app, key(KeyCode::Enter)).await;
    type_str(&mut app, "\"Name\": \"Q\",").await;
    press(&mut app, key(KeyCode::Enter)).await;
    type_str(&mut app, "\"id\": 1x").await;
    press(&mut app, key(KeyCode::Backspace)).await;
    press(&mut app, key(KeyCode::Enter)).await;
    type_str(&mut app, "}").await;

    // Render while editing (textarea widget) to catch panics.
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("Editing"));

    press(&mut app, key(KeyCode::Esc)).await;
    assert_eq!(app.mode, AppMode::Normal);
    match &app.active_tab().request.body {
        Some(RequestBody::Json(text)) => {
            assert_eq!(text, "{\n\"Name\": \"Q\",\n\"id\": 1\n}");
        }
        other => panic!("expected JSON body, got {other:?}"),
    }
    assert!(app.active_tab().dirty);

    // f pretty-prints, keeping key order.
    press(&mut app, key(KeyCode::Char('f'))).await;
    match &app.active_tab().request.body {
        Some(RequestBody::Json(text)) => {
            assert_eq!(text, "{\n  \"Name\": \"Q\",\n  \"id\": 1\n}");
        }
        other => panic!("expected JSON body, got {other:?}"),
    }

    // t carries the text over to raw.
    press(&mut app, key(KeyCode::Char('t'))).await;
    assert!(matches!(
        &app.active_tab().request.body,
        Some(RequestBody::Raw { content, content_type }) if content.contains("Name") && content_type == "text/plain"
    ));
}

#[tokio::test]
async fn invalid_json_warns_without_blocking() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Body);
    app.active_tab_mut().request.body = Some(RequestBody::Json("{\"a\": ".into()));
    press(&mut app, key(KeyCode::Char('f'))).await;
    // Unchanged, with a warning instead of an error state.
    assert!(matches!(
        &app.active_tab().request.body,
        Some(RequestBody::Json(t)) if t == "{\"a\": "
    ));
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("Invalid JSON"));
}

#[tokio::test]
async fn form_body_reuses_table_editor() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Body);
    app.active_tab_mut().request.body = Some(RequestBody::FormUrlEncoded(Vec::new()));
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "User").await;
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "Ana").await;
    press(&mut app, key(KeyCode::Enter)).await;
    match &app.active_tab().request.body {
        Some(RequestBody::FormUrlEncoded(pairs)) => {
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].key, "User");
            assert_eq!(pairs[0].value, "Ana");
        }
        other => panic!("expected form body, got {other:?}"),
    }
}

#[tokio::test]
async fn graphql_variables_edit_and_paste() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Body);
    app.active_tab_mut().request.body = Some(RequestBody::GraphQL {
        query: "{ me { id } }".into(),
        variables: None,
    });
    press(&mut app, char_key('I')).await;
    assert_eq!(app.mode, AppMode::Insert);
    press(&mut app, AppEvent::Paste("{\"id\":\r\n 1}".into())).await;
    press(&mut app, key(KeyCode::Esc)).await;
    assert!(matches!(
        &app.active_tab().request.body,
        Some(RequestBody::GraphQL { variables: Some(v), .. }) if v == "{\"id\":\n 1}"
    ));
}

// ════════════════════════════════════════════════════════════════════════════
// Auth
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn auth_bearer_edit_and_masking() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Auth);

    // inherit -> none -> bearer
    press(&mut app, key(KeyCode::Char('t'))).await;
    press(&mut app, key(KeyCode::Char('t'))).await;
    assert!(matches!(
        app.active_tab().request.auth,
        Some(AuthConfig::Bearer { .. })
    ));
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.mode, AppMode::Insert);
    type_str(&mut app, "SecretTOKEN9876").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert!(matches!(
        &app.active_tab().request.auth,
        Some(AuthConfig::Bearer { token }) if token == "SecretTOKEN9876"
    ));

    // Masked by default (last 4 visible), revealed with v.
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("********9876"));
    assert!(!screen.contains("SecretTOKEN"));
    press(&mut app, key(KeyCode::Char('v'))).await;
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("SecretTOKEN9876"));
}

#[tokio::test]
async fn auth_api_key_location_choice() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Auth);
    // inherit -> none -> bearer -> basic -> apikey
    for _ in 0..4 {
        press(&mut app, key(KeyCode::Char('t'))).await;
    }
    press(&mut app, key(KeyCode::Char('i'))).await;
    // Fields: Key, Value, Add to. Go to "Add to" and switch to query.
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "K3y").await;
    press(&mut app, key(KeyCode::Tab)).await;
    press(&mut app, key(KeyCode::Right)).await;
    press(&mut app, key(KeyCode::Enter)).await;
    match &app.active_tab().request.auth {
        Some(AuthConfig::ApiKey {
            key,
            value,
            location,
        }) => {
            assert_eq!(key, "X-API-Key");
            assert_eq!(value, "K3y");
            assert_eq!(*location, hitt_core::core::auth::ApiKeyLocation::QueryParam);
        }
        other => panic!("expected API key auth, got {other:?}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Assertions
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn assertion_add_edit_toggle_delete() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Assertions);

    // New form defaults to "status equals 200"; change it to 201.
    press(&mut app, key(KeyCode::Char('a'))).await;
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("New assertion"));
    for _ in 0..3 {
        press(&mut app, key(KeyCode::Backspace)).await;
    }
    type_str(&mut app, "201").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert!(matches!(
        app.active_tab().request.assertions[0].kind,
        AssertionKind::StatusEquals(201)
    ));

    // Second assertion: switch kind to "JSONPath equals".
    press(&mut app, key(KeyCode::Char('a'))).await;
    press(&mut app, key(KeyCode::Up)).await; // to Kind
    for _ in 0..3 {
        press(&mut app, key(KeyCode::Right)).await;
    }
    press(&mut app, key(KeyCode::Down)).await;
    press(&mut app, key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL)).await;
    type_str(&mut app, "$.Name").await;
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "Bob").await;
    press(&mut app, key(KeyCode::Enter)).await;
    match &app.active_tab().request.assertions[1].kind {
        AssertionKind::BodyPathEquals { path, expected } => {
            assert_eq!(path, "$.Name");
            assert_eq!(expected, &serde_json::json!("Bob"));
        }
        other => panic!("unexpected kind {other:?}"),
    }

    // Invalid number keeps the form open.
    press(&mut app, key(KeyCode::Char('k'))).await;
    press(&mut app, key(KeyCode::Enter)).await;
    type_str(&mut app, "abc").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.mode, AppMode::Insert);
    press(&mut app, key(KeyCode::Esc)).await;
    assert!(matches!(
        app.active_tab().request.assertions[0].kind,
        AssertionKind::StatusEquals(201)
    ));

    press(&mut app, key(KeyCode::Char(' '))).await;
    assert!(!app.active_tab().request.assertions[0].enabled);
    press(&mut app, key(KeyCode::Char('x'))).await;
    assert_eq!(app.active_tab().request.assertions.len(), 1);
    draw(&mut app, 80, 24);
}

// ════════════════════════════════════════════════════════════════════════════
// Environment / collection variables
// ════════════════════════════════════════════════════════════════════════════

fn saved_env(tmp: &TempDir, name: &str) -> Option<String> {
    std::fs::read_to_string(tmp.path().join("environments").join(format!("{name}.json"))).ok()
}

#[tokio::test]
async fn env_var_edit_persists() {
    let (mut app, tmp) = test_app();
    let mut env = Environment::new("dev");
    env.add_variable("host", "localhost");
    app.environments.push(env);
    app.active_env = Some(0);

    press(&mut app, char_key('E')).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));

    // Add API_KEY, then mark it secret.
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "API_KEY").await;
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "S3cretValue").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));
    press(&mut app, key(KeyCode::Char('s'))).await;

    // Edit the first variable's value.
    press(&mut app, key(KeyCode::Char('g'))).await;
    press(&mut app, key(KeyCode::Enter)).await;
    press(&mut app, key(KeyCode::Tab)).await;
    press(&mut app, key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL)).await;
    type_str(&mut app, "API.example.com").await;
    press(&mut app, key(KeyCode::Enter)).await;

    let env = &app.environments[0];
    assert_eq!(env.get("host"), Some("API.example.com"));
    assert_eq!(env.get("API_KEY"), Some("S3cretValue"));
    assert!(env.values[1].secret);

    // Short secrets are masked completely (no trailing characters shown).
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("********"));
    assert!(!screen.contains("alue"));

    let saved = saved_env(&tmp, "dev").expect("environment saved");
    assert!(saved.contains("API.example.com"));
    assert!(saved.contains("API_KEY"));

    // Toggle + delete also persist; Esc closes.
    press(&mut app, key(KeyCode::Char(' '))).await;
    assert!(!app.environments[0].values[0].enabled);
    press(&mut app, key(KeyCode::Char('x'))).await;
    assert_eq!(app.environments[0].values.len(), 1);
    assert!(!saved_env(&tmp, "dev").unwrap().contains("API.example.com"));
    press(&mut app, key(KeyCode::Esc)).await;
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn session_env_is_not_persisted() {
    let (mut app, tmp) = test_app();
    let env = Environment::new("local.env");
    app.session_env_ids.insert(env.id);
    app.environments.push(env);
    app.active_env = Some(0);

    execute_command(&mut app, "envedit").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "K").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.environments[0].values.len(), 1);
    assert!(saved_env(&tmp, "local.env").is_none());
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("session-only"));
}

#[tokio::test]
async fn env_editor_creates_environment_when_none() {
    let (mut app, tmp) = test_app();
    press(&mut app, char_key('E')).await;
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("No environment selected"));

    press(&mut app, key(KeyCode::Char('n'))).await;
    type_str(&mut app, "Staging").await;
    draw(&mut app, 120, 40);
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.environments.len(), 1);
    assert_eq!(app.active_env, Some(0));
    assert!(saved_env(&tmp, "Staging").is_some());
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));
}

#[tokio::test]
async fn collection_vars_edit_from_modal() {
    let (mut app, tmp) = test_app();
    app.collections.push(Collection::new("Shop"));
    app.active_tab_mut().collection_index = Some(0);

    execute_command(&mut app, "colvars").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "baseUrl").await;
    press(&mut app, key(KeyCode::Tab)).await;
    type_str(&mut app, "https://Shop.test").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.collections[0].variables[0].value, "https://Shop.test");
    let saved = std::fs::read_to_string(tmp.path().join("collections").join("Shop.json")).unwrap();
    assert!(saved.contains("https://Shop.test"));

    // c switches back to environment variables.
    press(&mut app, key(KeyCode::Char('c'))).await;
    assert_eq!(
        app.editor.env_scope,
        hitt_tui::editor::VarScope::Environment
    );
}

#[tokio::test]
async fn colvars_without_collection_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "colvars").await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.notification.is_some());
}

// ════════════════════════════════════════════════════════════════════════════
// Rendering
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn editors_render_in_small_terminals() {
    let (mut app, _tmp) = test_app();
    for tab in RequestTabKind::all() {
        on_request_tab(&mut app, *tab);
        // Open each editor (adds a row / form) and draw tiny and normal sizes.
        if matches!(tab, RequestTabKind::Body | RequestTabKind::Auth) {
            press(&mut app, key(KeyCode::Char('t'))).await;
        }
        // Auth: inherit -> none -> bearer (something with fields to edit).
        if *tab == RequestTabKind::Auth {
            press(&mut app, key(KeyCode::Char('t'))).await;
        }
        press(&mut app, key(KeyCode::Char('i'))).await;
        type_str(&mut app, "Ab").await;
        draw(&mut app, 40, 12);
        draw(&mut app, 120, 40);
        press(&mut app, key(KeyCode::Esc)).await;
        draw(&mut app, 20, 8);
    }
    press(&mut app, char_key('E')).await;
    draw(&mut app, 30, 10);
}

// ════════════════════════════════════════════════════════════════════════════
// Regression tests (QA round)
// ════════════════════════════════════════════════════════════════════════════

/// Local HTTP server answering 200 with the raw request head as the body
/// (`application/json` with a fixed body for `/json`).
async fn spawn_echo_server() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
                let (ct, body) = if path == "/json" {
                    (
                        "application/json",
                        r#"{"zeta":1,"alpha":2,"mid":3}"#.to_string(),
                    )
                } else {
                    ("text/plain", head)
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {ct}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });
    base
}

/// A port nothing listens on (bound, then released).
fn closed_port_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/")
}

fn response_text(app: &App) -> String {
    app.active_tab()
        .response
        .as_ref()
        .and_then(|r| r.body_text())
        .unwrap_or_default()
        .to_lowercase()
}

/// App with a collection whose auth is a bearer token, and a tab on one of
/// its requests (no auth of its own = inherit).
fn app_with_collection_auth(url: &str) -> (App, TempDir) {
    let (mut app, tmp) = test_app();
    let mut coll = Collection::new("Shop");
    coll.auth = Some(AuthConfig::bearer("colltoken"));
    let req = hitt_core::core::request::Request::new(
        "Get",
        hitt_core::core::request::HttpMethod::GET,
        url,
    );
    coll.add_request(req.clone());
    app.collections.push(coll);
    app.tabs[0] = hitt_tui::app::RequestTab::from_request(req, Some(0));
    (app, tmp)
}

#[tokio::test]
async fn no_auth_does_not_send_inherited_collection_token() {
    let base = spawn_echo_server().await;
    let (mut app, _tmp) = app_with_collection_auth(&format!("{base}/"));
    on_request_tab(&mut app, RequestTabKind::Auth);

    // Default: inherit, with the effective auth shown.
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("Inherit from parent"));
    assert!(screen.contains("Effective: Bearer Token"));
    app.send_request().await.unwrap();
    assert!(response_text(&app).contains("authorization: bearer colltoken"));

    // t: No Auth -> explicitly nothing, even inside the collection.
    press(&mut app, key(KeyCode::Char('t'))).await;
    assert!(matches!(
        app.active_tab().request.auth,
        Some(AuthConfig::None)
    ));
    assert!(app.active_tab().dirty);
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("No Auth"));
    assert!(!screen.contains("Inherit from parent"));
    app.send_request().await.unwrap();
    assert!(!response_text(&app).contains("authorization"));
}

#[tokio::test]
async fn abandon_editing_drops_new_row_and_resets_state() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Params);
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "MyKey").await;
    assert_eq!(app.active_tab().request.params.len(), 1);

    hitt_tui::event::abandon_editing(&mut app);
    assert!(app.active_tab().request.params.is_empty());
    assert!(app.editor.form.is_none());
    assert_eq!(app.mode, AppMode::Normal);
    let screen = draw(&mut app, 120, 40);
    assert!(!screen.contains("MyKey"));

    // Body textarea: typed text is kept.
    on_request_tab(&mut app, RequestTabKind::Body);
    press(&mut app, key(KeyCode::Char('t'))).await; // json
    press(&mut app, key(KeyCode::Char('i'))).await;
    type_str(&mut app, "{}").await;
    hitt_tui::event::abandon_editing(&mut app);
    assert!(app.editor.body.is_none());
    assert_eq!(app.mode, AppMode::Normal);
    assert!(matches!(
        &app.active_tab().request.body,
        Some(RequestBody::Json(t)) if t == "{}"
    ));
}

#[tokio::test]
async fn focus_moving_away_from_new_row_form_removes_it() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Params);
    press(&mut app, key(KeyCode::Char('a'))).await;
    assert_eq!(app.mode, AppMode::Insert);
    // Focus changed without the editor's own keys (e.g. a click on the URL).
    app.focus = FocusArea::UrlBar;
    type_str(&mut app, "x").await;
    assert!(app.active_tab().request.params.is_empty());
    assert!(app.editor.form.is_none());
    // The key went to the newly focused URL instead.
    assert_eq!(app.active_tab().request.url, "x");
}

#[tokio::test]
async fn dirty_tabs_show_marker_and_method_cycle_sets_dirty() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.name = "Orders".into();
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("1 Orders"));
    assert!(!screen.contains("● Orders"));

    app.focus = FocusArea::UrlBar;
    app.nav_mode = NavMode::Panel;
    press(&mut app, key(KeyCode::Char('m'))).await;
    assert_eq!(
        app.active_tab().request.method,
        hitt_core::core::request::HttpMethod::POST
    );
    assert!(app.active_tab().dirty);
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("1 ● Orders"));
}

#[tokio::test]
async fn url_editing_has_cursor_and_scrolls() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::UrlBar;
    app.nav_mode = NavMode::Panel;
    press(&mut app, key(KeyCode::Char('i'))).await;
    assert_eq!(app.mode, AppMode::Insert);

    let long = "https://api.example.com/v1/some/very/long/path/segment/TAILEND";
    type_str(&mut app, long).await;
    assert_eq!(app.active_tab().request.url, long);

    // Narrow terminal: the end of the URL (where the cursor is) stays visible.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 30)).unwrap();
    terminal
        .draw(|frame| hitt_tui::ui::layout::render(&mut app, frame))
        .unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(screen.contains("TAILEND"));
    let cursor = terminal.get_cursor_position().unwrap();
    let url_rect = app.regions.url_bar.unwrap();
    assert_eq!(cursor.y, url_rect.y + 1);
    assert!(cursor.x > url_rect.x && cursor.x < url_rect.x + url_rect.width);

    // Home + typing inserts at the start; End/Left/Backspace edit in place.
    press(&mut app, key(KeyCode::Home)).await;
    type_str(&mut app, "X").await;
    assert!(app.active_tab().request.url.starts_with("Xhttps://"));
    press(&mut app, key_mod(KeyCode::Char('e'), KeyModifiers::CONTROL)).await;
    press(&mut app, key(KeyCode::Left)).await;
    press(&mut app, key(KeyCode::Backspace)).await;
    assert!(app.active_tab().request.url.ends_with("TAILED"));
    press(&mut app, key_mod(KeyCode::Char('a'), KeyModifiers::CONTROL)).await;
    press(&mut app, key(KeyCode::Delete)).await;
    assert!(app.active_tab().request.url.starts_with("https://"));

    // Rendering after moving to the start shows the beginning again.
    let screen = draw(&mut app, 70, 30);
    assert!(screen.contains("https://api"));

    press(&mut app, key(KeyCode::Esc)).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.editor.url_cursor.is_none());
}

#[tokio::test]
async fn ctrl_r_sends_while_typing_url() {
    let base = spawn_echo_server().await;
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::UrlBar;
    app.nav_mode = NavMode::Panel;
    press(&mut app, key(KeyCode::Char('i'))).await;
    type_str(&mut app, &format!("{base}/typed")).await;
    press(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL)).await;
    assert!(app.active_tab().response.is_some());
    assert!(response_text(&app).contains("get /typed"));
    // Still typing in the URL.
    assert_eq!(app.mode, AppMode::Insert);
}

#[tokio::test]
async fn failed_send_clears_previous_response_and_shows_error() {
    let base = spawn_echo_server().await;
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.url = format!("{base}/ok");
    app.send_request().await.unwrap();
    assert!(app.active_tab().response.is_some());

    app.active_tab_mut().request.url = closed_port_url();
    app.send_request().await.unwrap();
    assert!(app.active_tab().response.is_none());
    assert!(app.active_tab().last_error.is_some());
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("✕ Request failed"));
    assert!(!screen.contains("200 OK"));

    // A later success clears the error.
    app.active_tab_mut().request.url = format!("{base}/ok");
    app.send_request().await.unwrap();
    assert!(app.active_tab().last_error.is_none());
}

#[tokio::test]
async fn assertion_kind_change_does_not_carry_status_default() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Assertions);
    press(&mut app, key(KeyCode::Char('a'))).await;
    let form = app.editor.form.as_ref().unwrap();
    assert_eq!(form.fields[1].input.value, "200");

    // Up to the kind, then 6 x Right: status equals -> body contains.
    press(&mut app, key(KeyCode::Up)).await;
    for _ in 0..6 {
        press(&mut app, key(KeyCode::Right)).await;
    }
    let form = app.editor.form.as_ref().unwrap();
    assert_eq!(form.fields[0].value(), "body contains");
    assert_eq!(form.fields[1].label, "Text");
    assert_eq!(form.fields[1].input.value, "");
}

#[tokio::test]
async fn body_type_cycle_restores_text_and_content_type() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Body);
    app.active_tab_mut().request.body = Some(RequestBody::Raw {
        content: "<a/>".into(),
        content_type: "application/xml".into(),
    });
    // raw -> form-urlencoded: add a row there.
    press(&mut app, key(KeyCode::Char('t'))).await;
    press(&mut app, key(KeyCode::Char('a'))).await;
    type_str(&mut app, "k").await;
    press(&mut app, key(KeyCode::Enter)).await;
    // form-data, graphql, none, json, raw
    for _ in 0..5 {
        press(&mut app, key(KeyCode::Char('t'))).await;
    }
    assert!(matches!(
        &app.active_tab().request.body,
        Some(RequestBody::Raw { content, content_type })
            if content == "<a/>" && content_type == "application/xml"
    ));
    press(&mut app, key(KeyCode::Char('t'))).await;
    assert!(matches!(
        &app.active_tab().request.body,
        Some(RequestBody::FormUrlEncoded(rows)) if rows.len() == 1 && rows[0].key == "k"
    ));
}

#[tokio::test]
async fn short_auth_secrets_are_fully_masked() {
    let (mut app, _tmp) = test_app();
    on_request_tab(&mut app, RequestTabKind::Auth);
    app.active_tab_mut().request.auth = Some(AuthConfig::bearer("abc123"));
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("******"));
    assert!(!screen.contains("c123"));
}

#[tokio::test]
async fn env_delete_declined_returns_to_env_editor() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("dev"));
    app.active_env = Some(0);
    press(&mut app, char_key('E')).await;
    press(&mut app, char_key('D')).await;
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    press(&mut app, key(KeyCode::Char('n'))).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));
    assert_eq!(app.environments.len(), 1);

    // Esc also returns to the editor.
    press(&mut app, char_key('D')).await;
    press(&mut app, key(KeyCode::Esc)).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));

    // From the command line, declining goes back to normal mode.
    press(&mut app, key(KeyCode::Esc)).await;
    execute_command(&mut app, "delenv").await.unwrap();
    press(&mut app, key(KeyCode::Char('n'))).await;
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn unresolved_variables_warn_but_still_send() {
    let base = spawn_echo_server().await;
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.url = format!("{base}/{{{{missing}}}}");
    app.active_tab_mut()
        .request
        .headers
        .push(hitt_core::core::request::KeyValuePair::new(
            "X-T",
            "{{other}}",
        ));
    app.send_request().await.unwrap();
    assert!(app.active_tab().response.is_some());
    let n = app.notification.as_ref().expect("warning toast");
    assert_eq!(n.kind, hitt_tui::app::NotificationKind::Warning);
    assert_eq!(n.message, "Unresolved variables: missing, other");
}

#[tokio::test]
async fn response_tabs_timing_and_json_order() {
    let base = spawn_echo_server().await;
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.url = format!("{base}/json");
    app.send_request().await.unwrap();

    // Server key order is kept.
    let screen = draw(&mut app, 120, 40);
    let zeta = screen.find("\"zeta\"").expect("zeta shown");
    let alpha = screen.find("\"alpha\"").expect("alpha shown");
    assert!(zeta < alpha);

    // Compact strips (request and response) both say "Tests"; wide ones
    // spell out "Assertions".
    let narrow = draw(&mut app, 60, 30);
    assert!(narrow.contains("Tests"), "{narrow}");
    assert!(!narrow.contains("Assert"), "{narrow}");
    assert!(draw(&mut app, 160, 40).contains("Assertions"));

    // Timing values line up in one column.
    app.active_tab_mut().response_tab = hitt_tui::app::ResponseTabKind::Timing;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| hitt_tui::ui::layout::render(&mut app, frame))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    // One Vec of cell symbols per screen row, so columns are cell indices.
    let rows: Vec<Vec<String>> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect()
        })
        .collect();
    let value_col = |label: &str| {
        let row = rows
            .iter()
            .find(|r| r.concat().contains(label))
            .expect(label);
        let label_cells: Vec<String> = label.chars().map(String::from).collect();
        let start = (0..row.len())
            .find(|&i| row[i..].starts_with(&label_cells))
            .unwrap()
            + label_cells.len();
        start + row[start..].iter().take_while(|c| *c == " ").count()
    };
    let dns = value_col("DNS Lookup:");
    assert_eq!(value_col("Content Download:"), dns);
    assert_eq!(value_col("Total:"), dns);
}

#[tokio::test]
async fn save_updates_updated_at() {
    let (mut app, _tmp) = app_with_collection_auth("http://example.test/");
    let before = app.active_tab().request.updated_at;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    app.active_tab_mut().request.url.push('x');
    app.active_tab_mut().dirty = true;
    execute_command(&mut app, "save").await.unwrap();
    assert!(!app.active_tab().dirty);
    let saved = app.collections[0]
        .find_request(&app.active_tab().request.id)
        .unwrap();
    assert!(saved.updated_at > before);
}
