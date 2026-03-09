use crate::app::App;

/// Saves all collections in memory to the collections directory.
pub(super) fn save_all_collections(app: &App) {
    if let Ok(store) =
        crate::storage::collections_store::CollectionsStore::new(app.config.collections_dir.clone())
    {
        for coll in &app.collections {
            if let Err(e) = store.save_collection(coll) {
                tracing::warn!("Failed to save collection '{}': {}", coll.name, e);
            }
        }
    }
}

/// Recursively removes a request with the given ID from a collection item tree.
pub(super) fn remove_request_from_collection(
    items: &mut Vec<crate::core::collection::CollectionItem>,
    request_id: &uuid::Uuid,
) -> bool {
    if let Some(pos) = items.iter().position(|item| {
        matches!(item, crate::core::collection::CollectionItem::Request(r) if r.id == *request_id)
    }) {
        items.remove(pos);
        return true;
    }
    for item in items.iter_mut() {
        if let crate::core::collection::CollectionItem::Folder { items: sub, .. } = item {
            if remove_request_from_collection(sub, request_id) {
                return true;
            }
        }
    }
    false
}
