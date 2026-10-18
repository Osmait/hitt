//! Remembering the last active environment, and renaming / deleting
//! environments from the TUI. Everything is written to temp dirs.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use hitt_core::core::environment::Environment;
use hitt_core::storage::collections_store::CollectionsStore;
use hitt_core::storage::config::AppConfig;
use hitt_tui::app::{App, AppMode, ConfirmAction, ModalKind, NotificationKind};
use hitt_tui::event::{execute_command, handle_event, AppEvent};

/// App whose collections, environments and config.toml live in a temp dir.
fn test_app() -> (App, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    let mut app = App::new(config).unwrap();
    app.config_path = Some(tmp.path().join("config.toml"));
    (app, tmp)
}

fn config_on_disk(tmp: &TempDir) -> Option<AppConfig> {
    AppConfig::load_from(&tmp.path().join("config.toml")).ok()
}

fn env_file(tmp: &TempDir, name: &str) -> std::path::PathBuf {
    tmp.path().join("environments").join(format!("{name}.json"))
}

/// Add a saved (persisted) environment.
fn add_saved_env(app: &mut App, name: &str) {
    let env = Environment::new(name);
    CollectionsStore::new(app.config.collections_dir.clone())
        .unwrap()
        .save_environment(&env)
        .unwrap();
    app.environments.push(env);
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn char_key(c: char) -> AppEvent {
    let modifiers = if c.is_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    AppEvent::Key(KeyEvent::new(KeyCode::Char(c), modifiers))
}

async fn press(app: &mut App, event: AppEvent) {
    handle_event(app, event).await.unwrap();
}

async fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, char_key(c)).await;
    }
}

fn draw(app: &mut App) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 40)).unwrap();
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
// Remember the last environment
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cycling_environment_persists_last_environment() {
    let (mut app, tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.environments.push(Environment::new("Prod"));

    press(&mut app, key(KeyCode::Char('e'))).await;
    assert_eq!(app.config.last_environment.as_deref(), Some("Dev"));
    assert_eq!(
        config_on_disk(&tmp).unwrap().last_environment.as_deref(),
        Some("Dev")
    );

    press(&mut app, key(KeyCode::Char('e'))).await;
    assert_eq!(
        config_on_disk(&tmp).unwrap().last_environment.as_deref(),
        Some("Prod")
    );
}

#[tokio::test]
async fn env_newenv_and_dupenv_commands_persist_last_environment() {
    let (mut app, tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.environments.push(Environment::new("Prod"));

    execute_command(&mut app, "env Prod").await.unwrap();
    assert_eq!(
        config_on_disk(&tmp).unwrap().last_environment.as_deref(),
        Some("Prod")
    );
    execute_command(&mut app, "newenv Staging").await.unwrap();
    assert_eq!(
        config_on_disk(&tmp).unwrap().last_environment.as_deref(),
        Some("Staging")
    );
    execute_command(&mut app, "dupenv Copy").await.unwrap();
    assert_eq!(
        config_on_disk(&tmp).unwrap().last_environment.as_deref(),
        Some("Copy")
    );
}

#[tokio::test]
async fn session_only_environment_is_not_remembered() {
    let (mut app, tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    let session = Environment::new("local.env");
    app.session_env_ids.insert(session.id);
    app.environments.push(session);

    press(&mut app, key(KeyCode::Char('e'))).await; // Dev
    press(&mut app, key(KeyCode::Char('e'))).await; // local.env
    assert_eq!(app.active_env, Some(1));
    assert_eq!(app.config.last_environment.as_deref(), Some("Dev"));
    assert_eq!(
        config_on_disk(&tmp).unwrap().last_environment.as_deref(),
        Some("Dev")
    );
}

#[tokio::test]
async fn remembering_only_updates_last_environment_in_the_file() {
    let (mut app, tmp) = test_app();
    // The file says dracula; the session runs with a CLI override.
    let on_disk = AppConfig {
        theme: "dracula".into(),
        ..app.config.clone()
    };
    on_disk.save_to(&tmp.path().join("config.toml")).unwrap();
    app.config.theme = "gruvbox".into();
    app.environments.push(Environment::new("Dev"));

    press(&mut app, key(KeyCode::Char('e'))).await;
    let saved = config_on_disk(&tmp).unwrap();
    assert_eq!(saved.last_environment.as_deref(), Some("Dev"));
    assert_eq!(saved.theme.as_str(), "dracula");
}

#[tokio::test]
async fn without_config_path_nothing_is_written() {
    let tmp = TempDir::new().unwrap();
    let config = AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    };
    let mut app = App::new(config).unwrap();
    assert!(app.config_path.is_none());
    app.environments.push(Environment::new("Dev"));
    press(&mut app, key(KeyCode::Char('e'))).await;
    assert_eq!(app.config.last_environment.as_deref(), Some("Dev"));
    assert!(!tmp.path().join("config.toml").exists());
}

#[test]
fn restore_prefers_last_environment_over_default() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.environments.push(Environment::new("Prod"));
    app.config.default_environment = Some("dev".into());
    app.config.last_environment = Some("Prod".into());
    app.restore_last_environment();
    assert_eq!(app.active_env, Some(1));
}

#[test]
fn restore_falls_back_to_default_when_last_is_missing() {
    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.config.default_environment = Some("dev".into());
    app.config.last_environment = Some("Gone".into());
    app.restore_last_environment();
    assert_eq!(app.active_env, Some(0));

    let (mut app, _tmp) = test_app();
    app.environments.push(Environment::new("Dev"));
    app.config.last_environment = Some("Gone".into());
    app.restore_last_environment();
    assert_eq!(app.active_env, None);
}

// ════════════════════════════════════════════════════════════════════════════
// Rename
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn renameenv_moves_the_file_and_updates_last_environment() {
    let (mut app, tmp) = test_app();
    add_saved_env(&mut app, "Dev");
    execute_command(&mut app, "env Dev").await.unwrap();

    execute_command(&mut app, "renameenv Development")
        .await
        .unwrap();
    assert_eq!(app.environments[0].name, "Development");
    assert!(!env_file(&tmp, "Dev").exists());
    assert!(env_file(&tmp, "Development").exists());
    assert_eq!(
        config_on_disk(&tmp).unwrap().last_environment.as_deref(),
        Some("Development")
    );
    let reloaded = CollectionsStore::new(app.config.collections_dir.clone())
        .unwrap()
        .load_all_environments()
        .unwrap();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].name, "Development");
}

#[tokio::test]
async fn renameenv_rejects_duplicates_and_requires_args() {
    let (mut app, tmp) = test_app();
    add_saved_env(&mut app, "Dev");
    add_saved_env(&mut app, "Prod");
    app.active_env = Some(0);

    execute_command(&mut app, "renameenv Prod").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
    assert_eq!(app.environments[0].name, "Dev");
    assert!(env_file(&tmp, "Dev").exists());

    execute_command(&mut app, "renameenv").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );

    app.active_env = None;
    execute_command(&mut app, "renameenv X").await.unwrap();
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Warning
    );
}

#[tokio::test]
async fn env_editor_r_renames_active_environment() {
    let (mut app, tmp) = test_app();
    add_saved_env(&mut app, "Dev");
    app.active_env = Some(0);

    execute_command(&mut app, "envedit").await.unwrap();
    press(&mut app, char_key('R')).await;
    assert!(draw(&mut app).contains("Rename environment"));
    // The field starts with the current name.
    for _ in 0.."Dev".len() {
        press(&mut app, key(KeyCode::Backspace)).await;
    }
    type_str(&mut app, "Local").await;
    press(&mut app, key(KeyCode::Enter)).await;

    assert_eq!(app.environments[0].name, "Local");
    assert!(!env_file(&tmp, "Dev").exists());
    assert!(env_file(&tmp, "Local").exists());
    assert_eq!(app.mode, AppMode::Modal(ModalKind::EnvironmentEdit));
    assert!(app.editor.env_form().is_none());
}

#[tokio::test]
async fn renaming_session_env_writes_no_file() {
    let (mut app, tmp) = test_app();
    let env = Environment::new("local.env");
    app.session_env_ids.insert(env.id);
    app.environments.push(env);
    app.active_env = Some(0);

    execute_command(&mut app, "renameenv local").await.unwrap();
    assert_eq!(app.environments[0].name, "local");
    assert!(!tmp.path().join("environments").exists());
    assert!(app.config.last_environment.is_none());
}

// ════════════════════════════════════════════════════════════════════════════
// Delete
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn delenv_asks_then_deletes_active_environment() {
    let (mut app, tmp) = test_app();
    add_saved_env(&mut app, "Dev");
    add_saved_env(&mut app, "Prod");
    execute_command(&mut app, "env Prod").await.unwrap();

    execute_command(&mut app, "delenv").await.unwrap();
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    assert!(matches!(
        app.pending_confirm,
        Some(ConfirmAction::DeleteEnvironment(_))
    ));
    press(&mut app, key(KeyCode::Char('y'))).await;

    assert_eq!(app.environments.len(), 1);
    assert_eq!(app.environments[0].name, "Dev");
    assert_eq!(app.active_env, None);
    assert!(!env_file(&tmp, "Prod").exists());
    assert!(env_file(&tmp, "Dev").exists());
    assert!(config_on_disk(&tmp).unwrap().last_environment.is_none());
}

#[tokio::test]
async fn delenv_by_name_keeps_active_env_pointing_at_same_env() {
    let (mut app, tmp) = test_app();
    add_saved_env(&mut app, "A");
    add_saved_env(&mut app, "B");
    add_saved_env(&mut app, "C");
    app.active_env = Some(2);

    execute_command(&mut app, "delenv A").await.unwrap();
    press(&mut app, key(KeyCode::Enter)).await;
    assert_eq!(app.environments.len(), 2);
    assert_eq!(app.active_env, Some(1));
    assert_eq!(app.active_environment().unwrap().name, "C");
    assert!(!env_file(&tmp, "A").exists());
}

#[tokio::test]
async fn delenv_cancel_and_unknown_name() {
    let (mut app, tmp) = test_app();
    add_saved_env(&mut app, "Dev");
    app.active_env = Some(0);

    execute_command(&mut app, "delenv").await.unwrap();
    press(&mut app, key(KeyCode::Char('n'))).await;
    assert_eq!(app.environments.len(), 1);
    assert!(env_file(&tmp, "Dev").exists());

    execute_command(&mut app, "delenv Nope").await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(
        app.notification.as_ref().unwrap().kind,
        NotificationKind::Error
    );
    assert_eq!(app.environments.len(), 1);
}

#[tokio::test]
async fn env_editor_d_deletes_after_confirmation() {
    let (mut app, tmp) = test_app();
    add_saved_env(&mut app, "Dev");
    app.active_env = Some(0);

    execute_command(&mut app, "envedit").await.unwrap();
    press(&mut app, char_key('D')).await;
    assert!(matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))));
    press(&mut app, key(KeyCode::Char('y'))).await;
    assert!(app.environments.is_empty());
    assert_eq!(app.active_env, None);
    assert!(!env_file(&tmp, "Dev").exists());
}

#[tokio::test]
async fn deleting_session_env_forgets_its_id() {
    let (mut app, _tmp) = test_app();
    let env = Environment::new("local.env");
    let id = env.id;
    app.session_env_ids.insert(id);
    app.environments.push(env);
    app.active_env = Some(0);

    execute_command(&mut app, "delenv").await.unwrap();
    press(&mut app, key(KeyCode::Char('y'))).await;
    assert!(app.environments.is_empty());
    assert!(!app.session_env_ids.contains(&id));
}
