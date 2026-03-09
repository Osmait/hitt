use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, ModalKind, NotificationKind};

use super::import_export::{execute_export, execute_import};
use super::persistence::save_all_collections;

pub(super) async fn handle_modal_mode(
    app: &mut App,
    key: KeyEvent,
    kind: &ModalKind,
) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
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
                (_, KeyCode::Char('q')) => app.mode = AppMode::Normal,
                (KeyModifiers::NONE, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                    app.help_scroll = app.help_scroll.saturating_add(1);
                }
                (KeyModifiers::NONE, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                    app.help_scroll = app.help_scroll.saturating_sub(1);
                }
                (KeyModifiers::SHIFT, KeyCode::Char('J')) | (_, KeyCode::PageDown) => {
                    app.help_scroll = app.help_scroll.saturating_add(10);
                }
                (KeyModifiers::SHIFT, KeyCode::Char('K')) | (_, KeyCode::PageUp) => {
                    app.help_scroll = app.help_scroll.saturating_sub(10);
                }
                (KeyModifiers::NONE, KeyCode::Char('g')) => {
                    app.help_scroll = 0;
                }
                (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                    app.help_scroll = usize::MAX / 2;
                }
                _ => {}
            },
            _ => {}
        },
    }
    Ok(())
}

fn handle_search_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => {
            app.search_query.push(c);
            update_search_results(app);
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            update_search_results(app);
        }
        KeyCode::Enter => {
            // Select the first search result
            if let Some(result) = app.search_results.first() {
                let request_id = result.request_id;
                let coll_idx = result.collection_index;
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Some(req) = coll.find_request(&request_id) {
                        let tab = crate::app::RequestTab::from_request(req.clone(), Some(coll_idx));
                        app.tabs.push(tab);
                        app.active_tab = app.tabs.len() - 1;
                    }
                }
            }
            app.mode = AppMode::Normal;
        }
        _ => {}
    }
}

fn handle_rename_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.clone();
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
        KeyCode::Char(c) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.clone();
            if !new_name.is_empty() {
                // Delete the old collection file before renaming
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Ok(store) = crate::storage::collections_store::CollectionsStore::new(
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
        KeyCode::Char(c) => {
            app.rename_input.push(c);
        }
        KeyCode::Backspace => {
            app.rename_input.pop();
        }
        KeyCode::Enter => {
            let new_name = app.rename_input.clone();
            if !new_name.is_empty() {
                if let Some(coll) = app.collections.get_mut(coll_idx) {
                    if let Some(req) = coll.find_request_mut(&request_id) {
                        req.name.clone_from(&new_name);
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
        KeyCode::Char(c) => {
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
        KeyCode::Char(c) => {
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

fn update_search_results(app: &mut App) {
    use fuzzy_matcher::skim::SkimMatcherV2;
    use fuzzy_matcher::FuzzyMatcher;

    let matcher = SkimMatcherV2::default();
    let query = &app.search_query;

    app.search_results.clear();

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
