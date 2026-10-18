use crate::app::App;

/// Saves all collections in memory to the collections directory.
pub(super) fn save_all_collections(app: &App) {
    if let Ok(store) = hitt_core::storage::collections_store::CollectionsStore::new(
        app.config.collections_dir.clone(),
    ) {
        for coll in &app.collections {
            if let Err(e) = store.save_collection(coll) {
                tracing::warn!("Failed to save collection '{}': {}", coll.name, e);
            }
        }
    }
}

/// Saves the environment at `idx` (no-op if the index is out of range).
pub(super) fn save_environment(app: &App, idx: usize) {
    let Some(env) = app.environments.get(idx) else {
        return;
    };
    match hitt_core::storage::collections_store::CollectionsStore::new(
        app.config.collections_dir.clone(),
    ) {
        Ok(store) => {
            if let Err(e) = store.save_environment(env) {
                tracing::warn!("Failed to save environment '{}': {}", env.name, e);
            }
        }
        Err(e) => tracing::warn!("Failed to init collections store: {}", e),
    }
}

/// Recursively removes a request with the given ID from a collection item tree.
pub(super) fn remove_request_from_collection(
    items: &mut Vec<hitt_core::core::collection::CollectionItem>,
    request_id: &uuid::Uuid,
) -> bool {
    if let Some(pos) = items.iter().position(|item| {
        matches!(item, hitt_core::core::collection::CollectionItem::Request(r) if r.id == *request_id)
    }) {
        items.remove(pos);
        return true;
    }
    for item in items.iter_mut() {
        if let hitt_core::core::collection::CollectionItem::Folder { items: sub, .. } = item {
            if remove_request_from_collection(sub, request_id) {
                return true;
            }
        }
    }
    false
}

/// True if a collection other than `except` is named `name` (case-insensitive:
/// collection files would collide on case-insensitive file systems).
pub(super) fn collection_name_taken(app: &App, name: &str, except: Option<usize>) -> bool {
    app.collections
        .iter()
        .enumerate()
        .any(|(i, c)| Some(i) != except && c.name.trim().eq_ignore_ascii_case(name.trim()))
}

/// True if an environment is named `name` (case-insensitive).
pub(super) fn environment_name_taken(app: &App, name: &str) -> bool {
    app.environments
        .iter()
        .any(|e| e.name.trim().eq_ignore_ascii_case(name.trim()))
}

/// `base`, or `base (2)`, `base (3)`… — the first name `taken` accepts. An
/// existing " (N)" suffix is replaced, not stacked (see
/// [`hitt_core::core::helpers::unique_name`]).
pub(super) fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    hitt_core::core::helpers::unique_name(base, taken)
}

/// True if collection `coll_idx` has a chain named `name` (case-insensitive).
pub(super) fn chain_name_taken(app: &App, coll_idx: usize, name: &str) -> bool {
    app.collections
        .get(coll_idx)
        .is_some_and(|c| hitt_core::core::helpers::find_chain_by_name(name, c).is_some())
}

/// True if a request other than `except` in collection `coll_idx` is named
/// `name` (case-insensitive, like `hitt create`).
pub(super) fn request_name_taken(
    app: &App,
    coll_idx: usize,
    name: &str,
    except: Option<uuid::Uuid>,
) -> bool {
    app.collections.get(coll_idx).is_some_and(|c| {
        c.iter_requests()
            .any(|r| Some(r.id) != except && r.name.trim().eq_ignore_ascii_case(name.trim()))
    })
}
