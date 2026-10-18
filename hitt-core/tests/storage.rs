use tempfile::TempDir;

use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::request::{HttpMethod, Request};
use hitt_core::storage::collections_store::CollectionsStore;
use hitt_core::storage::config::AppConfig;

// ── CollectionsStore ────────────────────────────────────────────────────────

#[test]
fn collections_store_creates_dir() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("collections");
    assert!(!dir.exists());
    let _store = CollectionsStore::new(dir.clone()).unwrap();
    assert!(dir.exists());
}

#[test]
fn collections_store_save_and_load_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();

    let mut collection = Collection::new("Test API");
    collection.add_request(Request::new(
        "Get Users",
        HttpMethod::GET,
        "https://api.example.com/users",
    ));

    let path = store.save_collection(&collection).unwrap();
    assert!(path.exists());

    let loaded = store.load_collection(&path).unwrap();
    assert_eq!(loaded.name, "Test API");
    assert_eq!(loaded.request_count(), 1);
}

#[test]
fn collections_store_delete() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();

    let collection = Collection::new("To Delete");
    store.save_collection(&collection).unwrap();
    store.delete_collection(&collection).unwrap();

    let all = store.load_all().unwrap();
    assert!(all.is_empty());
}

#[test]
fn collections_store_multiple_collections() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();

    store.save_collection(&Collection::new("API One")).unwrap();
    store.save_collection(&Collection::new("API Two")).unwrap();

    let all = store.load_all().unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn collections_store_load_all_empty() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();
    let all = store.load_all().unwrap();
    assert!(all.is_empty());
}

#[test]
fn collections_with_same_name_get_separate_files() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();

    let mut first = Collection::new("API");
    first.add_request(Request::new("one", HttpMethod::GET, "https://a.example/1"));
    let mut second = Collection::new("API");
    second.add_request(Request::new("two", HttpMethod::GET, "https://a.example/2"));

    let p1 = store.save_collection(&first).unwrap();
    let p2 = store.save_collection(&second).unwrap();
    assert_ne!(p1, p2);
    assert_eq!(p1.file_name().unwrap(), "API.json");
    assert_eq!(p2.file_name().unwrap(), "API_2.json");

    // Re-saving each keeps its own file; nothing is duplicated or lost.
    first.add_request(Request::new(
        "one-b",
        HttpMethod::GET,
        "https://a.example/1b",
    ));
    assert_eq!(store.save_collection(&first).unwrap(), p1);
    assert_eq!(store.save_collection(&second).unwrap(), p2);
    let all = store.load_all().unwrap();
    assert_eq!(all.len(), 2);
    let by_id = |id| all.iter().find(|c| c.id == id).unwrap();
    assert_eq!(by_id(first.id).request_count(), 2);
    assert_eq!(by_id(second.id).request_count(), 1);

    // Deleting one only deletes its own file.
    store.delete_collection(&first).unwrap();
    let all = store.load_all().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, second.id);
    assert!(p2.exists());
}

#[test]
fn names_that_sanitize_alike_do_not_collide() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();
    store.save_collection(&Collection::new("My API")).unwrap();
    store.save_collection(&Collection::new("My_API")).unwrap();
    let mut names: Vec<String> = store
        .load_all()
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["My API", "My_API"]);
}

#[test]
fn rename_after_delete_and_slot_reuse_keep_one_file_per_collection() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();
    let a = Collection::new("API");
    let mut b = Collection::new("API");
    store.save_collection(&a).unwrap();
    store.save_collection(&b).unwrap(); // API_2.json

    // `a` goes away: `b` moves into the free unsuffixed slot on next save.
    store.delete_collection(&a).unwrap();
    let path = store.save_collection(&b).unwrap();
    assert_eq!(path.file_name().unwrap(), "API.json");
    assert_eq!(store.load_all().unwrap().len(), 1);

    // Renaming (delete old + save new, as the TUI does) leaves one file.
    store.delete_collection(&b).unwrap();
    b.name = "Renamed".into();
    store.save_collection(&b).unwrap();
    let all = store.load_all().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Renamed");
}

#[test]
fn legacy_and_unreadable_files_are_never_overwritten() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("collections");
    let store = CollectionsStore::new(dir.clone()).unwrap();

    // A file written by an older version (plain name-derived path) still loads.
    let legacy = Collection::new("Legacy");
    std::fs::write(
        dir.join("Legacy.json"),
        serde_json::to_string_pretty(&legacy).unwrap(),
    )
    .unwrap();
    // A file that isn't a collection must not be clobbered.
    std::fs::write(dir.join("Notes.json"), "not json").unwrap();

    let loaded = store.load_all().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, legacy.id);
    assert_eq!(
        store.save_collection(&loaded[0]).unwrap(),
        dir.join("Legacy.json")
    );

    let notes = Collection::new("Notes");
    assert_eq!(
        store.save_collection(&notes).unwrap(),
        dir.join("Notes_2.json")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("Notes.json")).unwrap(),
        "not json"
    );
}

// ── Environment persistence ─────────────────────────────────────────────────

#[test]
fn environments_with_same_name_get_separate_files() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();

    let mut a = Environment::new("dev");
    a.add_variable("host", "a");
    let mut b = Environment::new("dev");
    b.add_variable("host", "b");
    let pa = store.save_environment(&a).unwrap();
    let pb = store.save_environment(&b).unwrap();
    assert_ne!(pa, pb);

    let all = store.load_all_environments().unwrap();
    assert_eq!(all.len(), 2);

    store.delete_environment(&b).unwrap();
    let all = store.load_all_environments().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].get("host"), Some("a"));
}

#[test]
fn environment_save_and_load_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();

    let mut env = Environment::new("Production");
    env.add_variable("host", "prod.example.com");
    env.add_variable("port", "443");
    env.add_secret("api_key", "secret123");

    let path = store.save_environment(&env).unwrap();
    assert!(path.exists());

    let loaded = store.load_environment(&path).unwrap();
    assert_eq!(loaded.name, "Production");
    assert_eq!(loaded.values.len(), 3);
    assert_eq!(loaded.get("host"), Some("prod.example.com"));
}

#[test]
fn environment_load_all_sorted_and_delete() {
    let tmp = TempDir::new().unwrap();
    let store = CollectionsStore::new(tmp.path().join("collections")).unwrap();

    assert!(store.load_all_environments().unwrap().is_empty());

    let mut staging = Environment::new("staging");
    staging.add_variable("host", "staging.example.com");
    let dev = Environment::new("Dev");
    store.save_environment(&staging).unwrap();
    store.save_environment(&dev).unwrap();

    let all = store.load_all_environments().unwrap();
    let names: Vec<&str> = all.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Dev", "staging"]);
    assert_eq!(all[1].get("host"), Some("staging.example.com"));

    // Saving again overwrites rather than duplicating
    staging.set("host", "new.example.com");
    store.save_environment(&staging).unwrap();
    let all = store.load_all_environments().unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].get("host"), Some("new.example.com"));

    store.delete_environment(&dev).unwrap();
    let all = store.load_all_environments().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "staging");

    // Deleting a missing environment is a no-op
    store.delete_environment(&dev).unwrap();
}

// ── AppConfig ───────────────────────────────────────────────────────────────

#[test]
fn app_config_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.theme.as_str(), "catppuccin");
    assert_eq!(config.history_limit, 1000);
    assert!(config.follow_redirects);
    assert!(config.verify_ssl);
    assert_eq!(config.timeout_ms, 30000);
    assert!(config.proxy.is_none());
    assert!(config.default_environment.is_none());
    assert!(config.editor.is_none());
}

#[test]
fn app_config_serialization_roundtrip() {
    let config = AppConfig {
        theme: "dracula".into(),
        default_environment: Some("prod".to_string()),
        last_environment: Some("staging".to_string()),
        history_limit: 500,
        persist_history: false,
        follow_redirects: false,
        verify_ssl: false,
        timeout_ms: 10000,
        proxy: Some("http://proxy:8080".to_string()),
        collections_dir: std::path::PathBuf::from("/tmp/test"),
        editor: Some("vim".to_string()),
        colors: None,
        borders: None,
    };

    let toml_str = toml::to_string_pretty(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.theme.as_str(), "dracula");
    assert_eq!(loaded.history_limit, 500);
    assert!(!loaded.follow_redirects);
    assert!(!loaded.verify_ssl);
    assert_eq!(loaded.timeout_ms, 10000);
    assert_eq!(loaded.proxy, Some("http://proxy:8080".to_string()));
    assert_eq!(loaded.last_environment.as_deref(), Some("staging"));
}

#[test]
fn app_config_without_last_environment_still_loads() {
    // A config.toml written before `last_environment` existed.
    let config = AppConfig {
        last_environment: Some("x".into()),
        ..AppConfig::default()
    };
    let toml_str = toml::to_string_pretty(&config)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("last_environment"))
        .collect::<Vec<_>>()
        .join("\n");
    let loaded: AppConfig = toml::from_str(&toml_str).unwrap();
    assert!(loaded.last_environment.is_none());
}

#[test]
fn app_config_save_to_writes_given_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("nested").join("config.toml");
    let config = AppConfig {
        last_environment: Some("dev".into()),
        ..AppConfig::default()
    };
    config.save_to(&path).unwrap();
    let loaded: AppConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(loaded.last_environment.as_deref(), Some("dev"));
}

#[test]
fn app_config_no_longer_writes_vim_mode_but_reads_old_files() {
    let toml_str = toml::to_string_pretty(&AppConfig::default()).unwrap();
    assert!(!toml_str.contains("vim_mode"), "{toml_str}");

    // A config.toml written by an older version still loads.
    let old = format!("{toml_str}\nvim_mode = true\n");
    let loaded: AppConfig = toml::from_str(&old).unwrap();
    assert_eq!(loaded.timeout_ms, AppConfig::default().timeout_ms);
}
