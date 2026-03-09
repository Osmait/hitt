use crate::app::{App, AppMode, ModalKind, NotificationKind, SidebarSection};

use super::persistence::{remove_request_from_collection, save_all_collections};
use super::sidebar::{build_sidebar_items, SidebarItem};

/// Save the active tab's request back to its collection, or open the collection picker.
pub(super) fn save_active_request(app: &mut App) {
    if let Some(coll_idx) = app.active_tab().collection_index {
        let req = app.active_tab().request.clone();
        if let Some(coll) = app.collections.get_mut(coll_idx) {
            if let Some(existing) = coll.find_request_mut(&req.id) {
                *existing = req;
            }
        }
        save_all_collections(app);
        app.active_tab_mut().dirty = false;
        app.notify("Saved".into(), NotificationKind::Success);
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

/// Cycle to the next environment (wraps around). Shows notification.
pub(super) fn cycle_environment(app: &mut App) {
    if app.environments.is_empty() {
        app.notify(
            "No environments configured".into(),
            NotificationKind::Warning,
        );
    } else {
        app.active_env = Some(match app.active_env {
            Some(i) => (i + 1) % app.environments.len(),
            None => 0,
        });
        let name = app
            .active_environment()
            .map(|e| e.name.clone())
            .unwrap_or_default();
        app.notify(format!("Environment: {name}"), NotificationKind::Info);
    }
}

/// Disconnect the active tab's WebSocket session.
pub(super) fn disconnect_ws(app: &mut App) {
    if let Some(ref tx) = app.active_tab().ws_cmd_sender {
        let _ = tx.send(crate::protocols::websocket::WsCommand::Disconnect);
    }
    let tab = app.active_tab_mut();
    tab.ws_cmd_sender = None;
    if let Some(ref mut s) = tab.ws_session {
        s.status = crate::protocols::websocket::WsStatus::Disconnected;
    }
    app.notify("Disconnected".into(), NotificationKind::Info);
}

/// Disconnect the active tab's SSE session.
pub(super) fn disconnect_sse(app: &mut App) {
    if let Some(ref tx) = app.active_tab().sse_cmd_sender {
        let _ = tx.send(crate::protocols::sse::SseCommand::Disconnect);
    }
    let tab = app.active_tab_mut();
    tab.sse_cmd_sender = None;
    if let Some(ref mut s) = tab.sse_session {
        s.status = crate::protocols::sse::SseStatus::Disconnected;
    }
    app.notify("Disconnected".into(), NotificationKind::Info);
}

/// Delete the request currently selected in the sidebar.
pub(super) fn delete_selected_request(app: &mut App) {
    if app.sidebar_state.section != SidebarSection::Collections {
        app.notify(
            "Switch to Collections sidebar first".into(),
            NotificationKind::Warning,
        );
        return;
    }
    let items = build_sidebar_items(app);
    if let Some(SidebarItem::Request {
        coll_idx,
        request_id,
    }) = items.get(app.sidebar_state.selected)
    {
        let coll_idx = *coll_idx;
        let request_id = *request_id;
        remove_request_from_collection(&mut app.collections[coll_idx].items, &request_id);
        save_all_collections(app);
        app.notify("Request deleted".into(), NotificationKind::Success);
        let new_len = build_sidebar_items(app).len();
        if app.sidebar_state.selected >= new_len && new_len > 0 {
            app.sidebar_state.selected = new_len - 1;
        }
    } else {
        app.notify(
            "Select a request to delete".into(),
            NotificationKind::Warning,
        );
    }
}

/// Remove a collection at the given index, delete from disk, and fix tab references.
pub(super) fn remove_collection(app: &mut App, pos: usize) {
    let coll = app.collections.remove(pos);
    if let Ok(store) = crate::storage::collections_store::CollectionsStore::new(
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
