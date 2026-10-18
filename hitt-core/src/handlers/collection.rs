use crate::core::collection::Collection;
use crate::{Core, CoreEvent};

pub(crate) async fn load(core: &mut Core) -> Vec<CoreEvent> {
    let store = match core.collections_store.as_ref() {
        Some(s) => s,
        None => return vec![CoreEvent::Error("Collections store not initialized".into())],
    };
    match store.load_all() {
        Ok(list) => {
            core.collections = list.clone();
            vec![CoreEvent::Collections(list)]
        }
        Err(e) => vec![CoreEvent::Error(e.to_string())],
    }
}

pub(crate) async fn save(core: &mut Core, collection: Collection) -> Vec<CoreEvent> {
    let store = match core.collections_store.as_ref() {
        Some(s) => s,
        None => return vec![CoreEvent::Error("Collections store not initialized".into())],
    };
    if let Err(e) = store.save_collection(&collection) {
        return vec![CoreEvent::Error(e.to_string())];
    }
    match core
        .collections
        .iter()
        .position(|c| c.name == collection.name)
    {
        Some(idx) => core.collections[idx] = collection.clone(),
        None => core.collections.push(collection.clone()),
    }
    vec![CoreEvent::CollectionSaved(collection)]
}

pub(crate) async fn delete(core: &mut Core, name: String) -> Vec<CoreEvent> {
    let store = match core.collections_store.as_ref() {
        Some(s) => s,
        None => return vec![CoreEvent::Error("Collections store not initialized".into())],
    };
    let Some(idx) = core.collections.iter().position(|c| c.name == name) else {
        return vec![CoreEvent::Error(format!("Collection '{name}' not found"))];
    };
    let collection = core.collections.remove(idx);
    if let Err(e) = store.delete_collection(&collection) {
        return vec![CoreEvent::Error(e.to_string())];
    }
    vec![CoreEvent::CollectionDeleted(name)]
}
