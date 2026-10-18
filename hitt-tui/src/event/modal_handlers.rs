use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, ModalKind, NotificationKind};
use crate::ui::widgets::diff_viewer::{self, DiffSelectorState, DiffSource};

use super::actions::handle_confirm_modal;
use super::editors::handle_env_editor_modal;
use super::import_export::{execute_export, execute_import};
use super::is_text_input;
use super::persistence::{collection_name_taken, request_name_taken, save_all_collections};
use hitt_core::core::constants::HALF_PAGE_SCROLL;

pub(super) async fn handle_modal_mode(
    app: &mut App,
    key: KeyEvent,
    kind: &ModalKind,
) -> Result<()> {
    // ThemePicker needs special Esc handling to restore the previous theme.
    if *kind == ModalKind::ThemePicker {
        if key.code == KeyCode::Esc {
            if let Some(prev) = app.theme_before_preview.take() {
                app.theme = prev;
            }
            app.mode = AppMode::Normal;
            return Ok(());
        }
        handle_theme_picker_modal(app, key);
        return Ok(());
    }

    // The variables editor uses Esc to cancel an open field form first.
    if *kind == ModalKind::EnvironmentEdit {
        handle_env_editor_modal(app, key);
        return Ok(());
    }

    // The collection runner uses Esc to cancel a run before closing.
    if *kind == ModalKind::Runner {
        super::runner::handle_runner_modal(app, key);
        return Ok(());
    }

    // DiffSelector owns its Esc handling (state reset, two-phase navigation).
    if *kind == ModalKind::DiffSelector {
        handle_diff_selector_modal(app, key);
        return Ok(());
    }

    match key.code {
        KeyCode::Esc => {
            // Confirm dialogs may return to the modal that opened them.
            app.mode = app.confirm_return_mode.take().unwrap_or(AppMode::Normal);
        }
        _ => match kind {
            ModalKind::Search => handle_search_modal(app, key),
            ModalKind::RenameTab => handle_rename_modal(app, key),
            ModalKind::CollectionPicker => handle_collection_picker_modal(app, key),
            ModalKind::RenameCollection(idx) => {
                let idx = *idx;
                handle_rename_collection_modal(app, key, idx);
            }
            ModalKind::RenameRequest {
                coll_idx,
                request_id,
            } => {
                let coll_idx = *coll_idx;
                let request_id = *request_id;
                handle_rename_request_modal(app, key, coll_idx, request_id);
            }
            ModalKind::Import => handle_import_modal(app, key),
            ModalKind::Export => handle_export_modal(app, key),
            ModalKind::Help => match (key.modifiers, key.code) {
                (_, KeyCode::Char('q' | '?')) => app.mode = AppMode::Normal,
                (KeyModifiers::NONE, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                    app.help_scroll = app.help_scroll.saturating_add(1);
                }
                (KeyModifiers::NONE, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                    app.help_scroll = app.help_scroll.saturating_sub(1);
                }
                // Same page size as J / K in the panels (documented in the help).
                (KeyModifiers::SHIFT, KeyCode::Char('J'))
                | (KeyModifiers::CONTROL, KeyCode::Char('d'))
                | (_, KeyCode::PageDown) => {
                    app.help_scroll = app.help_scroll.saturating_add(HALF_PAGE_SCROLL);
                }
                (KeyModifiers::SHIFT, KeyCode::Char('K'))
                | (KeyModifiers::CONTROL, KeyCode::Char('u'))
                | (_, KeyCode::PageUp) => {
                    app.help_scroll = app.help_scroll.saturating_sub(HALF_PAGE_SCROLL);
                }
                (KeyModifiers::NONE, KeyCode::Char('g')) => {
                    app.help_scroll = 0;
                }
                (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                    app.help_scroll = usize::MAX / 2;
                }
                _ => {}
            },
            ModalKind::Confirm(_) => handle_confirm_modal(app, key),
            _ => {}
        },
    }
    Ok(())
}

fn handle_search_modal(app: &mut App, key: KeyEvent) {
    let last = app.search_results.len().saturating_sub(1);
    match (key.modifiers, key.code) {
        (_, KeyCode::Down | KeyCode::Tab) | (KeyModifiers::CONTROL, KeyCode::Char('n' | 'j')) => {
            app.search_selected = (app.search_selected + 1).min(last);
        }
        (_, KeyCode::Up | KeyCode::BackTab) | (KeyModifiers::CONTROL, KeyCode::Char('p' | 'k')) => {
            app.search_selected = app.search_selected.saturating_sub(1);
        }
        (m, KeyCode::Char(c)) if is_text_input(m) => {
            app.search_query.push(c);
            update_search_results(app);
        }
        (_, KeyCode::Backspace) => {
            app.search_query.pop();
            update_search_results(app);
        }
        (_, KeyCode::Enter) => {
            let selected = app.search_selected.min(last);
            open_search_result(app, selected);
        }
        _ => {}
    }
}

/// Open search result `idx` (switching to its tab if it is already open)
/// and close the search modal.
pub(super) fn open_search_result(app: &mut App, idx: usize) {
    if let Some(result) = app.search_results.get(idx) {
        let (coll_idx, request_id) = (result.collection_index, result.request_id);
        super::sidebar::open_collection_request(app, coll_idx, request_id);
    }
    app.mode = AppMode::Normal;
}

fn handle_rename_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) if is_text_input(key.modifiers) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.trim().to_string();
            let tab = app.active_tab();
            if tab
                .collection_index
                .is_some_and(|ci| request_name_taken(app, ci, &new_name, Some(tab.request.id)))
            {
                // Keep the dialog open so the name can be changed.
                app.notify(
                    format!("A request named '{new_name}' already exists in this collection"),
                    NotificationKind::Error,
                );
                return;
            }
            if !new_name.is_empty() {
                app.active_tab_mut().request.name = new_name;
                app.active_tab_mut().dirty = true;
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_collection_picker_modal(app: &mut App, key: KeyEvent) {
    if app.collections.is_empty() {
        app.mode = AppMode::Normal;
        return;
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let max = app.collections.len().saturating_sub(1);
            app.collection_picker_selected = (app.collection_picker_selected + 1).min(max);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.collection_picker_selected = app.collection_picker_selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            let coll_idx = app.collection_picker_selected;
            let req = app.active_tab().request.clone();
            app.collections[coll_idx].add_request(req);
            app.active_tab_mut().collection_index = Some(coll_idx);
            app.active_tab_mut().dirty = false;
            save_all_collections(app);
            let name = app.collections[coll_idx].name.clone();
            app.notify(format!("Saved to '{name}'"), NotificationKind::Success);
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_rename_collection_modal(app: &mut App, key: KeyEvent, coll_idx: usize) {
    match key.code {
        KeyCode::Char(c) if is_text_input(key.modifiers) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.trim().to_string();
            if collection_name_taken(app, &new_name, Some(coll_idx)) {
                // Keep the dialog open so the name can be changed.
                app.notify(
                    format!("Collection '{new_name}' already exists"),
                    NotificationKind::Error,
                );
                return;
            }
            let unchanged = app
                .collections
                .get(coll_idx)
                .is_some_and(|c| c.name == new_name);
            if !new_name.is_empty() && !unchanged {
                // Delete the old collection file before renaming
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Ok(store) = hitt_core::storage::collections_store::CollectionsStore::new(
                        app.config.collections_dir.clone(),
                    ) {
                        let _ = store.delete_collection(coll);
                    }
                }
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    coll.name.clone_from(&new_name);
                }
                save_all_collections(app);
                app.notify(
                    format!("Renamed to '{new_name}'"),
                    NotificationKind::Success,
                );
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_rename_request_modal(
    app: &mut App,
    key: KeyEvent,
    coll_idx: usize,
    request_id: uuid::Uuid,
) {
    match key.code {
        KeyCode::Char(c) if is_text_input(key.modifiers) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.trim().to_string();
            if request_name_taken(app, coll_idx, &new_name, Some(request_id)) {
                // Keep the dialog open so the name can be changed.
                app.notify(
                    format!("A request named '{new_name}' already exists in this collection"),
                    NotificationKind::Error,
                );
                return;
            }
            if !new_name.is_empty() {
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    if let Some(req) = coll.find_request_mut(&request_id) {
                        req.name.clone_from(&new_name);
                    }
                }
                // Keep open tabs of this request in sync (their title is the name).
                for tab in &mut app.tabs {
                    if tab.collection_index == Some(coll_idx) && tab.request.id == request_id {
                        tab.request.name.clone_from(&new_name);
                    }
                }
                save_all_collections(app);
                app.notify(
                    format!("Renamed to '{new_name}'"),
                    NotificationKind::Success,
                );
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_import_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) if is_text_input(key.modifiers) => {
            app.modal_input.push(c);
        }
        KeyCode::Backspace => {
            app.modal_input.pop();
        }
        KeyCode::Enter => {
            let path = app.modal_input.clone();
            app.mode = AppMode::Normal;
            if !path.is_empty() {
                execute_import(app, path.trim());
            }
            app.modal_input.clear();
        }
        _ => {}
    }
}

fn handle_export_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) if is_text_input(key.modifiers) => {
            app.modal_input.push(c);
        }
        KeyCode::Backspace => {
            app.modal_input.pop();
        }
        KeyCode::Enter => {
            let path = app.modal_input.clone();
            app.mode = AppMode::Normal;
            if !path.is_empty() {
                execute_export(app, path.trim());
            }
            app.modal_input.clear();
        }
        _ => {}
    }
}

fn handle_theme_picker_modal(app: &mut App, key: KeyEvent) {
    let themes = crate::ui::theme::AVAILABLE_THEMES;
    let max = themes.len().saturating_sub(1);

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.theme_picker_selected = (app.theme_picker_selected + 1).min(max);
            apply_theme_preview(app);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.theme_picker_selected = app.theme_picker_selected.saturating_sub(1);
            apply_theme_preview(app);
        }
        KeyCode::Enter => {
            let name = themes[app.theme_picker_selected];
            app.config.theme = name.into();
            app.persist_theme();
            app.theme_before_preview = None;
            app.mode = AppMode::Normal;
            app.notify(format!("Theme: {name}"), NotificationKind::Info);
        }
        _ => {}
    }
}

fn close_diff_selector(app: &mut App) {
    app.diff_state = DiffSelectorState::default();
    app.mode = AppMode::Normal;
}

fn handle_diff_selector_modal(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        close_diff_selector(app);
        return;
    }
    if app.diff_state.view.is_some() {
        handle_diff_view_keys(app, key);
    } else {
        handle_diff_pick_keys(app, key);
    }
}

fn handle_diff_pick_keys(app: &mut App, key: KeyEvent) {
    let sources: Vec<DiffSource> = diff_viewer::collect_candidates(app)
        .iter()
        .map(|c| c.source)
        .collect();
    if sources.len() < 2 {
        if matches!(key.code, KeyCode::Enter | KeyCode::Char('q')) {
            close_diff_selector(app);
        }
        return;
    }

    let state = &mut app.diff_state;
    // Candidates can change while the modal is open (e.g. a request finishes).
    state.marked.retain(|m| sources.contains(m));
    let max = sources.len() - 1;
    state.selected = state.selected.min(max);

    match key.code {
        KeyCode::Char('q') => close_diff_selector(app),
        KeyCode::Char('j') | KeyCode::Down => {
            state.selected = (state.selected + 1).min(max);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Char('g') | KeyCode::Home => state.selected = 0,
        KeyCode::Char('G') | KeyCode::End => state.selected = max,
        KeyCode::Char(' ') => toggle_diff_mark(state, sources[state.selected]),
        KeyCode::Enter => {
            if state.marked.len() < 2 {
                toggle_diff_mark(state, sources[state.selected]);
            }
            if state.marked.len() == 2 {
                app.diff_state.view = diff_viewer::build_view(app);
                app.diff_state.scroll = 0;
            }
        }
        _ => {}
    }
}

/// Toggles a mark; marking a third response drops the oldest mark.
fn toggle_diff_mark(state: &mut DiffSelectorState, source: DiffSource) {
    if let Some(pos) = state.marked.iter().position(|m| *m == source) {
        state.marked.remove(pos);
    } else {
        if state.marked.len() == 2 {
            state.marked.remove(0);
        }
        state.marked.push(source);
    }
}

fn handle_diff_view_keys(app: &mut App, key: KeyEvent) {
    let state = &mut app.diff_state;
    // The last page stays full (the renderer records the visible height).
    let max = state.max_scroll();

    match (key.modifiers, key.code) {
        (_, KeyCode::Char('q')) => close_diff_selector(app),
        (_, KeyCode::Backspace | KeyCode::Char('h')) => {
            // Back to the picker, keeping the marks so the user can adjust.
            state.view = None;
            state.scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Char('j')) | (_, KeyCode::Down) => {
            state.scroll = (state.scroll + 1).min(max);
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (_, KeyCode::Up) => {
            state.scroll = state.scroll.saturating_sub(1);
        }
        (KeyModifiers::SHIFT, KeyCode::Char('J')) | (_, KeyCode::PageDown) => {
            state.scroll = (state.scroll + HALF_PAGE_SCROLL).min(max);
        }
        (KeyModifiers::SHIFT, KeyCode::Char('K')) | (_, KeyCode::PageUp) => {
            state.scroll = state.scroll.saturating_sub(HALF_PAGE_SCROLL);
        }
        (_, KeyCode::Char('g') | KeyCode::Home) => state.scroll = 0,
        (_, KeyCode::Char('G') | KeyCode::End) => state.scroll = max,
        (_, KeyCode::Char('u')) => state.unified = !state.unified,
        _ => {}
    }
}

pub(super) fn apply_theme_preview(app: &mut App) {
    let name = crate::ui::theme::AVAILABLE_THEMES[app.theme_picker_selected];
    if let Ok(mut theme) = crate::ui::theme::Theme::load(name) {
        if let Some(ref colors) = app.config.colors {
            theme.apply_overrides(colors);
        }
        if let Some(ref borders) = app.config.borders {
            theme.apply_border_overrides(borders);
        }
        app.theme = theme;
    }
}

fn update_search_results(app: &mut App) {
    use fuzzy_matcher::skim::SkimMatcherV2;
    use fuzzy_matcher::FuzzyMatcher;

    // Case-insensitive: the default smart case made `USER` miss "user".
    let matcher = SkimMatcherV2::default().ignore_case();
    let query = &app.search_query;

    app.search_results.clear();
    app.search_selected = 0;

    if query.is_empty() {
        return;
    }

    for (coll_idx, coll) in app.collections.iter().enumerate() {
        for req in coll.all_requests() {
            let search_str = format!("{} {} {}", req.method, req.name, req.url);
            if matcher.fuzzy_match(&search_str, query).is_some() {
                app.search_results.push(crate::app::SearchResult {
                    name: req.name.clone(),
                    method: Some(req.method),
                    url: req.url.clone(),
                    collection_name: Some(coll.name.clone()),
                    request_id: req.id,
                    collection_index: coll_idx,
                });
            }
        }
    }
}
