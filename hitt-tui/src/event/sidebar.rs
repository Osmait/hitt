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
    collection_items: &[hitt_core::core::collection::CollectionItem],
    coll_idx: usize,
    expanded: &std::collections::HashSet<uuid::Uuid>,
    out: &mut Vec<SidebarItem>,
) {
    for item in collection_items {
        match item {
            hitt_core::core::collection::CollectionItem::Request(req) => {
                out.push(SidebarItem::Request {
                    coll_idx,
                    request_id: req.id,
                });
            }
            hitt_core::core::collection::CollectionItem::Folder { id, items, .. } => {
                out.push(SidebarItem::Folder { coll_idx, id: *id });
                if expanded.contains(id) {
                    flatten_sidebar_items(items, coll_idx, expanded, out);
                }
            }
        }
    }
}

/// Number of rows in the current sidebar section.
pub(super) fn sidebar_len(app: &App) -> usize {
    match app.sidebar_state.section {
        SidebarSection::Collections => build_sidebar_items(app).len(),
        SidebarSection::History => app.history.entries().len(),
        SidebarSection::Chains => app.collections.iter().map(|c| c.chains.len()).sum(),
    }
}

/// `(coll_idx, chain_idx)` of the row selected in the Chains section.
pub(super) fn selected_sidebar_chain(app: &App) -> Option<(usize, usize)> {
    if app.sidebar_state.section != SidebarSection::Chains {
        return None;
    }
    app.collections
        .iter()
        .enumerate()
        .flat_map(|(ci, coll)| (0..coll.chains.len()).map(move |i| (ci, i)))
        .nth(app.sidebar_state.selected)
}

/// Enter on the sidebar: toggle a collection / folder, open a request,
/// run a chain or reopen a history entry.
pub(super) fn handle_sidebar_action(app: &mut App) {
    match app.sidebar_state.section {
        SidebarSection::Chains => {
            if let Some((coll_idx, chain_idx)) = selected_sidebar_chain(app) {
                let chain = app.collections[coll_idx].chains[chain_idx].clone();
                app.selected_chain = Some(chain.id);
                start_chain_execution(app, chain, coll_idx);
            }
        }
        SidebarSection::History => {
            // Reopen the request snapshot + stored response without sending.
            app.open_history_entry(app.sidebar_state.selected);
        }
        SidebarSection::Collections => {
            let items = build_sidebar_items(app);
            match items.get(app.sidebar_state.selected) {
                Some(SidebarItem::Collection { id, .. } | SidebarItem::Folder { id, .. }) => {
                    let id = *id;
                    if !app.sidebar_state.expanded.remove(&id) {
                        app.sidebar_state.expanded.insert(id);
                    }
                }
                Some(SidebarItem::Request {
                    coll_idx,
                    request_id,
                }) => open_collection_request(app, *coll_idx, *request_id),
                None => {}
            }
        }
    }
}

/// `l` / Right on the sidebar: expand a collection / folder or open a request
/// (never collapses). Other sections behave like Enter.
pub(super) fn handle_sidebar_expand(app: &mut App) {
    if app.sidebar_state.section != SidebarSection::Collections {
        handle_sidebar_action(app);
        return;
    }
    let items = build_sidebar_items(app);
    match items.get(app.sidebar_state.selected) {
        Some(SidebarItem::Collection { id, .. } | SidebarItem::Folder { id, .. }) => {
            app.sidebar_state.expanded.insert(*id);
        }
        Some(SidebarItem::Request {
            coll_idx,
            request_id,
        }) => open_collection_request(app, *coll_idx, *request_id),
        None => {}
    }
}

/// Open a collection request, switching to its tab if it is already open.
pub(super) fn open_collection_request(app: &mut App, coll_idx: usize, request_id: uuid::Uuid) {
    if !app.focus_open_request(coll_idx, request_id) {
        let Some(req) = app
            .collections
            .get(coll_idx)
            .and_then(|c| c.find_request(&request_id))
        else {
            return;
        };
        let tab = crate::app::RequestTab::from_request(req.clone(), Some(coll_idx));
        app.tabs.push(tab);
        app.switch_tab(app.tabs.len() - 1);
    }
    app.focus = FocusArea::UrlBar;
}

/// `h` / Left on the sidebar: collapse an expanded collection / folder,
/// otherwise move the selection to the parent row.
pub(super) fn handle_sidebar_collapse(app: &mut App) {
    if app.sidebar_state.section != SidebarSection::Collections {
        return;
    }

    let items = build_sidebar_items(app);
    let selected = app.sidebar_state.selected;
    let Some(item) = items.get(selected) else {
        return;
    };
    let (coll_idx, child_id) = match item {
        SidebarItem::Collection { id, .. } => {
            app.sidebar_state.expanded.remove(id);
            return;
        }
        SidebarItem::Folder { coll_idx, id } => {
            if app.sidebar_state.expanded.remove(id) {
                return;
            }
            (*coll_idx, *id)
        }
        SidebarItem::Request {
            coll_idx,
            request_id,
        } => (*coll_idx, *request_id),
    };
    let parent = app
        .collections
        .get(coll_idx)
        .and_then(|c| parent_folder(&c.items, child_id));
    let target = items.iter().position(|it| match (it, parent) {
        (SidebarItem::Folder { id, .. }, Some(p)) => *id == p,
        (SidebarItem::Collection { coll_idx: ci, .. }, None) => *ci == coll_idx,
        _ => false,
    });
    if let Some(row) = target {
        app.sidebar_state.selected = row;
    }
}

/// Id of the folder directly containing item `child` (`None` = top level or
/// not found).
fn parent_folder(
    items: &[hitt_core::core::collection::CollectionItem],
    child: uuid::Uuid,
) -> Option<uuid::Uuid> {
    use hitt_core::core::collection::CollectionItem;
    for item in items {
        if let CollectionItem::Folder { id, items: sub, .. } = item {
            let direct = sub.iter().any(|it| match it {
                CollectionItem::Request(r) => r.id == child,
                CollectionItem::Folder { id, .. } => *id == child,
            });
            if direct {
                return Some(*id);
            }
            if let Some(found) = parent_folder(sub, child) {
                return Some(found);
            }
        }
    }
    None
}
