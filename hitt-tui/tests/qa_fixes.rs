//! Regression tests for QA-reported command, keymap and layout bugs.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use hitt_core::core::chain::{ChainStepStatus, RequestChain};
use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::history::HistoryEntry;
use hitt_core::core::request::{HttpMethod, Protocol, Request};
use hitt_core::storage::config::AppConfig;
use hitt_tui::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind, RequestTab, ResponseTabKind,
    SidebarSection,
};
use hitt_tui::event::{execute_command, handle_event, AppEvent, ChainStepEvent};

fn test_app() -> (App, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    (App::new(config).unwrap(), tmp)
}

/// App whose config changes are written to `<tmp>/config.toml`.
fn test_app_with_config_file() -> (App, TempDir) {
    let (mut app, tmp) = test_app();
    app.config_path = Some(tmp.path().join("config.toml"));
    (app, tmp)
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, modifiers))
}

async fn press(app: &mut App, event: AppEvent) {
    handle_event(app, event).await.unwrap();
}

async fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, key(KeyCode::Char(c))).await;
    }
}

fn message(app: &App) -> String {
    app.notification
        .as_ref()
        .map(|n| n.message.clone())
        .unwrap_or_default()
}

fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| hitt_tui::ui::layout::render(app, f))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A collection "API" with requests "Users" and "Orders", expanded in the sidebar.
fn app_with_collection() -> (App, TempDir) {
    let (mut app, tmp) = test_app();
    let mut coll = Collection::new("API");
    coll.add_request(Request::new("Users", HttpMethod::GET, "http://x/users"));
    coll.add_request(Request::new("Orders", HttpMethod::POST, "http://x/orders"));
    app.sidebar_state.expanded.insert(coll.id);
    app.collections.push(coll);
    app.focus = FocusArea::Sidebar;
    app.nav_mode = NavMode::Panel;
    (app, tmp)
}

// ── A: chain runs can be stopped; stale events are ignored ─────────────────

#[tokio::test]
async fn chain_esc_cancels_run_and_ignores_stale_events() {
    let (mut app, _tmp) = test_app();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    let mut coll = Collection::new("A");
    // Unroutable address: the step stays in flight until cancelled.
    let req = Request::new("Slow", HttpMethod::GET, "http://10.255.255.1:9/slow");
    let mut chain = RequestChain::new("C");
    chain.add_step(req.id);
    chain.add_step(req.id);
    coll.add_request(req);
    coll.chains.push(chain);
    app.collections.push(coll);

    execute_command(&mut app, "chain C").await.unwrap();
    assert_eq!(app.mode, AppMode::ChainEditor);
    let old_run = app.chain_run_id.unwrap();

    // First Esc stops the run but keeps the runner open.
    press(&mut app, key(KeyCode::Esc)).await;
    assert_eq!(app.mode, AppMode::ChainEditor);
    let state = app.active_chain.as_ref().unwrap();
    assert!(!state.running);
    assert!(state
        .step_statuses
        .iter()
        .all(|s| matches!(s, ChainStepStatus::Skipped { .. })));

    // A late event of the cancelled run changes nothing.
    press(
        &mut app,
        AppEvent::ChainStepComplete {
            run_id: old_run,
            event: ChainStepEvent::Success {
                step_index: 0,
                status: 200,
                duration_ms: 1,
                extracted: std::collections::HashMap::new(),
            },
        },
    )
    .await;
    assert!(matches!(
        app.active_chain.as_ref().unwrap().step_statuses[0],
        ChainStepStatus::Skipped { .. }
    ));

    // A new run doesn't see events from the old one either.
    press(&mut app, key(KeyCode::Esc)).await;
    assert_eq!(app.mode, AppMode::Normal);
    execute_command(&mut app, "chain C").await.unwrap();
    assert_ne!(app.chain_run_id, Some(old_run));
    press(
        &mut app,
        AppEvent::ChainStepComplete {
            run_id: old_run,
            event: ChainStepEvent::Complete,
        },
    )
    .await;
    assert!(app.active_chain.as_ref().unwrap().running);
    press(&mut app, key(KeyCode::Esc)).await;
}

#[tokio::test]
async fn chain_marks_first_step_running_when_it_starts() {
    let (mut app, _tmp) = test_app();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    let mut coll = Collection::new("A");
    let req = Request::new("Slow", HttpMethod::GET, "http://10.255.255.1:9/slow");
    let mut chain = RequestChain::new("C");
    chain.add_step(req.id);
    coll.add_request(req);
    coll.chains.push(chain);
    app.collections.push(coll);

    execute_command(&mut app, "chain C").await.unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    press(&mut app, event).await;
    assert!(matches!(
        app.active_chain.as_ref().unwrap().step_statuses[0],
        ChainStepStatus::Running
    ));
    press(&mut app, key(KeyCode::Esc)).await;
}

// ── B: duplicate names ──────────────────────────────────────────────────────

#[tokio::test]
async fn newenv_with_existing_name_is_rejected_and_keeps_vars() {
    let (mut app, tmp) = test_app();
    execute_command(&mut app, "newenv dev").await.unwrap();
    app.environments[0].add_variable("TOKEN", "secret");
    execute_command(&mut app, "dupenv dev").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
    execute_command(&mut app, "newenv dev").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
    assert!(message(&app).contains("already exists"));
    assert_eq!(app.environments.len(), 1);
    assert_eq!(app.environments[0].get("TOKEN"), Some("secret"));
    let _ = tmp;
}

#[tokio::test]
async fn newcol_and_rename_reject_duplicate_collection_names() {
    let (mut app, _tmp) = app_with_collection();
    execute_command(&mut app, "newcol api").await.unwrap();
    assert_eq!(app.collections.len(), 1);
    assert!(message(&app).contains("already exists"));

    execute_command(&mut app, "newcol Other").await.unwrap();
    assert_eq!(app.collections.len(), 2);
    // Rename "Other" (index 1) to "API" from the sidebar.
    app.mode = AppMode::Modal(ModalKind::RenameCollection(1));
    app.rename_input = "API".into();
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.collections[1].name, "Other");
    assert!(message(&app).contains("already exists"));
    assert_eq!(app.mode, AppMode::Modal(ModalKind::RenameCollection(1)));
}

#[tokio::test]
async fn reimporting_an_exported_collection_gets_a_unique_name() {
    let (mut app, tmp) = app_with_collection();
    let path = tmp.path().join("api.json");
    execute_command(&mut app, &format!("export {}", path.display()))
        .await
        .unwrap();
    execute_command(&mut app, &format!("import {}", path.display()))
        .await
        .unwrap();
    let names: Vec<&str> = app.collections.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["API", "API (2)"]);
}

// ── C: search selection, smart case, reuse open tabs ────────────────────────

#[tokio::test]
async fn search_is_case_insensitive_and_enter_opens_selected() {
    let (mut app, _tmp) = app_with_collection();
    app.focus = FocusArea::UrlBar;
    press(&mut app, key(KeyCode::Char('/'))).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Search));
    type_str(&mut app, "HTTP://X").await;
    assert_eq!(app.search_results.len(), 2);

    press(&mut app, key(KeyCode::Down)).await;
    assert_eq!(app.search_selected, 1);
    press(&mut app, key_mod(KeyCode::Char('p'), KeyModifiers::CONTROL)).await;
    assert_eq!(app.search_selected, 0);
    press(&mut app, key(KeyCode::Tab)).await;
    let screen = draw(&mut app, 100, 30);
    assert!(screen.contains("> POST"), "{screen}");

    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.active_tab().request.name, "Orders");
    let tabs = app.tabs.len();

    // Opening it again switches to the existing tab.
    app.switch_tab(0);
    press(&mut app, key(KeyCode::Char('/'))).await;
    type_str(&mut app, "orders").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.tabs.len(), tabs);
    assert_eq!(app.active_tab().request.name, "Orders");
}

// ── D: sidebar l / h / Enter, rename updates tab, chain deletion, g / G ─────

#[tokio::test]
async fn sidebar_l_expands_h_goes_to_parent_and_enter_reuses_tab() {
    let (mut app, _tmp) = app_with_collection();
    let coll_id = app.collections[0].id;
    // `l` on an expanded collection keeps it expanded.
    press(&mut app, key(KeyCode::Char('l'))).await;
    assert!(app.sidebar_state.expanded.contains(&coll_id));
    // Enter toggles.
    press(&mut app, key(KeyCode::Enter)).await;
    assert!(!app.sidebar_state.expanded.contains(&coll_id));
    press(&mut app, key(KeyCode::Char('l'))).await;
    assert!(app.sidebar_state.expanded.contains(&coll_id));

    // G / g jump to the last / first row.
    press(&mut app, key_mod(KeyCode::Char('G'), KeyModifiers::SHIFT)).await;
    assert_eq!(app.sidebar_state.selected, 2);
    // `h` on a request goes to its collection row.
    press(&mut app, key(KeyCode::Char('h'))).await;
    assert_eq!(app.sidebar_state.selected, 0);
    assert!(app.sidebar_state.expanded.contains(&coll_id));
    press(&mut app, key(KeyCode::Char('h'))).await;
    assert!(!app.sidebar_state.expanded.contains(&coll_id));
    press(&mut app, key(KeyCode::Char('l'))).await;
    press(&mut app, key_mod(KeyCode::Char('G'), KeyModifiers::SHIFT)).await;
    press(&mut app, key(KeyCode::Char('g'))).await;
    assert_eq!(app.sidebar_state.selected, 0);

    // `l` on a request opens it; opening it again reuses the tab.
    app.sidebar_state.selected = 1;
    press(&mut app, key(KeyCode::Char('l'))).await;
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab().request.name, "Users");
    app.focus = FocusArea::Sidebar;
    app.switch_tab(0);
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
}

#[tokio::test]
async fn renaming_a_request_updates_its_open_tab() {
    let (mut app, _tmp) = app_with_collection();
    app.sidebar_state.selected = 1;
    press(&mut app, key(KeyCode::Enter)).await; // open "Users"
    app.focus = FocusArea::Sidebar;
    press(&mut app, key(KeyCode::Char('r'))).await;
    assert!(matches!(
        app.mode,
        AppMode::Modal(ModalKind::RenameRequest { .. })
    ));
    app.rename_input.clear();
    type_str(&mut app, "People").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.active_tab().request.name, "People");
    assert_eq!(app.active_tab().title(), "People");
}

#[tokio::test]
async fn x_in_chains_section_deletes_chain_after_confirm() {
    let (mut app, _tmp) = app_with_collection();
    app.collections[0]
        .chains
        .push(RequestChain::new("Login flow"));
    app.sidebar_state.section = SidebarSection::Chains;
    app.sidebar_state.selected = 0;
    let screen = draw(&mut app, 120, 30);
    assert!(screen.contains("x:delete"), "{screen}");
    assert!(!screen.contains("R:run"), "{screen}");

    press(&mut app, key(KeyCode::Char('x'))).await;
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    press(&mut app, key(KeyCode::Char('y'))).await;
    assert!(app.collections[0].chains.is_empty());
}

// ── E: destructive commands ask first ───────────────────────────────────────

#[tokio::test]
async fn delcol_without_name_asks_before_deleting() {
    let (mut app, _tmp) = app_with_collection();
    app.sidebar_state.selected = 0;
    execute_command(&mut app, "delcol").await.unwrap();
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    press(&mut app, key(KeyCode::Char('n'))).await;
    assert_eq!(app.collections.len(), 1);
}

// ── F: tiny terminals never panic ───────────────────────────────────────────

#[tokio::test]
async fn rendering_never_panics_on_tiny_terminals() {
    let (mut app, _tmp) = app_with_collection();
    app.environments.push(Environment::new("dev"));
    let modes = [
        AppMode::Normal,
        AppMode::Command,
        AppMode::Insert,
        AppMode::ChainEditor,
        AppMode::Modal(ModalKind::Search),
        AppMode::Modal(ModalKind::Help),
        AppMode::Modal(ModalKind::Confirm("Delete everything?".into())),
        AppMode::Modal(ModalKind::Import),
        AppMode::Modal(ModalKind::Export),
        AppMode::Modal(ModalKind::DiffSelector),
        AppMode::Modal(ModalKind::RenameTab),
        AppMode::Modal(ModalKind::CollectionPicker),
        AppMode::Modal(ModalKind::RenameCollection(0)),
        AppMode::Modal(ModalKind::ThemePicker),
        AppMode::Modal(ModalKind::EnvironmentEdit),
        AppMode::Modal(ModalKind::Runner),
    ];
    let mut sizes: Vec<(u16, u16)> = (1..=6).flat_map(|w| (1..=6).map(move |h| (w, h))).collect();
    sizes.extend([(80, 3), (80, 2), (80, 1), (3, 80), (200, 4)]);
    app.command_input = "a long command line that does not fit".into();
    app.modal_input = "/a/very/long/path/that/needs/scrolling/file.json".into();
    for mode in &modes {
        for &(w, h) in &sizes {
            app.mode = mode.clone();
            app.notify("Something happened".into(), NotificationKind::Info);
            let _ = draw(&mut app, w, h);
        }
    }
}

// ── G: content-sized modals at 80x24 ────────────────────────────────────────

#[tokio::test]
async fn rename_and_confirm_modals_fit_their_content_at_80x24() {
    let (mut app, _tmp) = app_with_collection();
    app.mode = AppMode::Modal(ModalKind::RenameCollection(0));
    app.rename_input = "New name".into();
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains("Enter new name"), "{screen}");
    assert!(screen.contains("New name█"), "{screen}");
    assert!(screen.contains("Enter:confirm"), "{screen}");

    app.mode = AppMode::Modal(ModalKind::Confirm(
        "'A request with a long name' has unsaved changes. Close anyway?".into(),
    ));
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains("Close anyway?"), "{screen}");
    assert!(screen.contains("[Y]es"), "{screen}");
    assert!(screen.contains("[N]o"), "{screen}");
}

#[tokio::test]
async fn import_modal_shows_the_end_of_long_paths() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Modal(ModalKind::Import);
    app.modal_input = format!("/{}/end-of-path.json", "x".repeat(150));
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains("end-of-path.json█"), "{screen}");
}

// ── H: mode-aware status hints ──────────────────────────────────────────────

#[tokio::test]
async fn status_hints_follow_the_mode() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::UrlBar;
    app.nav_mode = NavMode::Panel;
    press(&mut app, key(KeyCode::Char('i'))).await;
    assert_eq!(app.mode, AppMode::Insert);
    let bar = draw(&mut app, 120, 24).lines().last().unwrap().to_string();
    assert!(!bar.contains("Esc:global"), "{bar}");
    assert!(!bar.contains("m:method"), "{bar}");
    assert!(bar.contains("Esc:done"), "{bar}");

    app.mode = AppMode::Command;
    let bar = draw(&mut app, 120, 24).lines().last().unwrap().to_string();
    assert!(bar.contains("Enter:run"), "{bar}");

    app.mode = AppMode::Modal(ModalKind::Confirm("Sure?".into()));
    let bar = draw(&mut app, 120, 24).lines().last().unwrap().to_string();
    assert!(bar.contains("y/Enter:yes"), "{bar}");
}

// ── I: tab switching resets scroll; response filter is per tab ──────────────

#[tokio::test]
async fn alt_number_resets_scroll_and_filter_is_per_tab() {
    let (mut app, _tmp) = test_app();
    app.new_tab();
    app.active_tab_mut().response_filter = "id".into();
    app.response_scroll = 40;
    app.request_scroll = 7;
    press(&mut app, key_mod(KeyCode::Char('1'), KeyModifiers::ALT)).await;
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.response_scroll, 0);
    assert_eq!(app.request_scroll, 0);
    assert!(app.active_tab().response_filter.is_empty());
    press(&mut app, key_mod(KeyCode::Char('2'), KeyModifiers::ALT)).await;
    assert_eq!(app.active_tab().response_filter, "id");
}

// ── J: help modal ──────────────────────────────────────────────────────────

#[tokio::test]
async fn help_closes_with_question_mark_and_reopens_at_top() {
    let (mut app, _tmp) = test_app();
    press(&mut app, key(KeyCode::Char('?'))).await;
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Help));
    press(&mut app, key_mod(KeyCode::Char('J'), KeyModifiers::SHIFT)).await;
    assert_eq!(app.help_scroll, 15);
    press(&mut app, key(KeyCode::Char('?'))).await;
    assert_eq!(app.mode, AppMode::Normal);
    press(&mut app, key(KeyCode::Char('?'))).await;
    assert_eq!(app.help_scroll, 0);

    // Every theme is listed.
    let mut screen = String::new();
    for _ in 0..40 {
        screen.push_str(&draw(&mut app, 100, 40));
        press(&mut app, key_mod(KeyCode::Char('J'), KeyModifiers::SHIFT)).await;
    }
    for theme in hitt_tui::ui::theme::AVAILABLE_THEMES {
        assert!(screen.contains(theme), "help lacks theme {theme}");
    }
    assert!(!screen.contains("vim_mode"));
    assert!(screen.contains(":proxy"));
}

// ── L: command mode ─────────────────────────────────────────────────────────

#[tokio::test]
async fn empty_command_does_nothing_and_history_recalls() {
    let (mut app, _tmp) = test_app();
    press(&mut app, key(KeyCode::Char(':'))).await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.notification.is_none());

    press(&mut app, key(KeyCode::Char(':'))).await;
    type_str(&mut app, "help").await;
    press(&mut app, key(KeyCode::Enter)).await;
    press(&mut app, key(KeyCode::Esc)).await;
    press(&mut app, key(KeyCode::Char(':'))).await;
    press(&mut app, key(KeyCode::Up)).await;
    assert_eq!(app.command_input, "help");
    press(&mut app, key(KeyCode::Down)).await;
    assert_eq!(app.command_input, "");
}

// ── M: q on an idle WS / SSE tab ────────────────────────────────────────────

#[tokio::test]
async fn q_on_unconnected_ws_tab_behaves_like_normal_q() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.protocol = Protocol::WebSocket;
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;
    press(&mut app, key(KeyCode::Char('q'))).await;
    assert_eq!(app.nav_mode, NavMode::Global);
    assert!(!message(&app).contains("Disconnected"));

    execute_command(&mut app, "ws-disconnect").await.unwrap();
    assert_eq!(message(&app), "Not connected");
    execute_command(&mut app, "sse-disconnect").await.unwrap();
    assert_eq!(message(&app), "Not connected");
}

// ── N: w on the only tab ────────────────────────────────────────────────────

#[tokio::test]
async fn w_on_the_only_tab_resets_it() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.url = "http://example.com".into();
    press(&mut app, key(KeyCode::Char('w'))).await;
    assert!(app.active_tab().is_blank());

    app.active_tab_mut().request.url = "http://example.com".into();
    app.active_tab_mut().dirty = true;
    press(&mut app, key(KeyCode::Char('w'))).await;
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    press(&mut app, key(KeyCode::Char('y'))).await;
    assert!(app.active_tab().is_blank());
    assert_eq!(app.tabs.len(), 1);
}

// ── O / W: command feedback, No Env ─────────────────────────────────────────

#[tokio::test]
async fn env_without_args_lists_envs_and_none_clears() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("dev"));
    app.environments.push(Environment::new("prod"));
    execute_command(&mut app, "env").await.unwrap();
    assert!(message(&app).contains("dev, prod"), "{}", message(&app));
    execute_command(&mut app, "env prod").await.unwrap();
    assert_eq!(app.active_env, Some(1));
    execute_command(&mut app, "env none").await.unwrap();
    assert_eq!(app.active_env, None);
}

#[tokio::test]
async fn replay_with_empty_history_says_so() {
    let (mut app, _tmp) = test_app();
    for cmd in ["replay 0", "replay abc"] {
        execute_command(&mut app, cmd).await.unwrap();
        assert_eq!(message(&app), "History is empty");
    }
}

// ── P: import / export by file name ─────────────────────────────────────────

#[tokio::test]
async fn import_detects_dotenv_names_and_rejects_unknown_types() {
    let (mut app, tmp) = test_app();
    let dotenv = tmp.path().join(".env");
    std::fs::write(&dotenv, "BASE_URL=http://localhost\n").unwrap();
    execute_command(&mut app, &format!("import {}", dotenv.display()))
        .await
        .unwrap();
    assert_eq!(app.environments.len(), 1);
    assert_eq!(
        app.environments[0].get("BASE_URL"),
        Some("http://localhost")
    );
    assert_eq!(app.tabs.len(), 1);

    let notes = tmp.path().join("notes.xyz");
    std::fs::write(&notes, "curl http://example.com").unwrap();
    execute_command(&mut app, &format!("import {}", notes.display()))
        .await
        .unwrap();
    assert!(
        message(&app).contains("Unsupported file type"),
        "{}",
        message(&app)
    );
    assert_eq!(app.tabs.len(), 1);
}

#[tokio::test]
async fn export_env_writes_dotenv_and_directories_are_rejected() {
    let (mut app, tmp) = test_app();
    let mut env = Environment::new("dev");
    env.add_variable("BASE_URL", "http://localhost");
    env.add_variable("GREETING", "hello world");
    app.environments.push(env);
    app.active_env = Some(0);

    let path = tmp.path().join("file.env");
    execute_command(&mut app, &format!("export {}", path.display()))
        .await
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("BASE_URL=http://localhost\n"), "{text}");
    assert!(text.contains("GREETING=\"hello world\"\n"), "{text}");
    assert!(!text.contains('{'), "{text}");

    execute_command(&mut app, &format!("export {}", tmp.path().display()))
        .await
        .unwrap();
    assert!(
        message(&app).contains("Path is a directory"),
        "{}",
        message(&app)
    );
}

#[tokio::test]
async fn importing_a_broken_chain_reports_the_chain_error() {
    let (mut app, _tmp) = test_app();
    app.collections.push(Collection::new("Empty"));
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_chain.yaml");
    execute_command(&mut app, &format!("import {}", fixture.display()))
        .await
        .unwrap();
    let msg = message(&app);
    assert!(!msg.contains("openapi"), "{msg}");
    assert!(msg.contains("chain"), "{msg}");
}

// ── S / T: :set persists; persist_history doesn't wipe the file ─────────────

#[tokio::test]
async fn set_changes_are_written_to_the_config_file() {
    let (mut app, tmp) = test_app_with_config_file();
    execute_command(&mut app, "set timeout 1234").await.unwrap();
    execute_command(&mut app, "set verify_ssl false")
        .await
        .unwrap();
    execute_command(&mut app, "set theme nord").await.unwrap();
    let on_disk = AppConfig::load_from(&tmp.path().join("config.toml")).unwrap();
    assert_eq!(on_disk.timeout_ms, 1234);
    assert!(!on_disk.verify_ssl);
    assert_eq!(on_disk.theme.to_string(), "nord");
}

#[tokio::test]
async fn toggling_persist_history_keeps_saved_history() {
    let (mut app, _tmp) = test_app();
    let path = app.history_path();
    let saved = HistoryEntry::new(HttpMethod::GET, "http://saved");
    hitt_core::storage::history_store::save_history(&path, std::slice::from_ref(&saved)).unwrap();

    execute_command(&mut app, "set persist_history false")
        .await
        .unwrap();
    let on_disk = hitt_core::storage::history_store::load_history(&path, 100).unwrap();
    assert_eq!(on_disk.len(), 1);

    app.history
        .add(HistoryEntry::new(HttpMethod::GET, "http://session"));
    execute_command(&mut app, "set persist_history true")
        .await
        .unwrap();
    let on_disk = hitt_core::storage::history_store::load_history(&path, 100).unwrap();
    let urls: Vec<&str> = on_disk.iter().map(|e| e.url.as_str()).collect();
    assert_eq!(urls, ["http://session", "http://saved"]);
}

// ── U: :addvar updates ──────────────────────────────────────────────────────

#[tokio::test]
async fn addvar_updates_an_existing_key() {
    let (mut app, _tmp) = app_with_collection();
    app.tabs[0] = RequestTab::from_request(Request::new("R", HttpMethod::GET, ""), Some(0));
    execute_command(&mut app, "addvar host a").await.unwrap();
    execute_command(&mut app, "addvar host b").await.unwrap();
    let vars = &app.collections[0].variables;
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].value, "b");
}

// ── V: SSE stream toggle in sync with the tab ───────────────────────────────

#[tokio::test]
async fn sse_a_after_number_key_toggles_immediately() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.protocol = Protocol::Sse;
    app.active_tab_mut().response_tab = ResponseTabKind::SseEvents;
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;
    press(&mut app, key(KeyCode::Char('2'))).await;
    assert_eq!(app.active_tab().response_tab, ResponseTabKind::SseStream);
    press(&mut app, key(KeyCode::Char('a'))).await;
    assert_eq!(app.active_tab().response_tab, ResponseTabKind::SseEvents);
}

// ── X: 80-column cosmetics ──────────────────────────────────────────────────

#[tokio::test]
async fn status_bar_keeps_the_focus_label_at_80_columns() {
    let (mut app, _tmp) = test_app();
    app.nav_mode = NavMode::Panel;
    for focus in [
        FocusArea::RequestBody,
        FocusArea::ResponseBody,
        FocusArea::Sidebar,
    ] {
        app.focus = focus;
        let bar = draw(&mut app, 80, 24).lines().last().unwrap().to_string();
        assert!(bar.contains(&focus.to_string()), "{bar}");
    }
}

#[tokio::test]
async fn request_sub_tabs_use_short_labels_when_narrow() {
    let (mut app, _tmp) = test_app();
    let req = &mut app.active_tab_mut().request;
    req.headers
        .push(hitt_core::core::request::KeyValuePair::new("A", "1"));
    req.params
        .push(hitt_core::core::request::KeyValuePair::new("p", "1"));
    req.assertions
        .push(hitt_core::testing::assertion_engine::Assertion::status_equals(200));
    let screen = draw(&mut app, 80, 24);
    assert!(!screen.contains("Asser "), "{screen}");
    assert!(
        screen.contains("Tests(1)") || screen.contains("Tests (1)"),
        "{screen}"
    );
}

// ── Unique chain names ──────────────────────────────────────────────────────

#[tokio::test]
async fn newchain_rejects_a_duplicate_name_in_the_collection() {
    let (mut app, _tmp) = app_with_collection();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    execute_command(&mut app, "newchain slow").await.unwrap();
    execute_command(&mut app, "newchain SLOW").await.unwrap();
    assert_eq!(app.collections[0].chains.len(), 1);
    assert!(
        message(&app).contains("already exists"),
        "{}",
        message(&app)
    );
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );

    // `:addstep` and `:chain` target that one chain.
    execute_command(&mut app, "addstep Users").await.unwrap();
    execute_command(&mut app, "chain Slow").await.unwrap();
    let started = app.active_chain_def.as_ref().unwrap();
    assert_eq!(started.id, app.collections[0].chains[0].id);
    assert_eq!(started.steps.len(), 1);
}

#[tokio::test]
async fn chain_command_prefers_the_active_chain_among_legacy_duplicates() {
    // Data saved by older versions may hold two chains with the same name.
    let (mut app, _tmp) = app_with_collection();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    let empty = RequestChain::new("slow");
    let mut full = RequestChain::new("slow");
    full.add_step(app.collections[0].all_requests()[0].id);
    let full_id = full.id;
    app.collections[0].chains.push(empty);
    app.collections[0].chains.push(full);
    app.selected_chain = Some(full_id);

    execute_command(&mut app, "chain slow").await.unwrap();
    assert_eq!(app.active_chain_def.as_ref().unwrap().id, full_id);
}

#[tokio::test]
async fn importing_the_same_chain_twice_renames_the_second() {
    let (mut app, _tmp) = test_app();
    let mut coll = Collection::new("API");
    coll.add_request(Request::new("login", HttpMethod::POST, "/login"));
    coll.add_request(Request::new("get-user", HttpMethod::GET, "/users/1"));
    app.collections.push(coll);
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_chain.yaml");
    for _ in 0..2 {
        execute_command(&mut app, &format!("importchain {}", fixture.display()))
            .await
            .unwrap();
    }
    let names: Vec<&str> = app.collections[0]
        .chains
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Login Flow", "Login Flow (2)"]);
}

// ── :env-file uses the same unique naming as :import ────────────────────────

#[tokio::test]
async fn env_file_twice_gets_a_unique_name() {
    let (mut app, tmp) = test_app();
    let path = tmp.path().join(".env");
    std::fs::write(&path, "A=1\n").unwrap();
    execute_command(&mut app, &format!("env-file {}", path.display()))
        .await
        .unwrap();
    execute_command(&mut app, &format!("env-file {}", path.display()))
        .await
        .unwrap();
    let names: Vec<&str> = app.environments.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, [".env", ".env (2)"]);
    assert_eq!(app.active_env, Some(1));
    assert!(message(&app).contains(".env (2)"), "{}", message(&app));
}

// ── Auto-rename replaces an existing " (N)" suffix ──────────────────────────

#[tokio::test]
async fn reimporting_a_renamed_collection_picks_the_next_number() {
    let (mut app, tmp) = app_with_collection();
    let path = tmp.path().join("api.json");
    execute_command(&mut app, &format!("export {}", path.display()))
        .await
        .unwrap();
    execute_command(&mut app, &format!("import {}", path.display()))
        .await
        .unwrap();
    // Export "API (2)" by name and import it again.
    let path2 = tmp.path().join("api2.json");
    execute_command(&mut app, &format!("export {} API (2)", path2.display()))
        .await
        .unwrap();
    execute_command(&mut app, &format!("import {}", path2.display()))
        .await
        .unwrap();
    let names: Vec<&str> = app.collections.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["API", "API (2)", "API (3)"]);
}

// ── Request names are unique within a collection ────────────────────────────

#[tokio::test]
async fn sidebar_rename_rejects_a_duplicate_request_name() {
    let (mut app, _tmp) = app_with_collection();
    app.sidebar_state.selected = 1; // "Users"
    press(&mut app, key(KeyCode::Char('r'))).await;
    app.rename_input.clear();
    type_str(&mut app, "orders").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert!(
        message(&app).contains("already exists"),
        "{}",
        message(&app)
    );
    assert!(matches!(
        app.mode,
        AppMode::Modal(ModalKind::RenameRequest { .. })
    ));
    let names: Vec<String> = app.collections[0]
        .all_requests()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(names, ["Users", "Orders"]);

    // Keeping its own name (another case) is fine.
    app.rename_input.clear();
    type_str(&mut app, "USERS").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.collections[0].all_requests()[0].name, "USERS");
}

#[tokio::test]
async fn sidebar_a_numbers_new_requests() {
    let (mut app, _tmp) = app_with_collection();
    for _ in 0..3 {
        app.focus = FocusArea::Sidebar;
        app.sidebar_state.selected = 0;
        press(&mut app, key(KeyCode::Char('a'))).await;
    }
    let names: Vec<String> = app.collections[0]
        .all_requests()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(
        names,
        [
            "Users",
            "Orders",
            "New Request",
            "New Request 2",
            "New Request 3"
        ]
    );
    assert_eq!(app.active_tab().request.name, "New Request 3");
}

#[tokio::test]
async fn rename_command_rejects_a_duplicate_in_the_tabs_collection() {
    let (mut app, _tmp) = app_with_collection();
    app.sidebar_state.selected = 1;
    press(&mut app, key(KeyCode::Enter)).await; // open "Users"
    execute_command(&mut app, "rename Orders").await.unwrap();
    assert!(
        message(&app).contains("already exists"),
        "{}",
        message(&app)
    );
    assert_eq!(app.active_tab().request.name, "Users");

    // F2 applies the same rule.
    press(&mut app, key(KeyCode::F(2))).await;
    app.rename_input.clear();
    type_str(&mut app, "ORDERS").await;
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.active_tab().request.name, "Users");
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::RenameTab)));

    // A tab outside any collection can take any name.
    app.mode = AppMode::Normal;
    app.new_tab();
    execute_command(&mut app, "rename Orders").await.unwrap();
    assert_eq!(app.active_tab().request.name, "Orders");
}

// ── :export picks an explicit collection ────────────────────────────────────

#[tokio::test]
async fn export_without_a_selected_collection_warns() {
    let (mut app, tmp) = app_with_collection();
    app.focus = FocusArea::UrlBar; // tab has no collection, sidebar not focused
    let path = tmp.path().join("out.json");
    execute_command(&mut app, &format!("export {}", path.display()))
        .await
        .unwrap();
    assert!(!path.exists());
    assert!(
        message(&app).contains("No collection selected"),
        "{}",
        message(&app)
    );
}

#[tokio::test]
async fn export_uses_the_sidebar_selection_or_a_named_collection() {
    let (mut app, tmp) = app_with_collection();
    let other = Collection::new("Other");
    app.collections.push(other);

    // Sidebar focused on "Other" (row 3: API, Users, Orders, Other).
    app.focus = FocusArea::Sidebar;
    app.sidebar_state.selected = 3;
    let path = tmp.path().join("out.md");
    execute_command(&mut app, &format!("export {}", path.display()))
        .await
        .unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("# Other"));

    // An explicit name wins (case-insensitive).
    let path = tmp.path().join("named.json");
    execute_command(&mut app, &format!("export {} api", path.display()))
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(json["info"]["name"], "API");

    execute_command(&mut app, &format!("export {} Nope", path.display()))
        .await
        .unwrap();
    assert!(
        message(&app).contains("'Nope' not found"),
        "{}",
        message(&app)
    );
}

#[tokio::test]
async fn export_path_with_spaces_still_works() {
    let (mut app, tmp) = app_with_collection();
    let dir = tmp.path().join("my docs");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("api.json");
    execute_command(&mut app, &format!("export {}", path.display()))
        .await
        .unwrap();
    assert!(path.exists(), "{}", message(&app));
}

// ── J / K page the sidebar ──────────────────────────────────────────────────

#[tokio::test]
async fn shift_j_and_ctrl_u_page_the_sidebar_selection() {
    let (mut app, _tmp) = test_app();
    let mut coll = Collection::new("Big");
    for i in 0..40 {
        coll.add_request(Request::new(format!("R{i}"), HttpMethod::GET, "/"));
    }
    app.sidebar_state.expanded.insert(coll.id);
    app.collections.push(coll);
    app.focus = FocusArea::Sidebar;
    app.nav_mode = NavMode::Panel;
    draw(&mut app, 120, 30);
    let visible = usize::from(app.regions.sidebar.unwrap().height) - 3;
    let page = visible / 2;

    press(&mut app, key_mod(KeyCode::Char('J'), KeyModifiers::SHIFT)).await;
    assert_eq!(app.sidebar_state.selected, page);
    press(&mut app, key_mod(KeyCode::Char('d'), KeyModifiers::CONTROL)).await;
    assert_eq!(app.sidebar_state.selected, 2 * page);
    press(&mut app, key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL)).await;
    assert_eq!(app.sidebar_state.selected, page);
    press(&mut app, key_mod(KeyCode::Char('K'), KeyModifiers::SHIFT)).await;
    press(&mut app, key_mod(KeyCode::Char('K'), KeyModifiers::SHIFT)).await;
    assert_eq!(app.sidebar_state.selected, 0);
    for _ in 0..10 {
        press(&mut app, key_mod(KeyCode::Char('J'), KeyModifiers::SHIFT)).await;
    }
    assert_eq!(app.sidebar_state.selected, 40, "clamped to the last row");
}

// ── :import without a path opens the dialog ─────────────────────────────────

#[tokio::test]
async fn import_without_a_path_opens_the_import_modal() {
    let (mut app, _tmp) = test_app();
    for cmd in ["import", "import ", "import   \t"] {
        app.mode = AppMode::Normal;
        execute_command(&mut app, cmd).await.unwrap();
        assert_eq!(app.mode, AppMode::Modal(ModalKind::Import), "{cmd:?}");
        assert!(!message(&app).contains("Cannot read file"));
    }
}
