use anyhow::Result;

use super::collection::Collection;
use super::environment::Environment;
use super::request::{KeyValuePair, Request};
use crate::storage::collections_store::CollectionsStore;
use crate::storage::config::AppConfig;

/// Load all collections from the configured collections directory.
pub fn load_collections(config: &AppConfig) -> Result<Vec<Collection>> {
    let store = CollectionsStore::new(config.collections_dir.clone())?;
    store.load_all()
}

/// Find a collection by name (case-insensitive).
pub fn find_collection<'a>(name: &str, collections: &'a [Collection]) -> Result<&'a Collection> {
    collections
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("Collection '{name}' not found"))
}

/// Load a named environment from the collections store.
pub fn load_environment(name: &str, config: &AppConfig) -> Result<Option<Environment>> {
    let store = CollectionsStore::new(config.collections_dir.clone())?;
    let environments = store.load_environments()?;
    Ok(environments
        .into_iter()
        .find(|e| e.name.eq_ignore_ascii_case(name)))
}

/// Parse "Key: Value" header strings into KeyValuePair items.
pub fn parse_headers(header_strings: &[String]) -> Vec<KeyValuePair> {
    header_strings
        .iter()
        .filter_map(|s| {
            let (key, value) = s.split_once(':')?;
            Some(KeyValuePair::new(key.trim(), value.trim()))
        })
        .collect()
}

/// Find a request by name in a collection (case-insensitive).
pub fn find_request_by_name<'a>(name: &str, collection: &'a Collection) -> Option<&'a Request> {
    collection
        .iter_requests()
        .find(|r| r.name.eq_ignore_ascii_case(name))
}
