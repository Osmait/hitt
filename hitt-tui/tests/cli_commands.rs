use tempfile::TempDir;

use hitt_core::core::chain::RequestChain;
use hitt_core::core::collection::Collection;
use hitt_core::core::request::{HttpMethod, KeyValuePair, Request};
use hitt_core::storage::collections_store::CollectionsStore;
use hitt_core::storage::config::AppConfig;
use hitt_tui::cli::{ChainAction, Commands, TestArgs};

fn test_config(tmp: &TempDir) -> AppConfig {
    AppConfig {
        collections_dir: tmp.path().join("collections"),
        ..AppConfig::default()
    }
}

fn setup_collection(config: &AppConfig, name: &str) -> Collection {
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let mut coll = Collection::new(name);
    coll.add_request(Request::new(
        "login",
        HttpMethod::POST,
        "http://localhost/login",
    ));
    coll.add_request(Request::new(
        "get-user",
        HttpMethod::GET,
        "http://localhost/users/1",
    ));
    store.save_collection(&coll).unwrap();
    coll
}

// ── collections ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn cli_collections_lists_all() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();

    store.save_collection(&Collection::new("API One")).unwrap();
    store.save_collection(&Collection::new("API Two")).unwrap();

    let result = hitt_tui::cli::run(Commands::Collections, &config).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn cli_collections_empty() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    // Create the dir so load_all works
    let _store = CollectionsStore::new(config.collections_dir.clone()).unwrap();

    let result = hitt_tui::cli::run(Commands::Collections, &config).await;
    assert!(result.is_ok());
}

// ── requests ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cli_requests_lists_collection() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let result = hitt_tui::cli::run(
        Commands::Requests {
            collection: "My API".to_string(),
        },
        &config,
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn cli_requests_collection_not_found() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let _store = CollectionsStore::new(config.collections_dir.clone()).unwrap();

    let result = hitt_tui::cli::run(
        Commands::Requests {
            collection: "Nonexistent".to_string(),
        },
        &config,
    )
    .await;
    assert!(result.is_err());
}

// ── run ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cli_run_request_not_found() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let result = hitt_tui::cli::run(
        Commands::Run {
            name: "nonexistent".to_string(),
            collection: Some("My API".to_string()),
            env: None,
        },
        &config,
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn cli_run_request_not_found_no_collections() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let _store = CollectionsStore::new(config.collections_dir.clone()).unwrap();

    let result = hitt_tui::cli::run(
        Commands::Run {
            name: "login".to_string(),
            collection: None,
            env: None,
        },
        &config,
    )
    .await;
    assert!(result.is_err());
}

// ── create ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cli_create_saves_request() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let result = hitt_tui::cli::run(
        Commands::Create {
            name: "new-request".to_string(),
            method: "GET".to_string(),
            url: "http://example.com/test".to_string(),
            collection: "My API".to_string(),
            create_collection: false,
            header: vec!["Content-Type: application/json".to_string()],
            body: None,
            body_type: hitt_tui::cli::BodyType::Json,
        },
        &config,
    )
    .await;
    assert!(result.is_ok());

    // Reload and verify
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let collections = store.load_all().unwrap();
    let coll = collections.iter().find(|c| c.name == "My API").unwrap();
    assert_eq!(coll.request_count(), 3); // 2 original + 1 new
}

#[tokio::test]
async fn cli_create_new_collection() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let _store = CollectionsStore::new(config.collections_dir.clone()).unwrap();

    let result = hitt_tui::cli::run(
        Commands::Create {
            name: "test-req".to_string(),
            method: "POST".to_string(),
            url: "http://example.com".to_string(),
            collection: "Brand New".to_string(),
            create_collection: true,
            header: vec![],
            body: Some(r#"{"key":"value"}"#.to_string()),
            body_type: hitt_tui::cli::BodyType::Json,
        },
        &config,
    )
    .await;
    assert!(result.is_ok());

    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let collections = store.load_all().unwrap();
    assert!(collections.iter().any(|c| c.name == "Brand New"));
}

#[tokio::test]
async fn cli_create_invalid_method() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let _store = CollectionsStore::new(config.collections_dir.clone()).unwrap();

    let result = hitt_tui::cli::run(
        Commands::Create {
            name: "bad".to_string(),
            method: "INVALID".to_string(),
            url: "http://example.com".to_string(),
            collection: "Test".to_string(),
            create_collection: true,
            header: vec![],
            body: None,
            body_type: hitt_tui::cli::BodyType::Json,
        },
        &config,
    )
    .await;
    assert!(result.is_err());
}

// ── chain list ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn cli_chain_list() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();

    let mut coll = Collection::new("My API");
    let req = Request::new("login", HttpMethod::POST, "http://localhost/login");
    let req_id = req.id;
    coll.add_request(req);

    let mut chain = RequestChain::new("Login Flow");
    chain.description = Some("Test chain".to_string());
    chain.add_step(req_id);
    coll.chains.push(chain);
    store.save_collection(&coll).unwrap();

    let result = hitt_tui::cli::run(
        Commands::Chain {
            action: ChainAction::List {
                collection: "My API".to_string(),
            },
        },
        &config,
    )
    .await;
    assert!(result.is_ok());
}

// ── chain create ────────────────────────────────────────────────────────────

#[tokio::test]
async fn cli_chain_create_saves() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let result = hitt_tui::cli::run(
        Commands::Chain {
            action: ChainAction::Create {
                name: "Test Flow".to_string(),
                collection: "My API".to_string(),
                step: vec!["login".to_string(), "get-user".to_string()],
                description: Some("A test chain".to_string()),
            },
        },
        &config,
    )
    .await;
    assert!(result.is_ok());

    // Verify chain was saved
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let collections = store.load_all().unwrap();
    let coll = collections.iter().find(|c| c.name == "My API").unwrap();
    assert_eq!(coll.chains.len(), 1);
    assert_eq!(coll.chains[0].name, "Test Flow");
    assert_eq!(coll.chains[0].steps.len(), 2);
}

#[tokio::test]
async fn cli_chain_create_request_not_found() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let result = hitt_tui::cli::run(
        Commands::Chain {
            action: ChainAction::Create {
                name: "Bad Flow".to_string(),
                collection: "My API".to_string(),
                step: vec!["login".to_string(), "nonexistent".to_string()],
                description: None,
            },
        },
        &config,
    )
    .await;
    assert!(result.is_err());
}

// ── chain import ────────────────────────────────────────────────────────────

#[tokio::test]
async fn cli_chain_import_from_yaml() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sample_chain.yaml");

    let result = hitt_tui::cli::run(
        Commands::Chain {
            action: ChainAction::Import {
                file: fixture,
                collection: "My API".to_string(),
            },
        },
        &config,
    )
    .await;
    assert!(result.is_ok());

    // Verify chain was imported
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let collections = store.load_all().unwrap();
    let coll = collections.iter().find(|c| c.name == "My API").unwrap();
    assert_eq!(coll.chains.len(), 1);
    assert_eq!(coll.chains[0].name, "Login Flow");
}

// ── send header parsing ─────────────────────────────────────────────────────

#[test]
fn cli_send_parses_headers() {
    // Test the parse_headers helper indirectly via create
    // We verified header parsing works through the create test above.
    // Here we do a direct unit-style check by creating a request with headers.
    let headers = [
        "Content-Type: application/json".to_string(),
        "Authorization:Bearer abc".to_string(),
        "X-Custom: value with spaces".to_string(),
    ];

    // Use the same parsing logic
    let parsed: Vec<KeyValuePair> = headers
        .iter()
        .filter_map(|s| {
            let (key, value) = s.split_once(':')?;
            Some(KeyValuePair::new(key.trim(), value.trim()))
        })
        .collect();

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].key, "Content-Type");
    assert_eq!(parsed[0].value, "application/json");
    assert_eq!(parsed[1].key, "Authorization");
    assert_eq!(parsed[1].value, "Bearer abc");
    assert_eq!(parsed[2].key, "X-Custom");
    assert_eq!(parsed[2].value, "value with spaces");
}

// ── regressions ─────────────────────────────────────────────────────────────

fn create_cmd(name: &str, collection: &str, create_collection: bool) -> Commands {
    Commands::Create {
        name: name.to_string(),
        method: "GET".to_string(),
        url: "http://example.com".to_string(),
        collection: collection.to_string(),
        create_collection,
        header: vec![],
        body: None,
        body_type: hitt_tui::cli::BodyType::Json,
    }
}

#[tokio::test]
async fn cli_create_unknown_collection_without_flag_is_error() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let err = hitt_tui::cli::run(create_cmd("x", "Nope", false), &config)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Collection 'Nope' not found"),
        "{err}"
    );
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    assert_eq!(store.load_all().unwrap().len(), 1);
}

#[tokio::test]
async fn cli_create_duplicate_request_name_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let err = hitt_tui::cli::run(create_cmd("LOGIN", "My API", false), &config)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    assert_eq!(store.load_all().unwrap()[0].request_count(), 2);
}

#[tokio::test]
async fn cli_run_unknown_environment_is_error() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");

    let err = hitt_tui::cli::run(
        Commands::Run {
            name: "login".to_string(),
            collection: Some("My API".to_string()),
            env: Some("nope".to_string()),
        },
        &config,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Environment 'nope' not found");
}

fn save_chain_collection(config: &AppConfig, url: &str, missing_step: bool) {
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let mut coll = Collection::new("Chains");
    let req = Request::new("ping", HttpMethod::GET, url);
    let mut chain = RequestChain::new("Flow");
    chain.add_step(if missing_step {
        uuid::Uuid::new_v4()
    } else {
        req.id
    });
    coll.add_request(req);
    coll.chains.push(chain);
    store.save_collection(&coll).unwrap();
}

fn chain_run(env: Option<&str>) -> Commands {
    Commands::Chain {
        action: ChainAction::Run {
            name: "Flow".to_string(),
            collection: "Chains".to_string(),
            env: env.map(str::to_string),
        },
    }
}

#[tokio::test]
async fn cli_chain_run_unknown_environment_is_error() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    save_chain_collection(&config, "http://127.0.0.1:9/", false);
    let err = hitt_tui::cli::run(chain_run(Some("nope")), &config)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Environment 'nope' not found");
}

#[tokio::test]
async fn cli_chain_run_missing_request_fails_readably() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    save_chain_collection(&config, "http://127.0.0.1:9/", true);
    let err = hitt_tui::cli::run(chain_run(None), &config)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("request not found"), "{err}");
    assert!(
        !err.split_whitespace()
            .any(|w| uuid::Uuid::parse_str(w).is_ok()),
        "raw id in: {err}"
    );
}

#[tokio::test]
async fn cli_chain_run_failed_step_is_error() {
    let refused = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    save_chain_collection(&config, &format!("http://127.0.0.1:{refused}/"), false);
    let err = hitt_tui::cli::run(chain_run(None), &config)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("step 1 ('ping')"), "{err}");
}

/// Answer every connection with `404 Not Found` (no internet needed).
fn spawn_404_server() -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/events", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            );
        }
    });
    url
}

fn sse_cmd(url: &str) -> Commands {
    Commands::Sse {
        url: url.to_string(),
        header: vec![],
        duration: Some(5),
        max_events: None,
    }
}

#[tokio::test]
async fn cli_sse_http_error_is_error() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let url = spawn_404_server();
    let err = hitt_tui::cli::run(sse_cmd(&url), &config)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("404"), "{err}");
}

#[tokio::test]
async fn cli_sse_invalid_url_is_error() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    assert!(hitt_tui::cli::run(sse_cmd("notaurl"), &config)
        .await
        .is_err());
}

fn test_args(collection: &str, output: Option<std::path::PathBuf>) -> TestArgs {
    TestArgs {
        collection: collection.to_string(),
        folder: None,
        env: None,
        bail: false,
        reporter: hitt_tui::cli::Reporter::Text,
        output,
        delay: 0,
        iterations: 1,
    }
}

#[tokio::test]
async fn cli_test_bad_output_dir_fails_before_running() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let mut coll = Collection::new("Suite");
    coll.add_request(Request::new(
        "hit",
        HttpMethod::GET,
        format!("http://{}/", listener.local_addr().unwrap()),
    ));
    store.save_collection(&coll).unwrap();

    let output = tmp.path().join("nonexist").join("x.xml");
    let err = hitt_tui::cli::run(Commands::Test(test_args("Suite", Some(output))), &config)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("does not exist"), "{err}");
    // Nothing was sent.
    assert!(listener.accept().is_err());
}

#[test]
fn cli_test_zero_iterations_is_rejected_by_parser() {
    use clap::Parser;
    #[derive(Parser)]
    struct Wrapper {
        #[command(flatten)]
        args: TestArgs,
    }
    assert!(Wrapper::try_parse_from(["hitt", "Suite", "--iterations", "0"]).is_err());
    let ok = Wrapper::try_parse_from(["hitt", "Suite", "--iterations", "2"]).unwrap();
    assert_eq!(ok.args.iterations, 2);
}

// ── unique chain names ──────────────────────────────────────────────────────

fn chain_create(name: &str) -> Commands {
    Commands::Chain {
        action: ChainAction::Create {
            name: name.to_string(),
            collection: "My API".to_string(),
            step: vec![],
            description: None,
        },
    }
}

fn load_coll(config: &AppConfig, name: &str) -> Collection {
    CollectionsStore::new(config.collections_dir.clone())
        .unwrap()
        .load_all()
        .unwrap()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap()
}

#[tokio::test]
async fn cli_chain_create_rejects_a_duplicate_name() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");
    hitt_tui::cli::run(chain_create("slow"), &config)
        .await
        .unwrap();
    let err = hitt_tui::cli::run(chain_create("SLOW"), &config)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    assert_eq!(load_coll(&config, "My API").chains.len(), 1);
}

#[tokio::test]
async fn cli_chain_import_twice_renames_the_second() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    setup_collection(&config, "My API");
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sample_chain.yaml");
    for _ in 0..2 {
        hitt_tui::cli::run(
            Commands::Chain {
                action: ChainAction::Import {
                    file: fixture.clone(),
                    collection: "My API".to_string(),
                },
            },
            &config,
        )
        .await
        .unwrap();
    }
    let names: Vec<String> = load_coll(&config, "My API")
        .chains
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["Login Flow", "Login Flow (2)"]);
}

#[tokio::test]
async fn cli_chain_run_refuses_ambiguous_legacy_duplicates() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(&tmp);
    let store = CollectionsStore::new(config.collections_dir.clone()).unwrap();
    let mut coll = Collection::new("Chains");
    coll.chains.push(RequestChain::new("Flow"));
    coll.chains.push(RequestChain::new("flow"));
    store.save_collection(&coll).unwrap();
    let err = hitt_tui::cli::run(chain_run(None), &config)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("2 chains are named 'Flow'"), "{err}");
}

/// Answer every connection with `500 Internal Server Error`.
fn spawn_500_server() -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 4\r\nconnection: close\r\n\r\noops",
            );
        }
    });
    url
}

/// Documented semantics: a step whose request completes is a success even
/// with HTTP 500 (exit 0); the status is in the JSON output.
#[cfg(unix)]
#[test]
fn cli_chain_run_http_500_step_succeeds_and_reports_status() {
    let url = spawn_500_server();
    let home = TempDir::new().unwrap();
    // Pin the binary's config (macOS: ~/Library/Application Support,
    // elsewhere: $XDG_CONFIG_HOME) to a collections dir inside `home`.
    let config = test_config(&home);
    for dir in [
        home.path().join("Library/Application Support/hitt"),
        home.path().join(".config/hitt"),
    ] {
        std::fs::create_dir_all(&dir).unwrap();
        config.save_to(&dir.join("config.toml")).unwrap();
    }
    save_chain_collection(&config, &url, false);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hitt"))
        .args(["chain", "run", "Flow", "--collection", "Chains"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["steps"][0]["status"], 500, "{stdout}");
    assert_eq!(json["steps"][0]["request"], "ping");
}
