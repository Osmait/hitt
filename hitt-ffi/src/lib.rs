//! `hitt-ffi` — UniFFI bridge exposing [`hitt_core::Core`] to Swift.
//!
//! Complex types (`Request`, `Response`, `Collection`, `Environment`,
//! `AppConfig`) are marshalled as JSON strings to keep the FFI surface
//! small and avoid recursive `uniffi::Record` definitions for
//! folder-nested collections. Swift decodes/encodes them with its own
//! `Codable` models.
//!
//! The one exception is `FfiResponse`, kept as an explicit record so
//! Swift can show status/time without a JSON round-trip.

use std::sync::Arc;
use tokio::sync::Mutex;

use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::request::KeyValuePair;
use hitt_core::core::request::Request;
use hitt_core::storage::config::AppConfig;
use hitt_core::{Core, CoreCommand, CoreEvent};

mod streams;
use streams::StreamingSessions;

uniffi::setup_scaffolding!();

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HittFfiError {
    #[error("Core error: {msg}")]
    Core { msg: String },
    #[error("Invalid JSON: {msg}")]
    Json { msg: String },
    #[error("Engine not initialized")]
    NotInitialized,
}

impl HittFfiError {
    fn core(msg: impl Into<String>) -> Self {
        Self::Core { msg: msg.into() }
    }
    fn json(msg: impl Into<String>) -> Self {
        Self::Json { msg: msg.into() }
    }
}

// ---------------------------------------------------------------------------
// Explicit FFI records (kept small for ergonomics)
// ---------------------------------------------------------------------------

/// Fast-access projection of `hitt_core::core::response::Response`.
/// The full response (all headers, redirects, etc.) is also available
/// as JSON via `body_json` for Swift decoding.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiResponse {
    pub status: u16,
    pub status_text: String,
    pub body: String,
    pub size_bytes: u64,
    pub time_ms: u64,
    /// The complete `Response` serialized as JSON (headers, timing, cookies…).
    pub full_json: String,
}

// ---------------------------------------------------------------------------
// HittEngine — the FFI Object
// ---------------------------------------------------------------------------

#[derive(uniffi::Object)]
pub struct HittEngine {
    core: Arc<Mutex<Core>>,
    #[allow(dead_code)]
    runtime: Arc<tokio::runtime::Runtime>,
    streams: StreamingSessions,
}

impl Default for HittEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[uniffi::export]
impl HittEngine {
    /// Create a new engine. Loads persistent config from disk; does not
    /// load collections/environments — call `load_collections_json` /
    /// `load_environments_json` when you want them in memory.
    #[uniffi::constructor]
    pub fn new() -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
        );
        let core = Arc::new(Mutex::new(
            Core::new().expect("Failed to initialize hitt Core"),
        ));
        Self {
            core,
            runtime,
            streams: StreamingSessions::new(),
        }
    }

    // -------------------------------------------------------------------
    // Collections
    // -------------------------------------------------------------------

    /// Load all collections from disk. Returns a JSON array of
    /// `hitt_core::core::collection::Collection`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn load_collections_json(&self) -> Result<String, HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::LoadCollections).await;
        for ev in &events {
            if let CoreEvent::Error(msg) = ev {
                return Err(HittFfiError::core(msg));
            }
        }
        for ev in events {
            if let CoreEvent::Collections(list) = ev {
                return serde_json::to_string(&list).map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Ok("[]".into())
    }

    /// Save a collection. `collection_json` must deserialize into
    /// `hitt_core::core::collection::Collection`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn save_collection_json(
        &self,
        collection_json: String,
    ) -> Result<String, HittFfiError> {
        let collection: Collection = serde_json::from_str(&collection_json)
            .map_err(|e| HittFfiError::json(e.to_string()))?;
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::SaveCollection(collection)).await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::CollectionSaved(c) = ev {
                return serde_json::to_string(&c).map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Err(HittFfiError::core("no CollectionSaved event"))
    }

    /// Delete a collection by name.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn delete_collection(&self, name: String) -> Result<(), HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::DeleteCollection(name)).await;
        check_for_error(&events)
    }

    // -------------------------------------------------------------------
    // Environments
    // -------------------------------------------------------------------

    /// Load all environments from disk. Returns a JSON array of
    /// `hitt_core::core::environment::Environment`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn load_environments_json(&self) -> Result<String, HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::LoadEnvironments).await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::Environments(list) = ev {
                return serde_json::to_string(&list).map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Ok("[]".into())
    }

    /// Save an environment. `environment_json` must deserialize into
    /// `hitt_core::core::environment::Environment`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn save_environment_json(
        &self,
        environment_json: String,
    ) -> Result<String, HittFfiError> {
        let env: Environment = serde_json::from_str(&environment_json)
            .map_err(|e| HittFfiError::json(e.to_string()))?;
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::SaveEnvironment(env)).await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::EnvironmentSaved(e) = ev {
                return serde_json::to_string(&e)
                    .map_err(|err| HittFfiError::json(err.to_string()));
            }
        }
        Err(HittFfiError::core("no EnvironmentSaved event"))
    }

    /// Activate (or clear) the named environment.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn set_active_environment(&self, name: Option<String>) -> Result<(), HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::SetActiveEnvironment(name)).await;
        check_for_error(&events)
    }

    // -------------------------------------------------------------------
    // Config
    // -------------------------------------------------------------------

    /// Current `AppConfig` as JSON.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn load_config_json(&self) -> Result<String, HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::LoadConfig).await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::Config(c) = ev {
                return serde_json::to_string(&c).map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Err(HittFfiError::core("no Config event"))
    }

    /// Save a new `AppConfig`. `config_json` must deserialize into
    /// `hitt_core::storage::config::AppConfig`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn save_config_json(&self, config_json: String) -> Result<(), HittFfiError> {
        let cfg: AppConfig =
            serde_json::from_str(&config_json).map_err(|e| HittFfiError::json(e.to_string()))?;
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::SaveConfig(cfg)).await;
        check_for_error(&events)
    }

    // -------------------------------------------------------------------
    // HTTP
    // -------------------------------------------------------------------

    /// Execute an HTTP request. `request_json` must deserialize into
    /// `hitt_core::core::request::Request`. Returns an `FfiResponse`
    /// with the key fields pre-extracted plus `full_json` for the
    /// complete `Response` structure.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn send_request(&self, request_json: String) -> Result<FfiResponse, HittFfiError> {
        let request: Request =
            serde_json::from_str(&request_json).map_err(|e| HittFfiError::json(e.to_string()))?;
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::SendRequest { request }).await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::RequestCompleted(resp) = ev {
                use hitt_core::core::response::ResponseBody;
                let body_str = match &resp.body {
                    ResponseBody::Text(s)
                    | ResponseBody::Json(s)
                    | ResponseBody::Xml(s)
                    | ResponseBody::Html(s) => s.clone(),
                    ResponseBody::Binary(b) => format!("<binary {} bytes>", b.len()),
                    ResponseBody::Empty => String::new(),
                };
                let full_json =
                    serde_json::to_string(&*resp).map_err(|e| HittFfiError::json(e.to_string()))?;
                return Ok(FfiResponse {
                    status: resp.status,
                    status_text: resp.status_text.clone(),
                    body: body_str,
                    size_bytes: resp.size.total() as u64,
                    time_ms: resp.timing.total.as_millis() as u64,
                    full_json,
                });
            }
        }
        Err(HittFfiError::core("no RequestCompleted event"))
    }

    // -------------------------------------------------------------------
    // Testing
    // -------------------------------------------------------------------

    /// Run a load test. Returns a JSON-serialized `LoadTestResult`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn run_load_test(
        &self,
        request_json: String,
        total: u32,
        concurrency: u32,
        timeout_ms: u64,
    ) -> Result<String, HittFfiError> {
        let request: Request =
            serde_json::from_str(&request_json).map_err(|e| HittFfiError::json(e.to_string()))?;
        let mut core = self.core.lock().await;
        let events = core
            .handle(CoreCommand::RunLoadTest {
                request,
                total,
                concurrency,
                timeout_ms,
            })
            .await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::LoadTestCompleted(result) = ev {
                return serde_json::to_string(&*result)
                    .map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Err(HittFfiError::core("no LoadTestCompleted event"))
    }

    /// Compute a unified text diff between two strings. Returns JSON `DiffResult`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn run_diff(&self, left: String, right: String) -> Result<String, HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::RunDiff { left, right }).await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::DiffCompleted(result) = ev {
                return serde_json::to_string(&*result)
                    .map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Err(HittFfiError::core("no DiffCompleted event"))
    }

    // -------------------------------------------------------------------
    // WebSocket (polling API)
    // -------------------------------------------------------------------

    /// Open a WebSocket. Returns a session id (UUID string).
    /// `headers_json` must deserialize to `[KeyValuePair]` (empty array is fine).
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn ws_connect(
        &self,
        url: String,
        headers_json: String,
    ) -> Result<String, HittFfiError> {
        let headers: Vec<KeyValuePair> =
            serde_json::from_str(&headers_json).map_err(|e| HittFfiError::json(e.to_string()))?;
        self.streams
            .ws_connect(&url, &headers)
            .await
            .map_err(|e| HittFfiError::core(e.to_string()))
    }

    #[uniffi::method(async_runtime = "tokio")]
    pub async fn ws_send(&self, session_id: String, text: String) -> Result<(), HittFfiError> {
        self.streams
            .ws_send(&session_id, text)
            .await
            .map_err(|e| HittFfiError::core(e.to_string()))
    }

    /// Drain pending WS events since last poll as a JSON array of events.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn ws_poll(&self, session_id: String) -> Result<String, HittFfiError> {
        let events = self
            .streams
            .ws_poll(&session_id)
            .await
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        serde_json::to_string(&events).map_err(|e| HittFfiError::json(e.to_string()))
    }

    #[uniffi::method(async_runtime = "tokio")]
    pub async fn ws_disconnect(&self, session_id: String) -> Result<(), HittFfiError> {
        self.streams
            .ws_disconnect(&session_id)
            .await
            .map_err(|e| HittFfiError::core(e.to_string()))
    }

    // -------------------------------------------------------------------
    // SSE (polling API)
    // -------------------------------------------------------------------

    #[uniffi::method(async_runtime = "tokio")]
    pub async fn sse_connect(
        &self,
        url: String,
        headers_json: String,
    ) -> Result<String, HittFfiError> {
        let headers: Vec<KeyValuePair> =
            serde_json::from_str(&headers_json).map_err(|e| HittFfiError::json(e.to_string()))?;
        self.streams
            .sse_connect(&url, &headers)
            .await
            .map_err(|e| HittFfiError::core(e.to_string()))
    }

    #[uniffi::method(async_runtime = "tokio")]
    pub async fn sse_poll(&self, session_id: String) -> Result<String, HittFfiError> {
        let events = self
            .streams
            .sse_poll(&session_id)
            .await
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        serde_json::to_string(&events).map_err(|e| HittFfiError::json(e.to_string()))
    }

    #[uniffi::method(async_runtime = "tokio")]
    pub async fn sse_disconnect(&self, session_id: String) -> Result<(), HittFfiError> {
        self.streams
            .sse_disconnect(&session_id)
            .await
            .map_err(|e| HittFfiError::core(e.to_string()))
    }

    // -------------------------------------------------------------------
    // History
    // -------------------------------------------------------------------

    #[uniffi::method(async_runtime = "tokio")]
    pub async fn get_history_json(&self) -> Result<String, HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::GetHistory).await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::HistoryLoaded(entries) = ev {
                return serde_json::to_string(&entries)
                    .map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Ok("[]".into())
    }

    #[uniffi::method(async_runtime = "tokio")]
    pub async fn clear_history(&self) -> Result<(), HittFfiError> {
        let mut core = self.core.lock().await;
        let events = core.handle(CoreCommand::ClearHistory).await;
        check_for_error(&events)
    }

    // -------------------------------------------------------------------
    // Importers (all synchronous — parsers don't touch the network).
    // -------------------------------------------------------------------

    #[uniffi::method]
    pub fn import_postman_collection(&self, content: String) -> Result<String, HittFfiError> {
        let collection = hitt_core::postman::import::import_postman_collection(&content)
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        serde_json::to_string(&collection).map_err(|e| HittFfiError::json(e.to_string()))
    }

    #[uniffi::method]
    pub fn import_postman_environment(&self, content: String) -> Result<String, HittFfiError> {
        let env = hitt_core::postman::env_import::import_postman_environment(&content)
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        serde_json::to_string(&env).map_err(|e| HittFfiError::json(e.to_string()))
    }

    #[uniffi::method]
    pub fn import_openapi(&self, content: String) -> Result<String, HittFfiError> {
        let collection = hitt_core::importers::openapi::import_openapi(&content)
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        serde_json::to_string(&collection).map_err(|e| HittFfiError::json(e.to_string()))
    }

    #[uniffi::method]
    pub fn import_har(&self, content: String) -> Result<String, HittFfiError> {
        let collection = hitt_core::importers::har::import_har(&content)
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        serde_json::to_string(&collection).map_err(|e| HittFfiError::json(e.to_string()))
    }

    /// Parse a cURL command string. Returns JSON `Request`.
    #[uniffi::method]
    pub fn import_curl(&self, content: String) -> Result<String, HittFfiError> {
        let request = hitt_core::importers::curl::parse_curl(&content)
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        serde_json::to_string(&request).map_err(|e| HittFfiError::json(e.to_string()))
    }

    // -------------------------------------------------------------------
    // Exporters (synchronous)
    // -------------------------------------------------------------------

    /// Convert a `Request` to a cURL command string.
    #[uniffi::method]
    pub fn export_to_curl(&self, request_json: String) -> Result<String, HittFfiError> {
        let request: Request =
            serde_json::from_str(&request_json).map_err(|e| HittFfiError::json(e.to_string()))?;
        let resolver = hitt_core::core::variables::VariableResolver::new();
        Ok(hitt_core::exporters::curl::to_curl(&request, &resolver))
    }

    /// Generate Markdown API docs for a collection.
    #[uniffi::method]
    pub fn export_to_markdown(&self, collection_json: String) -> Result<String, HittFfiError> {
        let collection: Collection = serde_json::from_str(&collection_json)
            .map_err(|e| HittFfiError::json(e.to_string()))?;
        Ok(hitt_core::exporters::markdown_docs::generate_docs(
            &collection,
        ))
    }

    /// Export a collection as Postman Collection v2.1 JSON.
    #[uniffi::method]
    pub fn export_postman_collection(
        &self,
        collection_json: String,
    ) -> Result<String, HittFfiError> {
        let collection: Collection = serde_json::from_str(&collection_json)
            .map_err(|e| HittFfiError::json(e.to_string()))?;
        hitt_core::postman::export::export_postman_collection(&collection)
            .map_err(|e| HittFfiError::core(e.to_string()))
    }

    /// Export an environment as Postman Environment JSON.
    #[uniffi::method]
    pub fn export_postman_environment(&self, env_json: String) -> Result<String, HittFfiError> {
        let env: Environment =
            serde_json::from_str(&env_json).map_err(|e| HittFfiError::json(e.to_string()))?;
        hitt_core::postman::env_export::export_postman_environment(&env)
            .map_err(|e| HittFfiError::core(e.to_string()))
    }

    /// Parse a `.env`-format string into an `Environment` with the given name.
    #[uniffi::method]
    pub fn import_dotenv(&self, content: String, env_name: String) -> Result<String, HittFfiError> {
        let vars = hitt_core::importers::dotenv::parse_dotenv(&content)
            .map_err(|e| HittFfiError::core(e.to_string()))?;
        let mut env = hitt_core::core::environment::Environment::new(env_name);
        for (k, v) in vars {
            env.add_variable(k, v);
        }
        serde_json::to_string(&env).map_err(|e| HittFfiError::json(e.to_string()))
    }

    /// Run a named chain from a collection. Returns JSON `ChainRunResult`.
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn run_chain(
        &self,
        collection_id: String,
        chain_name: String,
    ) -> Result<String, HittFfiError> {
        let uuid = uuid::Uuid::parse_str(&collection_id)
            .map_err(|e| HittFfiError::json(format!("Invalid UUID: {e}")))?;
        let mut core = self.core.lock().await;
        let events = core
            .handle(CoreCommand::RunChain {
                collection_id: uuid,
                chain_name,
            })
            .await;
        check_for_error(&events)?;
        for ev in events {
            if let CoreEvent::ChainCompleted(result) = ev {
                return serde_json::to_string(&*result)
                    .map_err(|e| HittFfiError::json(e.to_string()));
            }
        }
        Err(HittFfiError::core("no ChainCompleted event"))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn check_for_error(events: &[CoreEvent]) -> Result<(), HittFfiError> {
    for ev in events {
        if let CoreEvent::Error(msg) = ev {
            return Err(HittFfiError::core(msg));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_new_does_not_panic() {
        let _engine = HittEngine::new();
    }

    #[tokio::test]
    async fn load_collections_on_fresh_engine() {
        let engine = HittEngine::new();
        let json = engine.load_collections_json().await.unwrap();
        // Should be valid JSON (array, possibly empty)
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        // Drop engine outside the async context so tokio runtime cleans up.
        tokio::task::spawn_blocking(move || drop(engine))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn send_request_invalid_json_returns_error() {
        let engine = HittEngine::new();
        let err = engine.send_request("not json".into()).await.unwrap_err();
        assert!(matches!(err, HittFfiError::Json { .. }));
        tokio::task::spawn_blocking(move || drop(engine))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn set_active_environment_roundtrip() {
        let engine = HittEngine::new();
        engine
            .set_active_environment(Some("dev".into()))
            .await
            .unwrap();
        engine.set_active_environment(None).await.unwrap();
        tokio::task::spawn_blocking(move || drop(engine))
            .await
            .unwrap();
    }
}
