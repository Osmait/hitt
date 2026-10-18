use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use hitt_core::core::history::HistoryEntry;
use hitt_core::core::request::HttpMethod;
use hitt_core::storage::config::AppConfig;
use hitt_tui::app::{App, AppMode, ModalKind};
use hitt_tui::event::{handle_event, AppEvent};
use hitt_tui::ui::widgets::diff_viewer::collect_candidates;

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

fn add_history(app: &mut App, url: &str, body: &str) {
    let mut entry = HistoryEntry::new(HttpMethod::GET, url);
    entry.status = Some(200);
    entry.response_body = Some(body.to_string());
    app.history.add(entry);
}

fn open_diff(app: &mut App) {
    app.mode = AppMode::Modal(ModalKind::DiffSelector);
}

#[tokio::test]
async fn diff_empty_state_closes_on_esc() {
    let (mut app, _tmp) = test_app();
    add_history(&mut app, "http://a", "one");
    open_diff(&mut app);
    assert_eq!(collect_candidates(&app).len(), 1);

    // Marking does nothing with fewer than two candidates.
    handle_event(&mut app, key(KeyCode::Char(' ')))
        .await
        .unwrap();
    assert!(app.diff_state.marked.is_empty());

    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
}

#[tokio::test]
async fn diff_pick_two_and_show_diff() {
    let (mut app, _tmp) = test_app();
    add_history(&mut app, "http://a", "line1\nline2\n");
    add_history(&mut app, "http://b", "line1\nchanged\nline3\n");
    open_diff(&mut app);

    // Space marks the first, j moves, Enter marks the second and opens the diff.
    handle_event(&mut app, key(KeyCode::Char(' ')))
        .await
        .unwrap();
    handle_event(&mut app, key(KeyCode::Char('j')))
        .await
        .unwrap();
    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();

    let view = app.diff_state.view.as_ref().expect("diff view shown");
    // Left = newest history entry (b), right = older one (a).
    assert_eq!(view.result.additions, 1);
    assert_eq!(view.result.deletions, 2);
    assert_eq!(view.result.unchanged, 1);

    // Scrolling is clamped to the number of diff lines.
    for _ in 0..20 {
        handle_event(&mut app, key(KeyCode::Char('j')))
            .await
            .unwrap();
    }
    let max = app.diff_state.view.as_ref().unwrap().result.lines.len() - 1;
    assert_eq!(app.diff_state.scroll, max);

    // Toggle unified mode.
    handle_event(&mut app, key(KeyCode::Char('u')))
        .await
        .unwrap();
    assert!(app.diff_state.unified);

    // Backspace returns to the picker, keeping the marks.
    handle_event(&mut app, key(KeyCode::Backspace))
        .await
        .unwrap();
    assert!(app.diff_state.view.is_none());
    assert_eq!(app.diff_state.marked.len(), 2);
    assert_eq!(app.mode, AppMode::Modal(ModalKind::DiffSelector));

    // Esc closes and resets state.
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.diff_state.marked.is_empty());
}

#[tokio::test]
async fn diff_space_toggles_and_limits_to_two_marks() {
    let (mut app, _tmp) = test_app();
    add_history(&mut app, "http://a", "a");
    add_history(&mut app, "http://b", "b");
    add_history(&mut app, "http://c", "c");
    open_diff(&mut app);

    handle_event(&mut app, key(KeyCode::Char(' ')))
        .await
        .unwrap();
    handle_event(&mut app, key(KeyCode::Char(' ')))
        .await
        .unwrap();
    assert!(app.diff_state.marked.is_empty(), "space unmarks");

    for _ in 0..3 {
        handle_event(&mut app, key(KeyCode::Char(' ')))
            .await
            .unwrap();
        handle_event(&mut app, key(KeyCode::Char('j')))
            .await
            .unwrap();
    }
    assert_eq!(app.diff_state.marked.len(), 2);
    assert!(app.diff_state.view.is_none(), "space never opens the diff");

    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();
    assert!(app.diff_state.view.is_some());
}
