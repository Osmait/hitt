use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use hitt::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind, RequestTabKind, ResponseTabKind,
    SidebarSection,
};
use hitt::core::chain::RequestChain;
use hitt::core::collection::Collection;
use hitt::core::environment::Environment;
use hitt::core::request::{HttpMethod, Request};
use hitt::event::{build_sidebar_items, execute_command, handle_event, AppEvent, SidebarItem};
use hitt::storage::config::AppConfig;

/// Create an App with a temp dir for collections so tests don't write to real config.
fn test_app() -> (App, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    let app = App::new(config).unwrap();
    (app, tmp)
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, modifiers))
}

fn key_char(c: char) -> AppEvent {
    key(KeyCode::Char(c))
}

// ════════════════════════════════════════════════════════════════════════════
// App construction & basic state
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn app_new_defaults() {
    let (app, _tmp) = test_app();
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.focus, FocusArea::Sidebar);
    assert_eq!(app.tabs.len(), 1);
    assert!(!app.should_quit);
    assert!(app.collections.is_empty());
    assert!(app.environments.is_empty());
    assert!(app.notification.is_none());
}

// ════════════════════════════════════════════════════════════════════════════
// Tab management
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn key_t_creates_new_tab() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.tabs.len(), 1);
    handle_event(&mut app, key_char('t')).await.unwrap();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
}

#[tokio::test]
async fn key_w_closes_tab() {
    let (mut app, _tmp) = test_app();
    // Create 2 tabs
    handle_event(&mut app, key_char('t')).await.unwrap();
    assert_eq!(app.tabs.len(), 2);
    // Close one
    handle_event(&mut app, key_char('w')).await.unwrap();
    assert_eq!(app.tabs.len(), 1);
}

#[tokio::test]
async fn key_w_does_not_close_last_tab() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.tabs.len(), 1);
    handle_event(&mut app, key_char('w')).await.unwrap();
    assert_eq!(app.tabs.len(), 1); // Still 1
}

#[tokio::test]
async fn key_n_b_switch_tabs() {
    let (mut app, _tmp) = test_app();
    // Create 3 tabs
    handle_event(&mut app, key_char('t')).await.unwrap();
    handle_event(&mut app, key_char('t')).await.unwrap();
    assert_eq!(app.tabs.len(), 3);
    assert_eq!(app.active_tab, 2);

    // n wraps to first
    handle_event(&mut app, key_char('n')).await.unwrap();
    assert_eq!(app.active_tab, 0);

    // b wraps to last
    handle_event(&mut app, key_char('b')).await.unwrap();
    assert_eq!(app.active_tab, 2);

    // b again
    handle_event(&mut app, key_char('b')).await.unwrap();
    assert_eq!(app.active_tab, 1);
}

// ════════════════════════════════════════════════════════════════════════════
// Focus cycling
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tab_key_cycles_focus_global_mode() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.focus, FocusArea::Sidebar);
    assert_eq!(app.nav_mode, NavMode::Global);

    // Global mode cycles 4 major panels
    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::RequestBody);

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::ResponseBody);

    // Wraps back to sidebar
    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::Sidebar);
}

#[tokio::test]
async fn tab_key_cycles_focus_panel_mode() {
    let (mut app, _tmp) = test_app();
    app.nav_mode = NavMode::Panel;
    assert_eq!(app.focus, FocusArea::Sidebar);

    // Panel mode cycles all 6 areas
    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::RequestTabs);

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::RequestBody);

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::ResponseBody);

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::ResponseTabs);

    // Wraps back to sidebar
    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::Sidebar);
}

#[tokio::test]
async fn shift_tab_cycles_backward() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.focus, FocusArea::Sidebar);

    // Global mode backward: Sidebar → ResponseBody
    handle_event(&mut app, key_mod(KeyCode::BackTab, KeyModifiers::SHIFT))
        .await
        .unwrap();
    assert_eq!(app.focus, FocusArea::ResponseBody);
}

// ════════════════════════════════════════════════════════════════════════════
// Mode transitions
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn key_i_enters_insert_mode() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.mode, AppMode::Insert);
}

#[tokio::test]
async fn esc_returns_to_normal_from_insert() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Insert;
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn colon_enters_command_mode() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char(':')).await.unwrap();
    assert_eq!(app.mode, AppMode::Command);
    assert!(app.command_input.is_empty());
}

#[tokio::test]
async fn question_mark_opens_help() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('?')).await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Help));
}

#[tokio::test]
async fn key_q_quits() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('q')).await.unwrap();
    assert!(app.should_quit);
}

#[tokio::test]
async fn ctrl_c_quits() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_mod(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert!(app.should_quit);
}

#[tokio::test]
async fn slash_opens_search() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('/')).await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Search));
}

#[tokio::test]
async fn key_d_opens_diff_selector() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('d')).await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::DiffSelector));
}

// ════════════════════════════════════════════════════════════════════════════
// Insert mode (URL bar editing)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn insert_mode_url_typing() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Insert;
    app.focus = FocusArea::UrlBar;
    app.active_tab_mut().request.url = String::new();

    // Type "https"
    for c in "https".chars() {
        handle_event(&mut app, key_char(c)).await.unwrap();
    }
    assert_eq!(app.active_tab().request.url, "https");
    assert!(app.active_tab().dirty);
}

#[tokio::test]
async fn insert_mode_backspace() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Insert;
    app.focus = FocusArea::UrlBar;
    app.active_tab_mut().request.url = "abc".to_string();

    handle_event(&mut app, key(KeyCode::Backspace))
        .await
        .unwrap();
    assert_eq!(app.active_tab().request.url, "ab");
}

#[tokio::test]
async fn insert_mode_tab_returns_to_normal_and_cycles() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Insert;
    app.focus = FocusArea::UrlBar;

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    // Focus moved forward from UrlBar
    assert_eq!(app.focus, FocusArea::RequestTabs);
}

// ════════════════════════════════════════════════════════════════════════════
// Command mode input
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn command_mode_typing_and_esc() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Command;

    // Type "hel"
    handle_event(&mut app, key_char('h')).await.unwrap();
    handle_event(&mut app, key_char('e')).await.unwrap();
    handle_event(&mut app, key_char('l')).await.unwrap();
    assert_eq!(app.command_input, "hel");

    // Esc cancels
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.command_input.is_empty());
}

#[tokio::test]
async fn command_mode_backspace_exits_when_empty() {
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Command;
    app.command_input = "x".to_string();

    handle_event(&mut app, key(KeyCode::Backspace))
        .await
        .unwrap();
    assert!(app.command_input.is_empty());
    assert_eq!(app.mode, AppMode::Normal);
}

// ════════════════════════════════════════════════════════════════════════════
// Execute commands (direct)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_newcol_creates_collection() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol My API").await.unwrap();
    assert_eq!(app.collections.len(), 1);
    assert_eq!(app.collections[0].name, "My API");
    assert!(app.notification.is_some());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_newcol_no_name_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol").await.unwrap();
    assert!(app.collections.is_empty());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

#[tokio::test]
async fn cmd_newcol_empty_name_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol  ").await.unwrap();
    assert!(app.collections.is_empty());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

#[tokio::test]
async fn cmd_quit() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "q").await.unwrap();
    assert!(app.should_quit);
}

#[tokio::test]
async fn cmd_quit_long() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "quit").await.unwrap();
    assert!(app.should_quit);
}

#[tokio::test]
async fn cmd_help_opens_modal() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "help").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Help));
}

#[tokio::test]
async fn cmd_env_switches_environment() {
    let (mut app, _tmp) = test_app();
    let mut env = Environment::new("Production");
    env.add_variable("host", "prod.example.com");
    app.environments.push(env);

    execute_command(&mut app, "env Production").await.unwrap();
    assert_eq!(app.active_env, Some(0));
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Info
    );
}

#[tokio::test]
async fn cmd_env_not_found() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "env NonExistent").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

#[tokio::test]
async fn cmd_save_no_collection_warns() {
    let (mut app, _tmp) = test_app();
    // No collections at all
    execute_command(&mut app, "save").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

#[tokio::test]
async fn cmd_save_with_collection_index() {
    let (mut app, _tmp) = test_app();
    // Create a collection and assign tab to it
    let mut coll = Collection::new("Test");
    let req = Request::new("R1", HttpMethod::GET, "https://example.com");
    let req_id = req.id;
    coll.add_request(req);
    app.collections.push(coll);
    app.active_tab_mut().collection_index = Some(0);
    app.active_tab_mut().request.id = req_id;

    execute_command(&mut app, "save").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
    assert!(!app.active_tab().dirty);
}

#[tokio::test]
async fn cmd_save_opens_collection_picker_when_multiple() {
    let (mut app, _tmp) = test_app();
    app.collections.push(Collection::new("API One"));
    app.collections.push(Collection::new("API Two"));
    // Tab not assigned to any collection
    app.active_tab_mut().collection_index = None;

    execute_command(&mut app, "save").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::CollectionPicker));
}

#[tokio::test]
async fn cmd_unknown_command_errors() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "foobar").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("Unknown command"));
}

#[tokio::test]
async fn cmd_import_no_args_opens_modal() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "import").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Import));
}

#[tokio::test]
async fn cmd_export_no_args_opens_modal() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "export").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Export));
}

// ════════════════════════════════════════════════════════════════════════════
// Execute command via full key sequence (command mode flow)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn full_command_flow_newcol() {
    let (mut app, _tmp) = test_app();

    // Enter command mode
    handle_event(&mut app, key_char(':')).await.unwrap();
    assert_eq!(app.mode, AppMode::Command);

    // Type "newcol Test"
    for c in "newcol Test".chars() {
        handle_event(&mut app, key_char(c)).await.unwrap();
    }
    assert_eq!(app.command_input, "newcol Test");

    // Press Enter
    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.collections.len(), 1);
    assert_eq!(app.collections[0].name, "Test");
}

// ════════════════════════════════════════════════════════════════════════════
// Sidebar
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn build_sidebar_items_empty() {
    let (app, _tmp) = test_app();
    let items = build_sidebar_items(&app);
    assert!(items.is_empty());
}

#[test]
fn build_sidebar_items_with_collections() {
    let (mut app, _tmp) = test_app();
    let mut coll = Collection::new("API");
    coll.add_request(Request::new("R1", HttpMethod::GET, "/r1"));
    app.collections.push(coll);

    // Collection is collapsed by default → only collection header
    let items = build_sidebar_items(&app);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0], SidebarItem::Collection { .. }));

    // Expand collection
    let coll_id = app.collections[0].id;
    app.sidebar_state.expanded.insert(coll_id);

    let items = build_sidebar_items(&app);
    assert_eq!(items.len(), 2); // collection + request
    assert!(matches!(items[1], SidebarItem::Request { .. }));
}

// ════════════════════════════════════════════════════════════════════════════
// Navigation (vim-style j/k in normal mode)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn nav_j_k_in_sidebar() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::Sidebar;
    app.nav_mode = NavMode::Panel;

    // Add collection with requests, expand it
    let mut coll = Collection::new("API");
    coll.add_request(Request::new("R1", HttpMethod::GET, "/r1"));
    coll.add_request(Request::new("R2", HttpMethod::GET, "/r2"));
    let coll_id = coll.id;
    app.collections.push(coll);
    app.sidebar_state.expanded.insert(coll_id);

    assert_eq!(app.sidebar_state.selected, 0);

    // j moves down
    handle_event(&mut app, key_char('j')).await.unwrap();
    assert_eq!(app.sidebar_state.selected, 1);

    handle_event(&mut app, key_char('j')).await.unwrap();
    assert_eq!(app.sidebar_state.selected, 2);

    // k moves up
    handle_event(&mut app, key_char('k')).await.unwrap();
    assert_eq!(app.sidebar_state.selected, 1);
}

#[tokio::test]
async fn nav_j_k_scroll_response_body() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;
    app.response_scroll = 5;

    handle_event(&mut app, key_char('j')).await.unwrap();
    assert_eq!(app.response_scroll, 6);

    handle_event(&mut app, key_char('k')).await.unwrap();
    assert_eq!(app.response_scroll, 5);
}

// ════════════════════════════════════════════════════════════════════════════
// Request/Response sub-tab switching (number keys)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn number_keys_switch_request_tabs() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::RequestTabs;

    handle_event(&mut app, key_char('2')).await.unwrap();
    assert_eq!(app.active_tab().request_tab, RequestTabKind::Auth);

    handle_event(&mut app, key_char('3')).await.unwrap();
    assert_eq!(app.active_tab().request_tab, RequestTabKind::Headers);

    handle_event(&mut app, key_char('4')).await.unwrap();
    assert_eq!(app.active_tab().request_tab, RequestTabKind::Body);

    handle_event(&mut app, key_char('5')).await.unwrap();
    assert_eq!(app.active_tab().request_tab, RequestTabKind::Assertions);

    handle_event(&mut app, key_char('1')).await.unwrap();
    assert_eq!(app.active_tab().request_tab, RequestTabKind::Params);
}

#[tokio::test]
async fn number_keys_switch_response_tabs() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::ResponseTabs;

    handle_event(&mut app, key_char('2')).await.unwrap();
    assert_eq!(app.active_tab().response_tab, ResponseTabKind::Headers);

    handle_event(&mut app, key_char('3')).await.unwrap();
    assert_eq!(app.active_tab().response_tab, ResponseTabKind::Cookies);

    handle_event(&mut app, key_char('1')).await.unwrap();
    assert_eq!(app.active_tab().response_tab, ResponseTabKind::Body);
}

// ════════════════════════════════════════════════════════════════════════════
// Method cycling
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn key_m_cycles_method_on_url_bar() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::UrlBar;
    assert_eq!(app.active_tab().request.method, HttpMethod::GET);

    handle_event(&mut app, key_char('m')).await.unwrap();
    assert_eq!(app.active_tab().request.method, HttpMethod::POST);

    handle_event(&mut app, key_char('m')).await.unwrap();
    assert_eq!(app.active_tab().request.method, HttpMethod::PUT);
}

// ════════════════════════════════════════════════════════════════════════════
// Environment cycling
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn key_e_cycles_environments() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.environments.push(Environment::new("Prod"));

    handle_event(&mut app, key_char('e')).await.unwrap();
    assert_eq!(app.active_env, Some(0));

    handle_event(&mut app, key_char('e')).await.unwrap();
    assert_eq!(app.active_env, Some(1));

    handle_event(&mut app, key_char('e')).await.unwrap();
    assert_eq!(app.active_env, Some(0)); // wraps
}

#[tokio::test]
async fn key_e_no_environments_warns() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('e')).await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Scroll
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn page_scroll_shift_j_k() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::ResponseBody;
    app.response_scroll = 0;

    // Shift+J = half page down
    handle_event(&mut app, key_mod(KeyCode::Char('J'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    assert_eq!(app.response_scroll, 15);

    // Shift+K = half page up
    handle_event(&mut app, key_mod(KeyCode::Char('K'), KeyModifiers::SHIFT))
        .await
        .unwrap();
    assert_eq!(app.response_scroll, 0);
}

#[tokio::test]
async fn g_scrolls_to_top() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::ResponseBody;
    app.response_scroll = 100;

    handle_event(&mut app, key_char('g')).await.unwrap();
    assert_eq!(app.response_scroll, 0);
}

// ════════════════════════════════════════════════════════════════════════════
// Notification
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn notify_and_clear() {
    let (mut app, _tmp) = test_app();
    app.notify("Hello".to_string(), NotificationKind::Info);
    assert!(app.notification.is_some());
    assert_eq!(app.notification.as_ref().unwrap().message, "Hello");

    // Not expired yet (< 3 seconds)
    app.clear_expired_notification();
    assert!(app.notification.is_some());
}

// ════════════════════════════════════════════════════════════════════════════
// Tick event
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tick_event_clears_notification() {
    let (mut app, _tmp) = test_app();
    // Manually create an expired notification
    app.notification = Some(hitt::app::Notification {
        message: "Old".into(),
        kind: NotificationKind::Info,
        created_at: std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(10))
            .unwrap(),
    });

    handle_event(&mut app, AppEvent::Tick).await.unwrap();
    assert!(app.notification.is_none());
}

// ════════════════════════════════════════════════════════════════════════════
// :delreq command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_delreq_deletes_selected_request() {
    let (mut app, _tmp) = test_app();

    // Create collection with requests
    let mut coll = Collection::new("API");
    coll.add_request(Request::new("R1", HttpMethod::GET, "/r1"));
    coll.add_request(Request::new("R2", HttpMethod::POST, "/r2"));
    let coll_id = coll.id;
    app.collections.push(coll);

    // Expand and select a request
    app.sidebar_state.expanded.insert(coll_id);
    app.sidebar_state.section = SidebarSection::Collections;
    app.sidebar_state.selected = 1; // First request

    assert_eq!(app.collections[0].request_count(), 2);

    execute_command(&mut app, "delreq").await.unwrap();
    assert_eq!(app.collections[0].request_count(), 1);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_delreq_on_collection_warns() {
    let (mut app, _tmp) = test_app();
    app.collections.push(Collection::new("API"));
    app.sidebar_state.section = SidebarSection::Collections;
    app.sidebar_state.selected = 0; // Collection header, not a request

    execute_command(&mut app, "delreq").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :import command with file
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_import_postman_file() {
    let (mut app, _tmp) = test_app();
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/sample_postman_collection.json"
    );
    execute_command(&mut app, &format!("import {fixture}"))
        .await
        .unwrap();
    assert_eq!(app.collections.len(), 1);
    assert_eq!(app.collections[0].name, "Sample API");
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_import_har_file() {
    let (mut app, _tmp) = test_app();
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.har");
    execute_command(&mut app, &format!("import {fixture}"))
        .await
        .unwrap();
    assert_eq!(app.collections.len(), 1);
    assert_eq!(app.collections[0].request_count(), 2);
}

#[tokio::test]
async fn cmd_import_openapi_file() {
    let (mut app, _tmp) = test_app();
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/petstore_openapi.yaml"
    );
    execute_command(&mut app, &format!("import {fixture}"))
        .await
        .unwrap();
    assert_eq!(app.collections.len(), 1);
    assert_eq!(app.collections[0].name, "Petstore API");
}

#[tokio::test]
async fn cmd_import_nonexistent_file() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "import /tmp/nonexistent_file_xyz.json")
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :export command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_export_postman_json() {
    let (mut app, tmp) = test_app();
    let mut coll = Collection::new("Export Test");
    coll.add_request(Request::new("R1", HttpMethod::GET, "https://example.com"));
    app.collections.push(coll);

    let export_path = tmp.path().join("exported.json");
    execute_command(&mut app, &format!("export {}", export_path.display()))
        .await
        .unwrap();
    assert!(export_path.exists());

    // Verify it's valid Postman JSON
    let content = std::fs::read_to_string(&export_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["info"]["name"], "Export Test");
}

#[tokio::test]
async fn cmd_export_markdown() {
    let (mut app, tmp) = test_app();
    let mut coll = Collection::new("Docs Test");
    coll.add_request(Request::new("Get Users", HttpMethod::GET, "/users"));
    app.collections.push(coll);

    let export_path = tmp.path().join("api.md");
    execute_command(&mut app, &format!("export {}", export_path.display()))
        .await
        .unwrap();
    assert!(export_path.exists());

    let content = std::fs::read_to_string(&export_path).unwrap();
    assert!(content.contains("# Docs Test"));
    assert!(content.contains("Get Users"));
}

#[tokio::test]
async fn cmd_export_curl() {
    let (mut app, tmp) = test_app();
    app.active_tab_mut().request =
        Request::new("Test", HttpMethod::POST, "https://api.example.com").with_body(
            hitt::core::request::RequestBody::Json(r#"{"key":"val"}"#.into()),
        );

    let export_path = tmp.path().join("request.sh");
    execute_command(&mut app, &format!("export {}", export_path.display()))
        .await
        .unwrap();
    assert!(export_path.exists());

    let content = std::fs::read_to_string(&export_path).unwrap();
    assert!(content.starts_with("curl"));
    assert!(content.contains("-X POST"));
}

// ════════════════════════════════════════════════════════════════════════════
// F2 rename tab
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn f2_opens_rename_modal() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.name = "Old Name".to_string();

    handle_event(&mut app, key(KeyCode::F(2))).await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::RenameTab));
    assert_eq!(app.rename_input, "Old Name");
}

// ════════════════════════════════════════════════════════════════════════════
// Save key (s) in normal mode
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn key_s_without_collection_warns() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('s')).await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

#[tokio::test]
async fn key_s_with_collection_saves() {
    let (mut app, _tmp) = test_app();
    let mut coll = Collection::new("Test");
    let req = Request::new("R1", HttpMethod::GET, "/r1");
    let req_id = req.id;
    coll.add_request(req);
    app.collections.push(coll);
    app.active_tab_mut().collection_index = Some(0);
    app.active_tab_mut().request.id = req_id;
    app.active_tab_mut().dirty = true;

    handle_event(&mut app, key_char('s')).await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
    assert!(!app.active_tab().dirty);
}

// ════════════════════════════════════════════════════════════════════════════
// Modal escape
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn esc_closes_any_modal() {
    let (mut app, _tmp) = test_app();

    let modals = vec![
        AppMode::Modal(ModalKind::Search),
        AppMode::Modal(ModalKind::Help),
        AppMode::Modal(ModalKind::Import),
        AppMode::Modal(ModalKind::Export),
        AppMode::Modal(ModalKind::DiffSelector),
    ];

    for modal in modals {
        app.mode = modal.clone();
        handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
        assert_eq!(app.mode, AppMode::Normal, "Esc should close {modal:?}");
    }
}

// ════════════════════════════════════════════════════════════════════════════
// App builder/resolver integration
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn build_resolver_with_collection_vars() {
    let (mut app, _tmp) = test_app();
    let mut coll = Collection::new("Test");
    coll.variables.push(hitt::core::request::KeyValuePair::new(
        "base_url",
        "https://api.example.com",
    ));
    app.collections.push(coll);
    app.active_tab_mut().collection_index = Some(0);

    let resolver = app.build_resolver();
    assert_eq!(
        resolver.resolve("{{base_url}}/users"),
        "https://api.example.com/users"
    );
}

#[test]
fn build_resolver_with_environment() {
    let (mut app, _tmp) = test_app();
    let mut env = Environment::new("Test");
    env.add_variable("token", "secret123");
    app.environments.push(env);
    app.active_env = Some(0);

    let resolver = app.build_resolver();
    assert_eq!(resolver.resolve("Bearer {{token}}"), "Bearer secret123");
}

// ════════════════════════════════════════════════════════════════════════════
// Ctrl+ keybindings
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ctrl_r_sends_request() {
    let (mut app, _tmp) = test_app();
    // With no URL, send_request will set loading then unset it
    assert!(!app.loading);
    handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    // After completion, loading should be false, but either a response or error notification exists
    assert!(!app.loading);
}

#[tokio::test]
async fn ctrl_s_saves() {
    let (mut app, _tmp) = test_app();
    let mut coll = Collection::new("Test");
    let req = Request::new("R1", HttpMethod::GET, "/r1");
    let req_id = req.id;
    coll.add_request(req);
    app.collections.push(coll);
    app.active_tab_mut().collection_index = Some(0);
    app.active_tab_mut().request.id = req_id;
    app.active_tab_mut().dirty = true;

    handle_event(&mut app, key_mod(KeyCode::Char('s'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
    assert!(!app.active_tab().dirty);
}

#[tokio::test]
async fn ctrl_p_opens_search() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_mod(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Search));
}

#[tokio::test]
async fn ctrl_e_cycles_environment() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.environments.push(Environment::new("Prod"));

    handle_event(&mut app, key_mod(KeyCode::Char('e'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.active_env, Some(0));

    handle_event(&mut app, key_mod(KeyCode::Char('e'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.active_env, Some(1));
}

#[tokio::test]
async fn ctrl_n_new_tab() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.tabs.len(), 1);
    handle_event(&mut app, key_mod(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
}

#[tokio::test]
async fn ctrl_i_opens_import() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_mod(KeyCode::Char('i'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Import));
}

#[tokio::test]
async fn ctrl_x_opens_export() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_mod(KeyCode::Char('x'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Export));
}

// ════════════════════════════════════════════════════════════════════════════
// :set command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_set_timeout() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set timeout 5000").await.unwrap();
    assert_eq!(app.config.timeout_ms, 5000);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_set_follow_redirects() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set follow_redirects false")
        .await
        .unwrap();
    assert!(!app.config.follow_redirects);
}

#[tokio::test]
async fn cmd_set_verify_ssl() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set verify_ssl false")
        .await
        .unwrap();
    assert!(!app.config.verify_ssl);
}

#[tokio::test]
async fn cmd_set_vim_mode() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set vim_mode false")
        .await
        .unwrap();
    assert!(!app.config.vim_mode);
}

#[tokio::test]
async fn cmd_set_history_limit() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set history_limit 500")
        .await
        .unwrap();
    assert_eq!(app.config.history_limit, 500);
}

#[tokio::test]
async fn cmd_set_unknown_key() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set nonexistent true")
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("Unknown setting"));
}

#[tokio::test]
async fn cmd_set_invalid_value() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set timeout not_a_number")
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

#[tokio::test]
async fn cmd_set_no_args() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :env-file command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_env_file_loads() {
    let (mut app, tmp) = test_app();
    let env_path = tmp.path().join("test.env");
    std::fs::write(&env_path, "API_KEY=secret123\nBASE_URL=https://example.com").unwrap();

    execute_command(&mut app, &format!("env-file {}", env_path.display()))
        .await
        .unwrap();
    assert_eq!(app.environments.len(), 1);
    assert_eq!(app.active_env, Some(0));
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_env_file_not_found() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "env-file /tmp/nonexistent_env_file.env")
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :diff command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_diff_opens_modal() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "diff").await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::DiffSelector));
}

// ════════════════════════════════════════════════════════════════════════════
// :chain / :proxy stubs
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_chain_not_found_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "chain test-chain").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("not found"));
}

#[tokio::test]
async fn cmd_proxy_stub() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "proxy 8080").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("not yet available"));
}

// ════════════════════════════════════════════════════════════════════════════
// :newenv command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_newenv_creates() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newenv Staging").await.unwrap();
    assert_eq!(app.environments.len(), 1);
    assert_eq!(app.environments[0].name, "Staging");
    assert_eq!(app.active_env, Some(0));
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_newenv_no_name() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newenv").await.unwrap();
    assert!(app.environments.is_empty());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :dupenv command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_dupenv_clones() {
    let (mut app, _tmp) = test_app();
    let mut env = Environment::new("Production");
    env.add_variable("host", "prod.example.com");
    app.environments.push(env);
    app.active_env = Some(0);

    execute_command(&mut app, "dupenv").await.unwrap();
    assert_eq!(app.environments.len(), 2);
    assert_eq!(app.environments[1].name, "Production (copy)");
    assert_eq!(app.active_env, Some(1));
    // Variables should be cloned
    assert_eq!(app.environments[1].values.len(), 1);
    // UUID should be different
    assert_ne!(app.environments[0].id, app.environments[1].id);
}

#[tokio::test]
async fn cmd_dupenv_custom_name() {
    let (mut app, _tmp) = test_app();
    let mut env = Environment::new("Production");
    env.add_variable("host", "prod.example.com");
    app.environments.push(env);
    app.active_env = Some(0);

    execute_command(&mut app, "dupenv Staging").await.unwrap();
    assert_eq!(app.environments.len(), 2);
    assert_eq!(app.environments[1].name, "Staging");
}

#[tokio::test]
async fn cmd_dupenv_no_active() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "dupenv").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :delcol command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_delcol_by_name() {
    let (mut app, _tmp) = test_app();
    app.collections.push(Collection::new("API One"));
    app.collections.push(Collection::new("API Two"));

    execute_command(&mut app, "delcol API One").await.unwrap();
    assert_eq!(app.collections.len(), 1);
    assert_eq!(app.collections[0].name, "API Two");
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_delcol_not_found() {
    let (mut app, _tmp) = test_app();
    app.collections.push(Collection::new("API"));
    execute_command(&mut app, "delcol NonExistent")
        .await
        .unwrap();
    assert_eq!(app.collections.len(), 1);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

#[tokio::test]
async fn cmd_delcol_no_args_no_selection() {
    let (mut app, _tmp) = test_app();
    // No collections, no sidebar selection
    execute_command(&mut app, "delcol").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :addvar command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_addvar_adds() {
    let (mut app, _tmp) = test_app();
    app.collections.push(Collection::new("API"));
    app.active_tab_mut().collection_index = Some(0);

    execute_command(&mut app, "addvar base_url https://api.example.com")
        .await
        .unwrap();
    assert_eq!(app.collections[0].variables.len(), 1);
    assert_eq!(app.collections[0].variables[0].key, "base_url");
    assert_eq!(
        app.collections[0].variables[0].value,
        "https://api.example.com"
    );
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_addvar_no_collection() {
    let (mut app, _tmp) = test_app();
    // No collection assigned to tab
    execute_command(&mut app, "addvar key value").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

#[tokio::test]
async fn cmd_addvar_no_args() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "addvar").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :clearhistory command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_clearhistory() {
    let (mut app, _tmp) = test_app();
    // Add a couple of history entries
    app.history.add(hitt::core::history::HistoryEntry::new(
        HttpMethod::GET,
        "https://example.com",
    ));
    app.history.add(hitt::core::history::HistoryEntry::new(
        HttpMethod::POST,
        "https://example.com/api",
    ));
    assert_eq!(app.history.len(), 2);

    execute_command(&mut app, "clearhistory").await.unwrap();
    assert!(app.history.is_empty());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

// ════════════════════════════════════════════════════════════════════════════
// :rename command
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_rename_changes_name() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.name = "Old Name".to_string();

    execute_command(&mut app, "rename My New Request")
        .await
        .unwrap();
    assert_eq!(app.active_tab().request.name, "My New Request");
    assert!(app.active_tab().dirty);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_rename_no_args() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "rename").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

// ════════════════════════════════════════════════════════════════════════════
// WebSocket / SSE commands and modes
// ════════════════════════════════════════════════════════════════════════════

use hitt::event::{handle_sse_protocol_event, handle_ws_protocol_event, SseEventData, WsEventData};
use hitt::protocols::sse::{SseEvent as ProtocolSseEvent, SseSession};
use hitt::protocols::websocket::{
    MessageDirection, WebSocketSession, WsContent, WsMessage, WsStatus,
};

#[tokio::test]
async fn cmd_ws_no_args_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "ws").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
    assert!(app.notification.as_ref().unwrap().message.contains("Usage"));
}

#[tokio::test]
async fn cmd_sse_no_args_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "sse").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
    assert!(app.notification.as_ref().unwrap().message.contains("Usage"));
}

#[tokio::test]
async fn ws_message_input_typing() {
    use hitt::core::request::Protocol;
    let (mut app, _tmp) = test_app();
    // Set up WS protocol on the active tab and enter Insert mode on ResponseBody
    app.tabs[app.active_tab].request.protocol = Protocol::WebSocket;
    app.tabs[app.active_tab].response_tab = ResponseTabKind::WsMessages;
    app.mode = AppMode::Insert;
    app.focus = FocusArea::ResponseBody;

    handle_event(&mut app, key_char('h')).await.unwrap();
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.tabs[app.active_tab].ws_message_input, "hi");

    // Backspace
    handle_event(&mut app, key(KeyCode::Backspace))
        .await
        .unwrap();
    assert_eq!(app.tabs[app.active_tab].ws_message_input, "h");
}

#[tokio::test]
async fn sse_toggle_accumulated() {
    use hitt::core::request::Protocol;
    let (mut app, _tmp) = test_app();
    app.tabs[app.active_tab].request.protocol = Protocol::Sse;
    app.tabs[app.active_tab].response_tab = ResponseTabKind::SseEvents;
    app.mode = AppMode::Normal;
    app.focus = FocusArea::ResponseBody;
    assert!(!app.tabs[app.active_tab].sse_show_accumulated);

    handle_event(&mut app, key_char('a')).await.unwrap();
    assert!(app.tabs[app.active_tab].sse_show_accumulated);

    handle_event(&mut app, key_char('a')).await.unwrap();
    assert!(!app.tabs[app.active_tab].sse_show_accumulated);
}

#[test]
fn handle_ws_protocol_event_connected() {
    let (mut app, _tmp) = test_app();
    let mut session = WebSocketSession::new("wss://example.com");
    session.status = WsStatus::Connecting;
    let session_id = session.id;
    app.tabs[app.active_tab].ws_session = Some(session);

    handle_ws_protocol_event(&mut app, session_id, WsEventData::Connected);

    match &app.tabs[app.active_tab].ws_session.as_ref().unwrap().status {
        WsStatus::Connected { .. } => {} // ok
        other => panic!("Expected Connected, got {other:?}"),
    }
}

#[test]
fn handle_ws_protocol_event_message() {
    let (mut app, _tmp) = test_app();
    let session = WebSocketSession::new("wss://example.com");
    let session_id = session.id;
    app.tabs[app.active_tab].ws_session = Some(session);

    let msg = WsMessage {
        direction: MessageDirection::Received,
        content: WsContent::Text("hello".into()),
        timestamp: chrono::Utc::now(),
    };

    handle_ws_protocol_event(&mut app, session_id, WsEventData::MessageReceived(msg));
    let ws = app.tabs[app.active_tab].ws_session.as_ref().unwrap();
    assert_eq!(ws.messages.len(), 1);
    assert_eq!(ws.messages[0].content.as_text().unwrap(), "hello");
}

#[test]
fn handle_sse_protocol_event_event() {
    let (mut app, _tmp) = test_app();
    let session = SseSession::new("https://example.com/events");
    let session_id = session.id;
    app.tabs[app.active_tab].sse_session = Some(session);

    let evt = ProtocolSseEvent {
        event_type: Some("message".into()),
        data: "test data".into(),
        id: Some("42".into()),
        timestamp: chrono::Utc::now(),
    };

    handle_sse_protocol_event(&mut app, session_id, SseEventData::Event(evt));
    let sse = app.tabs[app.active_tab].sse_session.as_ref().unwrap();
    assert_eq!(sse.events.len(), 1);
    assert_eq!(sse.events[0].data, "test data");
    assert!(sse.accumulated_text.contains("test data"));
    assert_eq!(sse.last_event_id, Some("42".into()));
}

#[tokio::test]
async fn method_cycle_includes_ws_sse() {
    use hitt::core::request::Protocol;
    let (mut app, _tmp) = test_app();
    app.mode = AppMode::Normal;
    app.focus = FocusArea::UrlBar;

    // Cycle through all HTTP methods to get to WS
    // Start at GET (index 0), cycle through POST(1), PUT(2), PATCH(3), DELETE(4),
    // HEAD(5), OPTIONS(6), TRACE(7) → one more press → WS
    let http_methods = HttpMethod::all();
    for _ in 0..http_methods.len() {
        handle_event(&mut app, key_char('m')).await.unwrap();
    }
    assert_eq!(
        app.tabs[app.active_tab].request.protocol,
        Protocol::WebSocket
    );

    // WS → SSE
    handle_event(&mut app, key_char('m')).await.unwrap();
    assert_eq!(app.tabs[app.active_tab].request.protocol, Protocol::Sse);

    // SSE → back to HTTP GET
    handle_event(&mut app, key_char('m')).await.unwrap();
    assert_eq!(app.tabs[app.active_tab].request.protocol, Protocol::Http);
    assert_eq!(app.tabs[app.active_tab].request.method, HttpMethod::GET);
}

#[tokio::test]
async fn response_tabs_change_with_protocol() {
    use hitt::core::request::Protocol;
    let (_app, _tmp) = test_app();

    // HTTP tabs
    let http_tabs = ResponseTabKind::for_protocol(&Protocol::Http);
    assert!(http_tabs.contains(&ResponseTabKind::Body));
    assert!(http_tabs.contains(&ResponseTabKind::Headers));

    // WS tabs
    let ws_tabs = ResponseTabKind::for_protocol(&Protocol::WebSocket);
    assert!(ws_tabs.contains(&ResponseTabKind::WsMessages));
    assert!(ws_tabs.contains(&ResponseTabKind::WsInfo));
    assert!(!ws_tabs.contains(&ResponseTabKind::Body));

    // SSE tabs
    let sse_tabs = ResponseTabKind::for_protocol(&Protocol::Sse);
    assert!(sse_tabs.contains(&ResponseTabKind::SseEvents));
    assert!(sse_tabs.contains(&ResponseTabKind::SseStream));
    assert!(sse_tabs.contains(&ResponseTabKind::SseInfo));
    assert!(!sse_tabs.contains(&ResponseTabKind::Body));
}

// ════════════════════════════════════════════════════════════════════════════
// NavMode (Global/Panel navigation)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn app_starts_in_global_mode() {
    let (app, _tmp) = test_app();
    assert_eq!(app.nav_mode, NavMode::Global);
}

#[tokio::test]
async fn global_mode_hjkl_navigates_panels() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.nav_mode, NavMode::Global);
    assert_eq!(app.focus, FocusArea::Sidebar);

    // l: Sidebar → UrlBar (last_right_focus default)
    handle_event(&mut app, key_char('l')).await.unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);

    // j: UrlBar → RequestBody
    handle_event(&mut app, key_char('j')).await.unwrap();
    assert_eq!(app.focus, FocusArea::RequestBody);

    // j: RequestBody → ResponseBody
    handle_event(&mut app, key_char('j')).await.unwrap();
    assert_eq!(app.focus, FocusArea::ResponseBody);

    // j: ResponseBody stays (no further down)
    handle_event(&mut app, key_char('j')).await.unwrap();
    assert_eq!(app.focus, FocusArea::ResponseBody);

    // k: ResponseBody → RequestBody
    handle_event(&mut app, key_char('k')).await.unwrap();
    assert_eq!(app.focus, FocusArea::RequestBody);

    // k: RequestBody → UrlBar
    handle_event(&mut app, key_char('k')).await.unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);

    // h: UrlBar → Sidebar
    handle_event(&mut app, key_char('h')).await.unwrap();
    assert_eq!(app.focus, FocusArea::Sidebar);

    // h: Sidebar stays
    handle_event(&mut app, key_char('h')).await.unwrap();
    assert_eq!(app.focus, FocusArea::Sidebar);
}

#[tokio::test]
async fn enter_transitions_global_to_panel() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.nav_mode, NavMode::Global);

    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();
    assert_eq!(app.nav_mode, NavMode::Panel);
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn esc_transitions_panel_to_global() {
    let (mut app, _tmp) = test_app();
    app.nav_mode = NavMode::Panel;

    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.nav_mode, NavMode::Global);
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn esc_snaps_focus_from_request_tabs_to_request_body() {
    let (mut app, _tmp) = test_app();
    app.nav_mode = NavMode::Panel;
    app.focus = FocusArea::RequestTabs;

    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.nav_mode, NavMode::Global);
    assert_eq!(app.focus, FocusArea::RequestBody);
}

#[tokio::test]
async fn alt_hjkl_navigates_panels_in_panel_mode() {
    let (mut app, _tmp) = test_app();
    app.nav_mode = NavMode::Panel;
    app.focus = FocusArea::Sidebar;

    // Alt+l: Sidebar → UrlBar
    handle_event(&mut app, key_mod(KeyCode::Char('l'), KeyModifiers::ALT))
        .await
        .unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);

    // Alt+j: UrlBar → RequestBody
    handle_event(&mut app, key_mod(KeyCode::Char('j'), KeyModifiers::ALT))
        .await
        .unwrap();
    assert_eq!(app.focus, FocusArea::RequestBody);

    // Alt+h: RequestBody → Sidebar
    handle_event(&mut app, key_mod(KeyCode::Char('h'), KeyModifiers::ALT))
        .await
        .unwrap();
    assert_eq!(app.focus, FocusArea::Sidebar);
}

#[tokio::test]
async fn insert_mode_sets_panel_nav() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.nav_mode, NavMode::Global);

    // i enters insert + panel mode
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.mode, AppMode::Insert);
    assert_eq!(app.nav_mode, NavMode::Panel);

    // Esc from insert → Normal + Panel
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.nav_mode, NavMode::Panel);
}

#[tokio::test]
async fn key_q_in_panel_mode_goes_to_global() {
    let (mut app, _tmp) = test_app();
    app.nav_mode = NavMode::Panel;
    app.focus = FocusArea::UrlBar;

    handle_event(&mut app, key_char('q')).await.unwrap();
    assert!(!app.should_quit);
    assert_eq!(app.nav_mode, NavMode::Global);
}

#[tokio::test]
async fn global_nav_right_remembers_last_right_focus() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.nav_mode, NavMode::Global);

    // Go right to UrlBar (default last_right_focus)
    handle_event(&mut app, key_char('l')).await.unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);

    // Go down to RequestBody
    handle_event(&mut app, key_char('j')).await.unwrap();
    assert_eq!(app.focus, FocusArea::RequestBody);

    // Go left to Sidebar (saves last_right_focus = RequestBody)
    handle_event(&mut app, key_char('h')).await.unwrap();
    assert_eq!(app.focus, FocusArea::Sidebar);

    // Go right again → should go to RequestBody (remembered)
    handle_event(&mut app, key_char('l')).await.unwrap();
    assert_eq!(app.focus, FocusArea::RequestBody);
}

// ════════════════════════════════════════════════════════════════════════════
// Chain commands
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_newchain_creates_chain() {
    let (mut app, _tmp) = test_app();
    // Need a collection first
    execute_command(&mut app, "newcol TestCol").await.unwrap();
    assert_eq!(app.collections.len(), 1);

    execute_command(&mut app, "newchain Login Flow")
        .await
        .unwrap();
    assert_eq!(app.collections[0].chains.len(), 1);
    assert_eq!(app.collections[0].chains[0].name, "Login Flow");
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success
    );
}

#[tokio::test]
async fn cmd_newchain_no_collection_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newchain Flow").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("No collections"));
}

#[tokio::test]
async fn cmd_addstep_adds_to_chain() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol TestCol").await.unwrap();

    // Add a request to the collection
    let req = Request::new("login", HttpMethod::POST, "http://localhost/login");
    let req_name = "login".to_string();
    app.collections[0].add_request(req);

    // Create chain
    execute_command(&mut app, "newchain Flow").await.unwrap();

    // Add step
    execute_command(&mut app, "addstep login").await.unwrap();
    assert_eq!(app.collections[0].chains[0].steps.len(), 1);
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains(&req_name));
}

#[tokio::test]
async fn cmd_addstep_request_not_found() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol TestCol").await.unwrap();
    execute_command(&mut app, "newchain Flow").await.unwrap();
    execute_command(&mut app, "addstep nonexistent")
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("not found"));
}

#[tokio::test]
async fn cmd_chain_no_args_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "chain").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
    assert!(app.notification.as_ref().unwrap().message.contains("Usage"));
}

#[tokio::test]
async fn chain_sidebar_enter_starts_execution() {
    let (mut app, _tmp) = test_app();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);

    // Setup collection with a chain
    let mut coll = Collection::new("TestCol");
    let req = Request::new("login", HttpMethod::POST, "http://localhost/login");
    let req_id = req.id;
    coll.add_request(req);
    let mut chain = RequestChain::new("Flow");
    chain.add_step(req_id);
    coll.chains.push(chain);
    app.collections.push(coll);

    // Switch sidebar to Chains section and select it
    app.sidebar_state.section = SidebarSection::Chains;
    app.sidebar_state.selected = 0;
    app.focus = FocusArea::Sidebar;
    app.nav_mode = NavMode::Panel;

    // Press Enter on the chain
    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();

    // Verify mode switched to ChainEditor
    assert_eq!(app.mode, AppMode::ChainEditor);
    assert!(app.active_chain.is_some());
    assert!(app.active_chain_def.is_some());
    assert_eq!(app.active_chain_def.as_ref().unwrap().name, "Flow");
}

// ════════════════════════════════════════════════════════════════════════════
// :importchain — YAML chain import
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_importchain_creates_chain() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol TestAPI").await.unwrap();

    // Add requests that match the YAML fixture
    let login = Request::new("login", HttpMethod::POST, "http://localhost/login");
    let get_user = Request::new("get-user", HttpMethod::GET, "http://localhost/user");
    app.collections[0].add_request(login);
    app.collections[0].add_request(get_user);

    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_chain.yaml");
    let cmd = format!("importchain {}", fixture.display());
    execute_command(&mut app, &cmd).await.unwrap();

    assert_eq!(app.collections[0].chains.len(), 1);
    let chain = &app.collections[0].chains[0];
    assert_eq!(chain.name, "Login Flow");
    assert_eq!(chain.description.as_deref(), Some("Authentication flow"));
    assert_eq!(chain.steps.len(), 2);
    assert_eq!(chain.steps[0].delay_ms, Some(500));
    assert_eq!(chain.steps[0].extractions.len(), 1);
    assert_eq!(chain.steps[0].extractions[0].variable_name, "auth_token");
    assert!(chain.steps[1].condition.is_some());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success,
    );
}

#[tokio::test]
async fn cmd_importchain_request_not_found() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol TestAPI").await.unwrap();

    // Only add "login", not "get-user" — should error
    let login = Request::new("login", HttpMethod::POST, "http://localhost/login");
    app.collections[0].add_request(login);

    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_chain.yaml");
    let cmd = format!("importchain {}", fixture.display());
    execute_command(&mut app, &cmd).await.unwrap();

    assert_eq!(app.collections[0].chains.len(), 0);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error,
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("get-user"));
}

#[tokio::test]
async fn cmd_importchain_no_collection() {
    let (mut app, _tmp) = test_app();

    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_chain.yaml");
    let cmd = format!("importchain {}", fixture.display());
    execute_command(&mut app, &cmd).await.unwrap();

    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning,
    );
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("No collections"));
}

#[tokio::test]
async fn import_yaml_auto_detects_chain() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "newcol TestAPI").await.unwrap();

    let login = Request::new("login", HttpMethod::POST, "http://localhost/login");
    let get_user = Request::new("get-user", HttpMethod::GET, "http://localhost/user");
    app.collections[0].add_request(login);
    app.collections[0].add_request(get_user);

    // Use :import with a .yaml file that has a steps key — should auto-detect as chain
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_chain.yaml");
    let cmd = format!("import {}", fixture.display());
    execute_command(&mut app, &cmd).await.unwrap();

    assert_eq!(app.collections[0].chains.len(), 1);
    assert_eq!(app.collections[0].chains[0].name, "Login Flow");
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Success,
    );
}
