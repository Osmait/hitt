//! `hitt-core` — headless HTTP client library.
//!
//! UI-agnostic domain model, HTTP transport, collections, environments,
//! variables, chains, assertions, importers/exporters, and real-time
//! protocol adapters (WebSocket, SSE, gRPC).
//!
//! Both the ratatui TUI (`hitt-tui`) and the future macOS app
//! (`hitt-ffi` + SwiftUI) depend on this crate.
//!
//! ## Headless driver
//!
//! The [`Core`] struct owns runtime state (HTTP client, active environment,
//! in-memory collections) and exposes a single async method `handle(cmd)`
//! that consumes a [`CoreCommand`] and returns one or more [`CoreEvent`]s.
//! This is the same command/event pattern used by `sbql-core`.

pub mod core;
pub mod exporters;
pub mod importers;
pub mod postman;
pub mod protocols;
pub mod proxy;
pub mod storage;
pub mod testing;

mod handlers;

use crate::core::client::HttpClient;
use crate::core::collection::Collection;
use crate::core::environment::Environment;
use crate::core::history::{HistoryEntry, HistoryStore};
use crate::core::request::Request;
use crate::core::response::Response;
use crate::storage::collections_store::CollectionsStore;
use crate::storage::config::AppConfig;
use crate::testing::diff::DiffResult;
use crate::testing::load_test::LoadTestResult;

use std::collections::HashMap;
use uuid::Uuid;

/// Serializable result of running a chain (one entry per step + final vars).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainRunResult {
    pub steps: Vec<ChainStepResult>,
    pub final_vars: HashMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainStepResult {
    pub step_index: u32,
    pub status: ChainStepStatusTag,
    pub request_name: Option<String>,
    pub http_status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub extracted: HashMap<String, String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ChainStepStatusTag {
    Success,
    Failed,
    Skipped,
}

// ---------------------------------------------------------------------------
// Public command / event enums
// ---------------------------------------------------------------------------

/// Commands sent from a frontend (TUI, FFI, tests) into [`Core`].
#[derive(Debug, Clone)]
pub enum CoreCommand {
    // ---- Collections ---------------------------------------------------
    /// Load all collections from disk into memory.
    LoadCollections,
    /// Persist a collection (create or update by name).
    SaveCollection(Collection),
    /// Remove a collection by name.
    DeleteCollection(String),

    // ---- Environments --------------------------------------------------
    /// Load all environments from disk into memory.
    LoadEnvironments,
    /// Persist an environment (create or update by name).
    SaveEnvironment(Environment),
    /// Set the active environment by name. `None` clears it.
    SetActiveEnvironment(Option<String>),

    // ---- Config --------------------------------------------------------
    /// Return the current [`AppConfig`] snapshot.
    LoadConfig,
    /// Persist a new [`AppConfig`] and update the in-memory copy.
    SaveConfig(AppConfig),

    // ---- HTTP ----------------------------------------------------------
    /// Execute an HTTP request using the active environment for variable
    /// resolution. The response is returned as a [`CoreEvent::RequestCompleted`].
    SendRequest { request: Request },

    // ---- Testing -------------------------------------------------------
    /// Run a load test: `total` requests at `concurrency` parallelism.
    RunLoadTest {
        request: Request,
        total: u32,
        concurrency: u32,
        timeout_ms: u64,
    },
    /// Compute a unified text diff between two strings.
    RunDiff { left: String, right: String },
    /// Run a named chain from a collection.
    RunChain {
        collection_id: Uuid,
        chain_name: String,
    },

    // ---- History -------------------------------------------------------
    /// Return the current in-memory history entries.
    GetHistory,
    /// Erase all history entries.
    ClearHistory,
}

/// Events emitted by [`Core`] in response to commands.
#[derive(Debug, Clone)]
pub enum CoreEvent {
    Collections(Vec<Collection>),
    CollectionSaved(Collection),
    CollectionDeleted(String),

    Environments(Vec<Environment>),
    EnvironmentSaved(Environment),
    ActiveEnvironmentChanged(Option<String>),

    Config(AppConfig),
    ConfigSaved,

    /// `Box<Response>` keeps the enum small even though `Response` carries
    /// headers/body/timing.
    RequestCompleted(Box<Response>),

    LoadTestCompleted(Box<LoadTestResult>),
    DiffCompleted(Box<DiffResult>),
    ChainCompleted(Box<ChainRunResult>),

    HistoryLoaded(Vec<HistoryEntry>),
    HistoryCleared,

    Error(String),
}

// ---------------------------------------------------------------------------
// Core state
// ---------------------------------------------------------------------------

/// Headless runtime state driven by commands. Holds the HTTP client, the
/// currently loaded collections/environments, and the active environment name.
pub struct Core {
    pub client: HttpClient,
    pub config: AppConfig,
    pub active_environment: Option<String>,
    pub environments: Vec<Environment>,
    pub collections: Vec<Collection>,
    pub history: HistoryStore,
    pub(crate) collections_store: Option<CollectionsStore>,
}

impl Core {
    /// Build a new `Core`, loading the persistent config and initializing
    /// the collections store. Does not read collections/environments from
    /// disk — frontends should dispatch `LoadCollections` / `LoadEnvironments`
    /// to populate in-memory state when they want it.
    pub fn new() -> anyhow::Result<Self> {
        let config = AppConfig::load().unwrap_or_default();
        let client = HttpClient::from_config(&config)?;
        let collections_store = CollectionsStore::new(config.collections_dir.clone()).ok();
        let history = HistoryStore::new(config.history_limit);
        Ok(Self {
            client,
            config,
            active_environment: None,
            environments: Vec::new(),
            collections: Vec::new(),
            history,
            collections_store,
        })
    }

    /// Process a single [`CoreCommand`] and return the resulting events.
    pub async fn handle(&mut self, cmd: CoreCommand) -> Vec<CoreEvent> {
        match cmd {
            CoreCommand::LoadCollections => handlers::collection::load(self).await,
            CoreCommand::SaveCollection(c) => handlers::collection::save(self, c).await,
            CoreCommand::DeleteCollection(name) => handlers::collection::delete(self, name).await,

            CoreCommand::LoadEnvironments => handlers::environment::load(self).await,
            CoreCommand::SaveEnvironment(env) => handlers::environment::save(self, env).await,
            CoreCommand::SetActiveEnvironment(name) => {
                self.active_environment = name.clone();
                vec![CoreEvent::ActiveEnvironmentChanged(name)]
            }

            CoreCommand::LoadConfig => vec![CoreEvent::Config(self.config.clone())],
            CoreCommand::SaveConfig(cfg) => handlers::config::save(self, cfg).await,

            CoreCommand::SendRequest { request } => handlers::request::send(self, request).await,

            CoreCommand::RunLoadTest {
                request,
                total,
                concurrency,
                timeout_ms,
            } => {
                handlers::testing::run_load_test(self, request, total, concurrency, timeout_ms)
                    .await
            }
            CoreCommand::RunDiff { left, right } => handlers::testing::run_diff(left, right),
            CoreCommand::RunChain {
                collection_id,
                chain_name,
            } => handlers::testing::run_chain(self, collection_id, chain_name).await,

            CoreCommand::GetHistory => {
                vec![CoreEvent::HistoryLoaded(self.history.entries().to_vec())]
            }
            CoreCommand::ClearHistory => {
                self.history.clear();
                vec![CoreEvent::HistoryCleared]
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::request::HttpMethod;

    fn test_core(tmp: &tempfile::TempDir) -> Core {
        let mut config = AppConfig::default();
        config.collections_dir = tmp.path().to_path_buf();
        let client = HttpClient::new().unwrap();
        let store = CollectionsStore::new(config.collections_dir.clone()).ok();
        let history = HistoryStore::new(config.history_limit);
        Core {
            client,
            config,
            active_environment: None,
            environments: Vec::new(),
            collections: Vec::new(),
            history,
            collections_store: store,
        }
    }

    #[tokio::test]
    async fn load_collections_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let mut core = test_core(&tmp);
        let events = core.handle(CoreCommand::LoadCollections).await;
        assert!(matches!(events[..], [CoreEvent::Collections(ref v)] if v.is_empty()));
    }

    #[tokio::test]
    async fn save_then_load_collection_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut core = test_core(&tmp);
        let mut collection = Collection::new("MyCol");
        collection.description = Some("hello".into());

        let events = core
            .handle(CoreCommand::SaveCollection(collection.clone()))
            .await;
        assert!(matches!(events[..], [CoreEvent::CollectionSaved(_)]));
        assert_eq!(core.collections.len(), 1);

        // Simulate a fresh process by clearing in-memory state and loading.
        core.collections.clear();
        let events = core.handle(CoreCommand::LoadCollections).await;
        match &events[..] {
            [CoreEvent::Collections(v)] => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].name, "MyCol");
            }
            _ => panic!("Expected Collections event, got {events:?}"),
        }
    }

    #[tokio::test]
    async fn delete_collection_removes_from_disk_and_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let mut core = test_core(&tmp);
        let collection = Collection::new("Doomed");
        core.handle(CoreCommand::SaveCollection(collection)).await;
        assert_eq!(core.collections.len(), 1);

        let events = core
            .handle(CoreCommand::DeleteCollection("Doomed".into()))
            .await;
        assert!(matches!(events[..], [CoreEvent::CollectionDeleted(_)]));
        assert_eq!(core.collections.len(), 0);
    }

    #[tokio::test]
    async fn delete_collection_not_found_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mut core = test_core(&tmp);
        let events = core
            .handle(CoreCommand::DeleteCollection("ghost".into()))
            .await;
        assert!(matches!(events[..], [CoreEvent::Error(_)]));
    }

    #[tokio::test]
    async fn set_active_environment_updates_state() {
        let tmp = tempfile::tempdir().unwrap();
        let mut core = test_core(&tmp);
        let events = core
            .handle(CoreCommand::SetActiveEnvironment(Some("dev".into())))
            .await;
        assert!(
            matches!(events[..], [CoreEvent::ActiveEnvironmentChanged(Some(ref n))] if n == "dev")
        );
        assert_eq!(core.active_environment.as_deref(), Some("dev"));

        let events = core.handle(CoreCommand::SetActiveEnvironment(None)).await;
        assert!(matches!(
            events[..],
            [CoreEvent::ActiveEnvironmentChanged(None)]
        ));
        assert!(core.active_environment.is_none());
    }

    #[tokio::test]
    async fn load_config_returns_current() {
        let tmp = tempfile::tempdir().unwrap();
        let mut core = test_core(&tmp);
        let events = core.handle(CoreCommand::LoadConfig).await;
        assert!(matches!(events[..], [CoreEvent::Config(_)]));
    }

    #[tokio::test]
    async fn send_request_bad_host_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mut core = test_core(&tmp);
        let request = Request::new(
            "t",
            HttpMethod::GET,
            "http://this-host-should-not-exist.invalid/",
        );
        let events = core.handle(CoreCommand::SendRequest { request }).await;
        assert!(matches!(events[..], [CoreEvent::Error(_)]));
    }
}
