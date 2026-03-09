use tempfile::TempDir;

use hitt::core::collection::Collection;
use hitt::core::environment::Environment;
use hitt::core::request::{HttpMethod, Request};
use hitt::storage::collections_store::CollectionsStore;
use hitt::storage::config::AppConfig;

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
    collection.add_request(Request::new("Get Users", HttpMethod::GET, "https://api.example.com/users"));

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

    store
        .save_collection(&Collection::new("API One"))
        .unwrap();
    store
        .save_collection(&Collection::new("API Two"))
        .unwrap();

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

// ── Environment persistence ─────────────────────────────────────────────────

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

// ── AppConfig ───────────────────────────────────────────────────────────────

#[test]
fn app_config_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.theme, "catppuccin");
    assert_eq!(config.history_limit, 1000);
    assert!(config.follow_redirects);
    assert!(config.verify_ssl);
    assert_eq!(config.timeout_ms, 30000);
    assert!(config.proxy.is_none());
    assert!(config.vim_mode);
    assert!(config.default_environment.is_none());
    assert!(config.editor.is_none());
}

#[test]
fn app_config_serialization_roundtrip() {
    let config = AppConfig {
        theme: "dracula".to_string(),
        default_environment: Some("prod".to_string()),
        history_limit: 500,
        follow_redirects: false,
        verify_ssl: false,
        timeout_ms: 10000,
        proxy: Some("http://proxy:8080".to_string()),
        collections_dir: std::path::PathBuf::from("/tmp/test"),
        editor: Some("vim".to_string()),
        vim_mode: false,
    };

    let toml_str = toml::to_string_pretty(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(loaded.theme, "dracula");
    assert_eq!(loaded.history_limit, 500);
    assert!(!loaded.follow_redirects);
    assert!(!loaded.verify_ssl);
    assert_eq!(loaded.timeout_ms, 10000);
    assert_eq!(loaded.proxy, Some("http://proxy:8080".to_string()));
}
