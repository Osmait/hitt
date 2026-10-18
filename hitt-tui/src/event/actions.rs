use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppMode, ConfirmAction, ModalKind, NotificationKind, SidebarSection};

use super::persistence::{remove_request_from_collection, save_all_collections};
use super::sidebar::{build_sidebar_items, selected_sidebar_chain, SidebarItem};

/// Save the active tab's request back to its collection, or open the collection picker.
pub(super) fn save_active_request(app: &mut App) {
    let coll_idx = app
        .active_tab()
        .collection_index
        .filter(|ci| *ci < app.collections.len());
    if let Some(coll_idx) = coll_idx {
        if app.active_tab().dirty {
            app.active_tab_mut().request.updated_at = chrono::Utc::now();
        }
        let req = app.active_tab().request.clone();
        let coll = &mut app.collections[coll_idx];
        let message = if let Some(existing) = coll.find_request_mut(&req.id) {
            *existing = req;
            "Saved".to_string()
        } else {
            // The request was deleted from the collection while the tab was open.
            coll.add_request(req);
            format!("Request was missing; re-added to '{}'", coll.name)
        };
        save_all_collections(app);
        app.active_tab_mut().dirty = false;
        app.notify(message, NotificationKind::Success);
    } else if app.collections.is_empty() {
        app.notify(
            "No collections. Use :newcol <name> first".into(),
            NotificationKind::Warning,
        );
    } else {
        app.collection_picker_selected = 0;
        app.mode = AppMode::Modal(ModalKind::CollectionPicker);
    }
}

/// Cycle to the next environment: env 1 → … → env N → No Env → env 1.
/// Shows a notification.
pub(super) fn cycle_environment(app: &mut App) {
    if app.environments.is_empty() {
        app.notify(
            "No environments configured".into(),
            NotificationKind::Warning,
        );
        return;
    }
    app.active_env = match app.active_env {
        Some(i) if i + 1 < app.environments.len() => Some(i + 1),
        Some(_) => None,
        None => Some(0),
    };
    app.remember_active_env();
    let name = app
        .active_environment()
        .map_or_else(|| "No Env".to_string(), |e| e.name.clone());
    app.notify(format!("Environment: {name}"), NotificationKind::Info);
}

/// True if the active tab's WebSocket is connected or connecting.
pub(super) fn ws_is_active(app: &App) -> bool {
    use hitt_core::protocols::websocket::WsStatus;
    app.active_tab()
        .ws_session
        .as_ref()
        .is_some_and(|s| matches!(s.status, WsStatus::Connected { .. } | WsStatus::Connecting))
}

/// True if the active tab's SSE stream is connected or connecting.
pub(super) fn sse_is_active(app: &App) -> bool {
    use hitt_core::protocols::sse::SseStatus;
    app.active_tab()
        .sse_session
        .as_ref()
        .is_some_and(|s| matches!(s.status, SseStatus::Connected | SseStatus::Connecting))
}

/// Disconnect the active tab's WebSocket session ("Not connected" if idle).
pub(super) fn disconnect_ws(app: &mut App) {
    if !ws_is_active(app) {
        app.notify("Not connected".into(), NotificationKind::Warning);
        return;
    }
    if let Some(ref tx) = app.active_tab().ws_cmd_sender {
        let _ = tx.send(hitt_core::protocols::websocket::WsCommand::Disconnect);
    }
    let tab = app.active_tab_mut();
    tab.ws_cmd_sender = None;
    if let Some(ref mut s) = tab.ws_session {
        s.status = hitt_core::protocols::websocket::WsStatus::Disconnected;
    }
    app.notify("Disconnected".into(), NotificationKind::Info);
}

/// Disconnect the active tab's SSE session ("Not connected" if idle).
pub(super) fn disconnect_sse(app: &mut App) {
    if !sse_is_active(app) {
        app.notify("Not connected".into(), NotificationKind::Warning);
        return;
    }
    if let Some(ref tx) = app.active_tab().sse_cmd_sender {
        let _ = tx.send(hitt_core::protocols::sse::SseCommand::Disconnect);
    }
    let tab = app.active_tab_mut();
    tab.sse_cmd_sender = None;
    if let Some(ref mut s) = tab.sse_session {
        s.status = hitt_core::protocols::sse::SseStatus::Disconnected;
    }
    app.notify("Disconnected".into(), NotificationKind::Info);
}

/// The request selected in the Collections sidebar, or a warning if there is none.
fn selected_sidebar_request(app: &mut App) -> Option<(usize, uuid::Uuid)> {
    if app.sidebar_state.section != SidebarSection::Collections {
        app.notify(
            "Select a request in the Collections section first".into(),
            NotificationKind::Warning,
        );
        return None;
    }
    let items = build_sidebar_items(app);
    if let Some(SidebarItem::Request {
        coll_idx,
        request_id,
    }) = items.get(app.sidebar_state.selected)
    {
        Some((*coll_idx, *request_id))
    } else {
        app.notify(
            "Select a request to delete".into(),
            NotificationKind::Warning,
        );
        None
    }
}

/// Ask for confirmation, then delete the request selected in the sidebar.
pub(super) fn request_delete_selected_request(app: &mut App) {
    if let Some((coll_idx, request_id)) = selected_sidebar_request(app) {
        let name = app.collections[coll_idx]
            .find_request(&request_id)
            .map(|r| r.name.clone())
            .unwrap_or_default();
        open_confirm(
            app,
            format!("Delete request '{name}'?"),
            ConfirmAction::DeleteRequest {
                coll_idx,
                request_id,
            },
        );
    }
}

fn delete_request(app: &mut App, coll_idx: usize, request_id: uuid::Uuid) {
    let Some(coll) = app.collections.get_mut(coll_idx) else {
        return;
    };
    remove_request_from_collection(&mut coll.items, &request_id);
    save_all_collections(app);
    app.notify("Request deleted".into(), NotificationKind::Success);
    let new_len = build_sidebar_items(app).len();
    if app.sidebar_state.selected >= new_len && new_len > 0 {
        app.sidebar_state.selected = new_len - 1;
    }
}

/// Sidebar `x`: delete the selected chain (Chains section) or request
/// (Collections section), after confirmation.
pub(super) fn request_delete_selected_item(app: &mut App) {
    match app.sidebar_state.section {
        SidebarSection::Chains => request_delete_selected_chain(app),
        SidebarSection::Collections => request_delete_selected_request(app),
        SidebarSection::History => app.notify(
            "Use :clearhistory to clear the history".into(),
            NotificationKind::Warning,
        ),
    }
}

/// Ask for confirmation, then delete the chain selected in the Chains sidebar.
fn request_delete_selected_chain(app: &mut App) {
    let Some((coll_idx, chain_idx)) = selected_sidebar_chain(app) else {
        app.notify("Select a chain to delete".into(), NotificationKind::Warning);
        return;
    };
    let coll = &app.collections[coll_idx];
    let chain = &coll.chains[chain_idx];
    let message = format!("Delete chain '{}'?", chain.name);
    let action = ConfirmAction::DeleteChain {
        coll_id: coll.id,
        chain_id: chain.id,
    };
    open_confirm(app, message, action);
}

fn delete_chain(app: &mut App, coll_id: uuid::Uuid, chain_id: uuid::Uuid) {
    let Some(coll) = app.collections.iter_mut().find(|c| c.id == coll_id) else {
        return;
    };
    let Some(pos) = coll.chains.iter().position(|c| c.id == chain_id) else {
        return;
    };
    let chain = coll.chains.remove(pos);
    if app.selected_chain == Some(chain_id) {
        app.selected_chain = None;
    }
    save_all_collections(app);
    let total: usize = app.collections.iter().map(|c| c.chains.len()).sum();
    app.sidebar_state.selected = app.sidebar_state.selected.min(total.saturating_sub(1));
    app.notify(
        format!("Deleted chain '{}'", chain.name),
        NotificationKind::Success,
    );
}

/// Ask for confirmation, then delete the collection at `pos` (and its file).
pub(super) fn request_delete_collection(app: &mut App, pos: usize) {
    let Some(coll) = app.collections.get(pos) else {
        return;
    };
    let message = format!(
        "Delete collection '{}' ({} requests)?",
        coll.name,
        coll.request_count()
    );
    let id = coll.id;
    open_confirm(app, message, ConfirmAction::DeleteCollection(id));
}

/// Close the active tab, asking first if it has unsaved changes. The last
/// tab can't be closed: it is reset to a blank request instead.
pub(super) fn request_close_tab(app: &mut App) {
    if app.tabs.len() <= 1 {
        let tab = app.active_tab();
        if tab.is_blank() {
            app.notify("Already a blank tab".into(), NotificationKind::Info);
        } else if tab.dirty {
            let msg = format!("'{}' has unsaved changes. Reset tab anyway?", tab.title());
            let id = tab.id;
            open_confirm(app, msg, ConfirmAction::ResetTab(id));
        } else {
            let id = tab.id;
            reset_tab(app, id);
        }
        return;
    }
    if app.active_tab().dirty {
        let tab = app.active_tab();
        let msg = format!("'{}' has unsaved changes. Close anyway?", tab.title());
        let id = tab.id;
        open_confirm(app, msg, ConfirmAction::CloseTab(id));
    } else {
        app.close_tab();
    }
}

/// Quit, asking first if any tab has unsaved changes.
pub(super) fn request_quit(app: &mut App) {
    let dirty = app.dirty_tab_count();
    if dirty == 0 {
        app.should_quit = true;
    } else {
        let plural = if dirty == 1 { "" } else { "s" };
        open_confirm(
            app,
            format!("{dirty} tab{plural} with unsaved changes. Quit anyway?"),
            ConfirmAction::Quit,
        );
    }
}

/// Show a yes/no dialog that runs `action` when accepted.
pub(super) fn open_confirm(app: &mut App, message: String, action: ConfirmAction) {
    app.pending_confirm = Some(action);
    app.confirm_return_mode = None;
    app.mode = AppMode::Modal(ModalKind::Confirm(message));
}

/// Keys for the `ModalKind::Confirm` dialog (Esc is handled by the modal dispatcher).
pub(super) fn handle_confirm_modal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
            app.mode = app.confirm_return_mode.take().unwrap_or(AppMode::Normal);
            if let Some(action) = app.pending_confirm.take() {
                run_confirm_action(app, action);
            }
        }
        KeyCode::Char('n' | 'N') => {
            app.mode = app.confirm_return_mode.take().unwrap_or(AppMode::Normal);
            app.pending_confirm = None;
        }
        _ => {}
    }
}

fn run_confirm_action(app: &mut App, action: ConfirmAction) {
    match action {
        ConfirmAction::DeleteRequest {
            coll_idx,
            request_id,
        } => delete_request(app, coll_idx, request_id),
        ConfirmAction::CloseTab(id) => {
            if let Some(idx) = app.tabs.iter().position(|t| t.id == id) {
                app.active_tab = idx;
                app.close_tab();
            }
        }
        ConfirmAction::Quit => app.should_quit = true,
        ConfirmAction::DeleteEnvironment(id) => super::environments::delete_environment(app, id),
        ConfirmAction::DeleteCollection(id) => {
            if let Some(pos) = app.collections.iter().position(|c| c.id == id) {
                remove_collection(app, pos);
            }
        }
        ConfirmAction::DeleteChain { coll_id, chain_id } => delete_chain(app, coll_id, chain_id),
        ConfirmAction::ResetTab(id) => reset_tab(app, id),
    }
}

/// Replace the tab with `id` by a blank request, stopping anything it runs.
fn reset_tab(app: &mut App, id: uuid::Uuid) {
    let Some(idx) = app.tabs.iter().position(|t| t.id == id) else {
        return;
    };
    let tab = &mut app.tabs[idx];
    tab.abort_pending_request();
    if let Some(tx) = tab.ws_cmd_sender.take() {
        let _ = tx.send(hitt_core::protocols::websocket::WsCommand::Disconnect);
    }
    if let Some(tx) = tab.sse_cmd_sender.take() {
        let _ = tx.send(hitt_core::protocols::sse::SseCommand::Disconnect);
    }
    app.tabs[idx] = crate::app::RequestTab::new();
    app.refresh_loading();
    if idx == app.active_tab {
        app.response_scroll = 0;
        app.request_scroll = 0;
    }
    app.notify(
        "Tab reset to a blank request".into(),
        NotificationKind::Info,
    );
}

/// Remove a collection at the given index, delete from disk, and fix tab references.
pub(super) fn remove_collection(app: &mut App, pos: usize) {
    let coll = app.collections.remove(pos);
    if let Ok(store) = hitt_core::storage::collections_store::CollectionsStore::new(
        app.config.collections_dir.clone(),
    ) {
        let _ = store.delete_collection(&coll);
    }
    for tab in &mut app.tabs {
        match tab.collection_index {
            Some(ci) if ci == pos => tab.collection_index = None,
            Some(ci) if ci > pos => tab.collection_index = Some(ci - 1),
            _ => {}
        }
    }
    app.notify(
        format!("Deleted collection '{}'", coll.name),
        NotificationKind::Success,
    );
}
