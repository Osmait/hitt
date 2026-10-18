use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use hitt_core::core::chain::RequestChain;
use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::request::{HttpMethod, Request};
use hitt_core::storage::config::AppConfig;
use hitt_tui::app::{
    App, AppMode, FocusArea, ModalKind, NavMode, NotificationKind, RequestTabKind, ResponseTabKind,
    SidebarSection,
};
use hitt_tui::event::{build_sidebar_items, execute_command, handle_event, AppEvent, SidebarItem};

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

    // Screen order: response tab strip, then the response body.
    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::ResponseTabs);

    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(app.focus, FocusArea::ResponseBody);

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
    app.focus = FocusArea::UrlBar;
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.mode, AppMode::Insert);
}

#[tokio::test]
async fn key_i_on_sidebar_opens_import() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.focus, FocusArea::Sidebar);
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.mode, AppMode::Modal(ModalKind::Import));
}

#[tokio::test]
async fn key_i_without_editor_moves_focus_to_url_bar() {
    let (mut app, _tmp) = test_app();
    // The request area has its own editors; the response tab bar does not.
    app.focus = FocusArea::ResponseTabs;
    app.nav_mode = NavMode::Panel;
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.mode, AppMode::Insert);
    assert_eq!(app.focus, FocusArea::UrlBar);
    handle_event(&mut app, key_char('x')).await.unwrap();
    assert_eq!(app.active_tab().request.url, "x");
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

    // Deleting the last character keeps an empty `:` (like vim)...
    handle_event(&mut app, key(KeyCode::Backspace))
        .await
        .unwrap();
    assert!(app.command_input.is_empty());
    assert_eq!(app.mode, AppMode::Command);

    // ...and Backspace on the empty line leaves command mode.
    handle_event(&mut app, key(KeyCode::Backspace))
        .await
        .unwrap();
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

    // After the last environment comes "No Env", then it wraps.
    handle_event(&mut app, key_char('e')).await.unwrap();
    assert_eq!(app.active_env, None);
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("No Env"));

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
    app.notification = Some(hitt_tui::app::Notification {
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
    // Asks first; nothing is deleted until confirmed.
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    assert_eq!(app.collections[0].request_count(), 2);
    handle_event(&mut app, key_char('y')).await.unwrap();
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
            hitt_core::core::request::RequestBody::Json(r#"{"key":"val"}"#.into()),
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
    coll.variables
        .push(hitt_core::core::request::KeyValuePair::new(
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
async fn ctrl_o_opens_import() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_mod(KeyCode::Char('o'), KeyModifiers::CONTROL))
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
async fn cmd_set_vim_mode_is_not_a_setting() {
    // `vim_mode` was never read; `:set` no longer offers it (and the config
    // no longer has the field).
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "set vim_mode false")
        .await
        .unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
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
    // Asks first; nothing is deleted until confirmed.
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    assert_eq!(app.collections.len(), 2);
    handle_event(&mut app, key_char('y')).await.unwrap();
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
    app.history.add(hitt_core::core::history::HistoryEntry::new(
        HttpMethod::GET,
        "https://example.com",
    ));
    app.history.add(hitt_core::core::history::HistoryEntry::new(
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

use hitt_core::protocols::sse::{SseEvent as ProtocolSseEvent, SseSession};
use hitt_core::protocols::websocket::{
    MessageDirection, WebSocketSession, WsContent, WsMessage, WsStatus,
};
use hitt_tui::event::{
    handle_sse_protocol_event, handle_ws_protocol_event, SseEventData, WsEventData,
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
    use hitt_core::core::request::Protocol;
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
    use hitt_core::core::request::Protocol;
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
    use hitt_core::core::request::Protocol;
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
    use hitt_core::core::request::Protocol;
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
    app.focus = FocusArea::UrlBar;
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
        NotificationKind::Error
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

// ════════════════════════════════════════════════════════════════════════════
// Text input accepts shifted characters
// ════════════════════════════════════════════════════════════════════════════

fn shift_char(c: char) -> AppEvent {
    key_mod(KeyCode::Char(c), KeyModifiers::SHIFT)
}

#[tokio::test]
async fn insert_mode_accepts_shifted_chars() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::UrlBar;
    handle_event(&mut app, key_char('i')).await.unwrap();
    for ev in [
        key_char('a'),
        shift_char('B'),
        shift_char('?'),
        shift_char('&'),
    ] {
        handle_event(&mut app, ev).await.unwrap();
    }
    assert_eq!(app.active_tab().request.url, "aB?&");
    // Ctrl chords are not text.
    handle_event(&mut app, key_mod(KeyCode::Char('z'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.active_tab().request.url, "aB?&");
}

#[tokio::test]
async fn response_filter_accepts_shifted_chars() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::ResponseBody;
    handle_event(&mut app, key_char('/')).await.unwrap();
    assert!(app.response_filter_active);
    handle_event(&mut app, shift_char('I')).await.unwrap();
    handle_event(&mut app, key_char('d')).await.unwrap();
    assert_eq!(app.active_tab().response_filter, "Id");
}

#[tokio::test]
async fn ws_input_accepts_shifted_chars() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.protocol = hitt_core::core::request::Protocol::WebSocket;
    app.focus = FocusArea::ResponseBody;
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.mode, AppMode::Insert);
    handle_event(&mut app, shift_char('H')).await.unwrap();
    handle_event(&mut app, key_char('i')).await.unwrap();
    assert_eq!(app.active_tab().ws_message_input, "Hi");
}

#[tokio::test]
async fn command_and_rename_inputs_accept_shifted_chars() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char(':')).await.unwrap();
    handle_event(&mut app, shift_char('A')).await.unwrap();
    assert_eq!(app.command_input, "A");
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();

    handle_event(&mut app, key(KeyCode::F(2))).await.unwrap();
    app.rename_input.clear();
    handle_event(&mut app, shift_char('X')).await.unwrap();
    assert_eq!(app.rename_input, "X");
}

// ════════════════════════════════════════════════════════════════════════════
// Scroll clamping (G must not panic the renderer)
// ════════════════════════════════════════════════════════════════════════════

fn text_response(body: String) -> hitt_core::core::response::Response {
    hitt_core::core::response::Response {
        id: uuid::Uuid::new_v4(),
        status: 200,
        status_text: "OK".into(),
        headers: Vec::new(),
        body: hitt_core::core::response::ResponseBody::Text(body),
        cookies: Vec::new(),
        timing: hitt_core::core::response::RequestTiming::default(),
        size: hitt_core::core::response::ResponseSize::default(),
        assertion_results: Vec::new(),
        timestamp: chrono::Utc::now(),
    }
}

fn draw(
    app: &mut App,
    width: u16,
    height: u16,
) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| hitt_tui::ui::layout::render(app, frame))
        .unwrap();
    terminal
}

fn screen_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

#[tokio::test]
async fn shift_g_scrolls_to_bottom_without_panicking() {
    let (mut app, _tmp) = test_app();
    let body = (0..200)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    app.active_tab_mut().response = Some(text_response(body));
    app.focus = FocusArea::ResponseBody;
    app.nav_mode = NavMode::Panel;

    handle_event(&mut app, shift_char('G')).await.unwrap();
    draw(&mut app, 100, 40);
    // Clamped to the last page and stored back.
    assert!(app.response_scroll < 200);
    assert!(app.response_scroll > 150);
    let bottom = app.response_scroll;

    // j at the bottom doesn't move past the end; k scrolls up from it.
    handle_event(&mut app, key_char('j')).await.unwrap();
    draw(&mut app, 100, 40);
    assert_eq!(app.response_scroll, bottom);
    handle_event(&mut app, key_char('k')).await.unwrap();
    assert_eq!(app.response_scroll, bottom - 1);
}

#[tokio::test]
async fn help_shift_g_is_clamped() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('?')).await.unwrap();
    handle_event(&mut app, shift_char('G')).await.unwrap();
    draw(&mut app, 100, 40);
    let bottom = app.help_scroll;
    assert!(bottom > 0 && bottom < 500);
    handle_event(&mut app, key_char('k')).await.unwrap();
    assert_eq!(app.help_scroll, bottom - 1);
}

#[tokio::test]
async fn request_body_scrolls_independently() {
    let (mut app, _tmp) = test_app();
    let body = (0..100)
        .map(|i| format!("\"k{i}\": {i},"))
        .collect::<Vec<_>>()
        .join("\n");
    app.active_tab_mut().request.body = Some(hitt_core::core::request::RequestBody::Json(body));
    app.active_tab_mut().request_tab = RequestTabKind::Body;
    app.focus = FocusArea::RequestBody;
    app.nav_mode = NavMode::Panel;
    handle_event(&mut app, shift_char('G')).await.unwrap();
    draw(&mut app, 100, 40);
    assert!(app.request_scroll > 0 && app.request_scroll < 100);
    assert_eq!(app.response_scroll, 0);
}

#[tokio::test]
async fn ctrl_d_ctrl_u_half_page() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::ResponseBody;
    handle_event(&mut app, key_mod(KeyCode::Char('d'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.response_scroll, 15);
    handle_event(&mut app, key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.response_scroll, 0);
}

// ════════════════════════════════════════════════════════════════════════════
// Layout: truncation / tab visibility
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn layout_keeps_labels_visible_at_95_cols() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().response = Some(text_response("{}".into()));
    let terminal = draw(&mut app, 95, 30);
    let text = screen_text(&terminal);
    assert!(text.contains("History"), "sidebar section label cut off");
    assert!(text.contains("Time"), "response tab label cut off");
    assert!(!text.contains("Timin "), "response tab label truncated");
}

#[tokio::test]
async fn header_tab_bar_scrolls_to_active_tab() {
    let (mut app, _tmp) = test_app();
    for i in 0..15 {
        app.new_tab();
        app.active_tab_mut().request.name = format!("Tab{i:02}");
    }
    let terminal = draw(&mut app, 95, 30);
    let text = screen_text(&terminal);
    assert!(text.contains("Tab14"), "active tab not visible in tab bar");
    // Clickable region for the active tab is recorded.
    assert!(app
        .regions
        .header_tabs
        .iter()
        .any(|(_, idx)| *idx == app.active_tab));
}

// ════════════════════════════════════════════════════════════════════════════
// Non-blocking HTTP requests
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn send_with_empty_url_warns() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert!(!app.loading);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

#[tokio::test]
async fn http_request_runs_in_background_and_completes() {
    let (mut app, _tmp) = test_app();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    // Nothing listens on the port: the request fails fast with connection refused.
    app.active_tab_mut().request.url = format!("http://127.0.0.1:{}/", refused_port());

    handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert!(app.loading);
    assert!(app.active_tab().pending_request.is_some());

    // A second send while pending is ignored.
    handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("already in progress"));

    // Switching tabs doesn't lose the result.
    handle_event(&mut app, key_char('t')).await.unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, AppEvent::HttpResponse { .. }));
    handle_event(&mut app, event).await.unwrap();
    assert!(!app.loading);
    assert!(app.tabs[0].pending_request.is_none());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

#[tokio::test]
async fn esc_cancels_in_flight_request() {
    let (mut app, _tmp) = test_app();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    // The listener accepts the connection but never answers: the request
    // hangs until cancelled.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    app.active_tab_mut().request.url =
        format!("http://127.0.0.1:{}/", silent.local_addr().unwrap().port());
    handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert!(app.loading);

    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert!(!app.loading);
    assert!(app.active_tab().pending_request.is_none());
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("cancelled"));
}

/// A local port with nothing listening on it (bound, then released).
fn refused_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Accept one connection and read the HTTP request head, never answering.
async fn accept_silently(listener: &tokio::net::TcpListener) -> (tokio::net::TcpStream, String) {
    use tokio::io::AsyncReadExt;
    let (mut stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept())
            .await
            .expect("client never connected")
            .unwrap();
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = tokio::time::timeout(std::time::Duration::from_secs(10), stream.read(&mut buf))
            .await
            .expect("request head never arrived")
            .unwrap();
        assert!(n > 0, "client closed before sending the request head");
        head.extend_from_slice(&buf[..n]);
    }
    (stream, String::from_utf8_lossy(&head).into_owned())
}

/// Wait until the peer closes the connection (read returns EOF or an error).
async fn assert_peer_closes(stream: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 1024];
    loop {
        let read = tokio::time::timeout(std::time::Duration::from_secs(10), stream.read(&mut buf))
            .await
            .expect("connection was not closed after cancel");
        match read {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

fn ws_status(app: &App) -> hitt_core::protocols::websocket::WsStatus {
    app.active_tab().ws_session.as_ref().unwrap().status.clone()
}

/// Point the active tab at a WebSocket server that accepts TCP but never
/// completes the handshake.
async fn ws_app_with_silent_server() -> (App, TempDir, tokio::net::TcpListener) {
    let (mut app, tmp) = test_app();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let tab = app.active_tab_mut();
    tab.request.protocol = hitt_core::core::request::Protocol::WebSocket;
    tab.response_tab = ResponseTabKind::WsMessages;
    tab.request.url = format!("ws://127.0.0.1:{port}/");
    (app, tmp, listener)
}

#[tokio::test]
async fn ws_connect_does_not_block_and_esc_cancels() {
    use hitt_core::protocols::websocket::WsStatus;
    let (mut app, _tmp, listener) = ws_app_with_silent_server().await;

    // Sending returns right away even though the handshake never completes.
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL)),
    )
    .await
    .expect("WebSocket connect blocked the event loop")
    .unwrap();
    assert_eq!(ws_status(&app), WsStatus::Connecting);
    assert!(app.active_tab().ws_cmd_sender.is_some());

    // The handshake reached the server with hitt's User-Agent.
    let (mut stream, head) = accept_silently(&listener).await;
    let ua = format!("user-agent: {}", hitt_core::core::client::user_agent());
    assert!(
        head.to_ascii_lowercase().contains(&ua),
        "missing UA in: {head}"
    );

    // The app stays responsive while connecting.
    handle_event(&mut app, key(KeyCode::Tab)).await.unwrap();
    assert_eq!(ws_status(&app), WsStatus::Connecting);

    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(ws_status(&app), WsStatus::Disconnected);
    assert!(app.active_tab().ws_cmd_sender.is_none());
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("cancelled"));
    // Cancelling aborts the pending handshake.
    assert_peer_closes(&mut stream).await;
}

#[tokio::test]
async fn ws_connect_toggle_again_cancels_and_ignores_late_connected() {
    use hitt_core::protocols::websocket::WsStatus;
    let (mut app, _tmp, listener) = ws_app_with_silent_server().await;
    let send = || key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL);

    handle_event(&mut app, send()).await.unwrap();
    let (mut stream, _) = accept_silently(&listener).await;
    let session_id = app.active_tab().ws_session.as_ref().unwrap().id;

    handle_event(&mut app, send()).await.unwrap();
    assert_eq!(ws_status(&app), WsStatus::Disconnected);
    assert_peer_closes(&mut stream).await;

    // A handshake result that raced with the cancel doesn't revive the session.
    hitt_tui::event::handle_ws_protocol_event(
        &mut app,
        session_id,
        hitt_tui::event::WsEventData::Connected,
    );
    assert_eq!(ws_status(&app), WsStatus::Disconnected);
}

#[tokio::test]
async fn closing_tab_while_ws_connecting_aborts_handshake() {
    let (mut app, _tmp, listener) = ws_app_with_silent_server().await;
    handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    let (mut stream, _) = accept_silently(&listener).await;

    app.new_tab();
    app.switch_tab(0);
    app.close_tab();
    assert_eq!(app.tabs.len(), 1);
    assert_peer_closes(&mut stream).await;
}

#[tokio::test]
async fn ws_connect_refused_reports_error() {
    use hitt_core::protocols::websocket::WsStatus;
    let (mut app, _tmp) = test_app();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    let tab = app.active_tab_mut();
    tab.request.protocol = hitt_core::core::request::Protocol::WebSocket;
    tab.request.url = format!("ws://127.0.0.1:{}/", refused_port());

    handle_event(&mut app, key_mod(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, AppEvent::WebSocketEvent { .. }));
    handle_event(&mut app, event).await.unwrap();
    assert!(matches!(ws_status(&app), WsStatus::Error(_)));
    assert!(app.active_tab().ws_cmd_sender.is_none());
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
}

#[tokio::test]
async fn header_tab_click_maps_to_scrolled_tab() {
    let (mut app, _tmp) = test_app();
    for i in 0..15 {
        app.new_tab();
        app.active_tab_mut().request.name = format!("Tab{i:02}");
    }
    draw(&mut app, 95, 30);
    let (first_rect, first_idx) = app.regions.header_tabs[0];
    assert!(first_idx > 0, "tab bar should be scrolled");

    let click = |x: u16, y: u16| {
        AppEvent::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    };
    handle_event(&mut app, click(first_rect.x, first_rect.y))
        .await
        .unwrap();
    assert_eq!(app.active_tab, first_idx);

    // Every recorded region (up to its right edge) selects the tab rendered there.
    app.switch_tab(app.tabs.len() - 1);
    draw(&mut app, 95, 30);
    for (rect, idx) in app.regions.header_tabs.clone() {
        handle_event(&mut app, click(rect.x + rect.width - 1, rect.y))
            .await
            .unwrap();
        assert_eq!(app.active_tab, idx);
    }
}

#[tokio::test]
async fn stale_http_result_is_ignored() {
    let (mut app, _tmp) = test_app();
    let tab_id = app.active_tab().id;
    let req = app.active_tab().request.clone();
    app.finish_http_request(
        tab_id,
        uuid::Uuid::new_v4(),
        &req,
        Ok(Box::new(text_response("late".into()))),
    );
    assert!(app.active_tab().response.is_none());
    assert!(app.history.entries().is_empty());
}

#[tokio::test]
async fn loadtest_with_empty_url_warns() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "loadtest 10 2").await.unwrap();
    assert!(!app.load_test_running);
    let n = app.notification.as_ref().unwrap();
    assert_eq!(n.kind, NotificationKind::Warning);
    assert!(n.message.contains("URL"));
}

#[tokio::test]
async fn loadtest_bad_args_shows_usage() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "loadtest abc").await.unwrap();
    assert!(app.notification.as_ref().unwrap().message.contains("Usage"));
}

// ════════════════════════════════════════════════════════════════════════════
// Focus / nav mode
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn new_tab_focuses_url_bar() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.focus, FocusArea::Sidebar);
    handle_event(&mut app, key_char('t')).await.unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);
    app.focus = FocusArea::Sidebar;
    handle_event(&mut app, key_mod(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    assert_eq!(app.focus, FocusArea::UrlBar);
}

#[tokio::test]
async fn closing_modal_restores_nav_mode() {
    let (mut app, _tmp) = test_app();
    assert_eq!(app.nav_mode, NavMode::Global);
    handle_event(&mut app, key_char('?')).await.unwrap();
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.nav_mode, NavMode::Global);

    handle_event(&mut app, key_char(':')).await.unwrap();
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.nav_mode, NavMode::Global);

    // Panel mode is preserved too.
    app.nav_mode = NavMode::Panel;
    handle_event(&mut app, key_mod(KeyCode::Char('x'), KeyModifiers::CONTROL))
        .await
        .unwrap();
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.nav_mode, NavMode::Panel);
}

#[tokio::test]
async fn sidebar_brackets_switch_sections() {
    let (mut app, _tmp) = test_app();
    app.focus = FocusArea::Sidebar;
    assert_eq!(app.sidebar_state.section, SidebarSection::Collections);
    handle_event(&mut app, key_char(']')).await.unwrap();
    assert_eq!(app.sidebar_state.section, SidebarSection::Chains);
    handle_event(&mut app, key_char(']')).await.unwrap();
    assert_eq!(app.sidebar_state.section, SidebarSection::History);
    handle_event(&mut app, key_char(']')).await.unwrap();
    assert_eq!(app.sidebar_state.section, SidebarSection::Collections);
    handle_event(&mut app, key_char('[')).await.unwrap();
    assert_eq!(app.sidebar_state.section, SidebarSection::History);
}

// ════════════════════════════════════════════════════════════════════════════
// Confirmation dialogs
// ════════════════════════════════════════════════════════════════════════════

fn app_with_expanded_request() -> (App, TempDir) {
    let (mut app, tmp) = test_app();
    let mut coll = Collection::new("API");
    coll.add_request(Request::new("R1", HttpMethod::GET, "/r1"));
    app.sidebar_state.expanded.insert(coll.id);
    app.collections.push(coll);
    app.focus = FocusArea::Sidebar;
    app.sidebar_state.selected = 1; // the request row
    (app, tmp)
}

#[tokio::test]
async fn sidebar_x_asks_before_deleting() {
    let (mut app, _tmp) = app_with_expanded_request();
    handle_event(&mut app, key_char('x')).await.unwrap();
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    assert_eq!(app.collections[0].request_count(), 1);

    // n cancels
    handle_event(&mut app, key_char('n')).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.collections[0].request_count(), 1);

    // y confirms
    handle_event(&mut app, key_char('x')).await.unwrap();
    handle_event(&mut app, key_char('y')).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.collections[0].request_count(), 0);
}

#[tokio::test]
async fn esc_on_confirm_does_nothing() {
    let (mut app, _tmp) = app_with_expanded_request();
    handle_event(&mut app, key_char('x')).await.unwrap();
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.pending_confirm.is_none());
    assert_eq!(app.collections[0].request_count(), 1);
}

#[tokio::test]
async fn w_on_dirty_tab_asks_first() {
    let (mut app, _tmp) = test_app();
    handle_event(&mut app, key_char('t')).await.unwrap();
    app.active_tab_mut().dirty = true;
    handle_event(&mut app, key_char('w')).await.unwrap();
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    assert_eq!(app.tabs.len(), 2);
    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();
    assert_eq!(app.tabs.len(), 1);
}

#[tokio::test]
async fn q_with_dirty_tabs_asks_first() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().dirty = true;
    handle_event(&mut app, key_char('q')).await.unwrap();
    assert!(!app.should_quit);
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    handle_event(&mut app, key_char('y')).await.unwrap();
    assert!(app.should_quit);
}

#[tokio::test]
async fn cmd_quit_bang_skips_confirmation() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().dirty = true;
    execute_command(&mut app, "q").await.unwrap();
    assert!(!app.should_quit);
    app.mode = AppMode::Normal;
    execute_command(&mut app, "q!").await.unwrap();
    assert!(app.should_quit);
}

// ════════════════════════════════════════════════════════════════════════════
// Save after the request was deleted from its collection
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn save_readds_request_missing_from_collection() {
    let (mut app, _tmp) = test_app();
    app.collections.push(Collection::new("API"));
    app.active_tab_mut().collection_index = Some(0);
    app.active_tab_mut().dirty = true;
    handle_event(&mut app, key_char('s')).await.unwrap();
    assert_eq!(app.collections[0].request_count(), 1);
    assert!(!app.active_tab().dirty);
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("re-added"));
}

// ════════════════════════════════════════════════════════════════════════════
// :ws / :sse open a new tab
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cmd_ws_opens_new_tab_when_current_has_work() {
    let (mut app, _tmp) = test_app();
    app.active_tab_mut().request.url = "https://api.example.com/keep".into();
    app.active_tab_mut().dirty = true;
    execute_command(&mut app, "ws ws://127.0.0.1:9/")
        .await
        .unwrap();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.tabs[0].request.url, "https://api.example.com/keep");
    assert_eq!(app.tabs[1].request.url, "ws://127.0.0.1:9/");
    assert_eq!(app.active_tab, 1);
}

#[tokio::test]
async fn cmd_sse_reuses_blank_tab() {
    let (mut app, _tmp) = test_app();
    execute_command(&mut app, "sse http://127.0.0.1:9/events")
        .await
        .unwrap();
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].request.url, "http://127.0.0.1:9/events");
    assert_eq!(
        app.tabs[0].request.protocol,
        hitt_core::core::request::Protocol::Sse
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Chains: :addstep targeting
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn addstep_targets_active_chain_and_its_collection_only() {
    let (mut app, _tmp) = test_app();
    let mut a = Collection::new("A");
    a.add_request(Request::new("login", HttpMethod::POST, "/login"));
    let mut b = Collection::new("B");
    b.add_request(Request::new("other", HttpMethod::GET, "/other"));
    app.collections.push(a);
    app.collections.push(b);

    // Chain created in collection A (active tab has no collection -> first).
    execute_command(&mut app, "newchain Flow").await.unwrap();
    // A chain in B, created afterwards, must not steal steps from :addstep
    // unless it becomes the active one.
    let mut chain_b = RequestChain::new("BFlow");
    chain_b.steps.clear();
    app.collections[1].chains.push(chain_b);

    execute_command(&mut app, "addstep login").await.unwrap();
    assert_eq!(app.collections[0].chains[0].steps.len(), 1);
    assert_eq!(app.collections[1].chains[0].steps.len(), 0);

    // Requests from another collection are rejected.
    execute_command(&mut app, "addstep other").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
    assert_eq!(app.collections[0].chains[0].steps.len(), 1);

    // --chain targets a named chain explicitly.
    execute_command(&mut app, "addstep other --chain BFlow")
        .await
        .unwrap();
    assert_eq!(app.collections[1].chains[0].steps.len(), 1);
}

#[tokio::test]
async fn addstep_without_active_chain_warns() {
    let (mut app, _tmp) = test_app();
    let mut a = Collection::new("A");
    a.add_request(Request::new("login", HttpMethod::POST, "/login"));
    a.chains.push(RequestChain::new("Existing"));
    app.collections.push(a);
    execute_command(&mut app, "addstep login").await.unwrap();
    assert!(app
        .notification
        .as_ref()
        .unwrap()
        .message
        .contains("No active chain"));
}

// ════════════════════════════════════════════════════════════════════════════
// Environments
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn import_postman_environment_activates_it() {
    let (mut app, _tmp) = test_app();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sample_postman_environment.json");
    execute_command(&mut app, &format!("import {}", fixture.display()))
        .await
        .unwrap();
    assert_eq!(app.environments.len(), 1);
    assert_eq!(app.active_env, Some(0));
}

#[tokio::test]
async fn chain_with_missing_request_fails_and_esc_restores_focus() {
    let (mut app, _tmp) = test_app();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.event_sender = Some(tx);
    let mut coll = Collection::new("A");
    let mut chain = RequestChain::new("Broken");
    chain.add_step(uuid::Uuid::new_v4()); // request that doesn't exist
    coll.chains.push(chain);
    app.collections.push(coll);
    app.focus = FocusArea::Sidebar;

    execute_command(&mut app, "chain Broken").await.unwrap();
    assert_eq!(app.mode, AppMode::ChainEditor);
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let done = matches!(
            event,
            AppEvent::ChainStepComplete {
                event: hitt_tui::event::ChainStepEvent::Complete,
                ..
            }
        );
        handle_event(&mut app, event).await.unwrap();
        if done {
            break;
        }
    }
    let state = app.active_chain.as_ref().unwrap();
    assert_eq!(hitt_tui::event::chain_failed_steps(state), 1);
    match &state.step_statuses[0] {
        hitt_core::core::chain::ChainStepStatus::Failed { error } => {
            assert!(error.contains("not found"));
        }
        other => panic!("expected failed step, got {other:?}"),
    }
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );

    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.focus, FocusArea::Sidebar);
}
