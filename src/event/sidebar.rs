use crate::app::{App, FocusArea, SidebarSection};

use super::chain::start_chain_execution;

/// Describes what a flat sidebar row represents.
#[derive(Debug, Clone)]
pub enum SidebarItem {
    /// A collection header row. `coll_idx` is the index into `app.collections`.
    Collection { coll_idx: usize, id: uuid::Uuid },
    /// A folder row inside a collection.
    Folder { coll_idx: usize, id: uuid::Uuid },
    /// A request row.
    Request {
        coll_idx: usize,
        request_id: uuid::Uuid,
    },
}

/// Builds the flat list of sidebar items exactly matching the render order.
pub fn build_sidebar_items(app: &App) -> Vec<SidebarItem> {
    let mut items = Vec::new();
    for (coll_idx, coll) in app.collections.iter().enumerate() {
        items.push(SidebarItem::Collection {
            coll_idx,
            id: coll.id,
        });
        if app.sidebar_state.expanded.contains(&coll.id) {
            flatten_sidebar_items(
                &coll.items,
                coll_idx,
                &app.sidebar_state.expanded,
                &mut items,
            );
        }
    }
    items
}

fn flatten_sidebar_items(
    collection_items: &[crate::core::collection::CollectionItem],
    coll_idx: usize,
    expanded: &std::collections::HashSet<uuid::Uuid>,
    out: &mut Vec<SidebarItem>,
) {
    for item in collection_items {
        match item {
            crate::core::collection::CollectionItem::Request(req) => {
                out.push(SidebarItem::Request {
                    coll_idx,
                    request_id: req.id,
                });
            }
            crate::core::collection::CollectionItem::Folder { id, items, .. } => {
                out.push(SidebarItem::Folder { coll_idx, id: *id });
                if expanded.contains(id) {
                    flatten_sidebar_items(items, coll_idx, expanded, out);
                }
            }
        }
    }
}

/// Toggle expand/collapse or open a request for the currently selected sidebar row.
pub(super) fn handle_sidebar_action(app: &mut App) {
    if app.sidebar_state.section == SidebarSection::Chains {
        // Find the selected chain across all collections
        let mut chain_idx = 0usize;
        for (coll_idx, coll) in app.collections.iter().enumerate() {
            for chain in &coll.chains {
                if chain_idx == app.sidebar_state.selected {
                    start_chain_execution(app, chain.clone(), coll_idx);
                    return;
                }
                chain_idx += 1;
            }
        }
        return;
    }

    if app.sidebar_state.section == SidebarSection::History {
        let entries = app.history.entries();
        if let Some(entry) = entries.get(app.sidebar_state.selected) {
            let request = crate::core::request::Request::new(
                format!("{} {}", entry.method, entry.short_url(40)),
                entry.method,
                &entry.url,
            );
            if let Some(req_id) = entry.request_id {
                for (ci, coll) in app.collections.iter().enumerate() {
                    if let Some(found) = coll.find_request(&req_id) {
                        let tab = crate::app::RequestTab::from_request(found.clone(), Some(ci));
                        app.tabs.push(tab);
                        app.active_tab = app.tabs.len() - 1;
                        app.focus = FocusArea::UrlBar;
                        return;
                    }
                }
            }
            let tab = crate::app::RequestTab::from_request(request, None);
            app.tabs.push(tab);
            app.active_tab = app.tabs.len() - 1;
            app.focus = FocusArea::UrlBar;
        }
        return;
    }

    if app.sidebar_state.section != SidebarSection::Collections {
        return;
    }

    let items = build_sidebar_items(app);
    let selected = app.sidebar_state.selected;

    if let Some(item) = items.get(selected) {
        match item {
            SidebarItem::Collection { id, .. } | SidebarItem::Folder { id, .. } => {
                // Toggle expand/collapse
                let id = *id;
                if app.sidebar_state.expanded.contains(&id) {
                    app.sidebar_state.expanded.remove(&id);
                } else {
                    app.sidebar_state.expanded.insert(id);
                }
            }
            SidebarItem::Request {
                coll_idx,
                request_id,
            } => {
                let coll_idx = *coll_idx;
                let request_id = *request_id;
                // Open the request in a new tab
                if let Some(coll) = app.collections.get(coll_idx) {
                    if let Some(req) = coll.find_request(&request_id) {
                        let tab = crate::app::RequestTab::from_request(req.clone(), Some(coll_idx));
                        app.tabs.push(tab);
                        app.active_tab = app.tabs.len() - 1;
                        app.focus = FocusArea::UrlBar;
                        app.response_scroll = 0;
                    }
                }
            }
        }
    }
}

/// Collapse the currently selected sidebar item (or its parent).
pub(super) fn handle_sidebar_collapse(app: &mut App) {
    if app.sidebar_state.section != SidebarSection::Collections {
        return;
    }

    let items = build_sidebar_items(app);
    let selected = app.sidebar_state.selected;

    if let Some(item) = items.get(selected) {
        match item {
            SidebarItem::Collection { id, .. } | SidebarItem::Folder { id, .. } => {
                // If expanded, collapse it. If already collapsed, do nothing.
                app.sidebar_state.expanded.remove(id);
            }
            SidebarItem::Request { .. } => {
                // Navigate up to the parent folder/collection
                if selected > 0 {
                    app.sidebar_state.selected = selected.saturating_sub(1);
                }
            }
        }
    }
}
