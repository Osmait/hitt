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

/// Remove the query string from `url`, keeping any `#fragment`.
///
/// Works on unresolved templates (e.g. `{{base_url}}/users?page=1`) since it
/// only splits on `?` / `#` rather than parsing the URL.
pub fn strip_query(url: &str) -> String {
    let (before_frag, frag) = split_fragment(url);
    let base = before_frag.split_once('?').map_or(before_frag, |(b, _)| b);
    match frag {
        Some(f) => format!("{base}#{f}"),
        None => base.to_string(),
    }
}

/// Append `params` to the query string of `url`, encoding keys and values with
/// `encode`. A param whose exact `key=value` pair is already present in the
/// URL's query is skipped, so params are never duplicated in the final URL.
pub fn append_query_params(
    url: &str,
    params: &[KeyValuePair],
    encode: impl Fn(&str) -> String,
) -> String {
    let (before_frag, frag) = split_fragment(url);
    let existing_query = before_frag.split_once('?').map(|(_, q)| q);
    let existing: Vec<(String, String)> = existing_query
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();

    let query: Vec<String> = params
        .iter()
        .filter(|p| !existing.iter().any(|(k, v)| *k == p.key && *v == p.value))
        .map(|p| format!("{}={}", encode(&p.key), encode(&p.value)))
        .collect();
    if query.is_empty() {
        return url.to_string();
    }

    let sep = match existing_query {
        None => "?",
        Some(q) if q.is_empty() || q.ends_with('&') => "",
        Some(_) => "&",
    };
    let mut out = format!("{before_frag}{sep}{}", query.join("&"));
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn split_fragment(url: &str) -> (&str, Option<&str>) {
    match url.split_once('#') {
        Some((before, frag)) => (before, Some(frag)),
        None => (url, None),
    }
}

/// `base` if `taken` accepts it, else the first free `"<stem> (N)"` with
/// N ≥ 2, where `<stem>` is `base` without a trailing `" (N)"` suffix — so a
/// taken "API (2)" becomes "API (3)", never "API (2) (2)" or "API (1)".
pub fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    let (stem, start) = match split_number_suffix(base) {
        Some((stem, n)) => (stem, n.saturating_add(1).max(2)),
        None => (base, 2),
    };
    (start..start.saturating_add(10_000))
        .chain(2..start)
        .map(|n| format!("{stem} ({n})"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_string())
}

/// `("API", 2)` for `"API (2)"`; `None` without a `" (N)"` suffix (or if
/// nothing would be left before it).
fn split_number_suffix(name: &str) -> Option<(&str, u32)> {
    let inner = name.strip_suffix(')')?;
    let (stem, digits) = inner.rsplit_once(" (")?;
    if stem.trim().is_empty() || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((stem, digits.parse().ok()?))
}

/// The chain named `name` in `collection` (case-insensitive).
pub fn find_chain_by_name<'a>(
    name: &str,
    collection: &'a Collection,
) -> Option<&'a super::chain::RequestChain> {
    collection
        .chains
        .iter()
        .find(|c| c.name.trim().eq_ignore_ascii_case(name.trim()))
}
