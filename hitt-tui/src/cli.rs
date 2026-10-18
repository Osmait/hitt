use anyhow::{bail, Result};
use clap::{Subcommand, ValueEnum};
use serde_json::json;
use std::path::PathBuf;

use hitt_core::core::auth::AuthConfig;
use hitt_core::core::chain::RequestChain;
use hitt_core::core::chain_executor::{self, StepOutcome};
use hitt_core::core::client::HttpClient;
use hitt_core::core::collection::Collection;
use hitt_core::core::environment::Environment;
use hitt_core::core::helpers::{
    find_chain_by_name, find_collection, find_request_by_name, load_collections, load_environment,
    parse_headers, unique_name,
};
use hitt_core::core::request::{HttpMethod, Request, RequestBody};
use hitt_core::core::response::Response;
use hitt_core::core::variables::VariableResolver;
use hitt_core::storage::collections_store::CollectionsStore;
use hitt_core::storage::config::AppConfig;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BodyType {
    Json,
    Raw,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List all saved collections
    Collections,

    /// List requests in a collection
    Requests {
        #[arg(short, long)]
        collection: String,
    },

    /// Execute a saved request by name
    Run {
        name: String,
        #[arg(short, long)]
        collection: Option<String>,
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Send an ad-hoc HTTP request (not saved)
    Send {
        method: String,
        url: String,
        #[arg(short = 'H', long)]
        header: Vec<String>,
        #[arg(short, long)]
        body: Option<String>,
        #[arg(long)]
        auth_bearer: Option<String>,
        #[arg(long)]
        auth_basic: Option<String>,
    },

    /// Create a new request in an existing collection
    Create {
        /// Request name (must be unique in the collection)
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        method: String,
        #[arg(short, long)]
        url: String,
        /// Collection name (must exist unless --create-collection is given)
        #[arg(short = 'C', long)]
        collection: String,
        /// Create the collection if it doesn't exist
        #[arg(long)]
        create_collection: bool,
        #[arg(short = 'H', long)]
        header: Vec<String>,
        #[arg(short, long)]
        body: Option<String>,
        #[arg(long, value_enum, default_value = "json")]
        body_type: BodyType,
    },

    /// Manage request chains
    Chain {
        #[command(subcommand)]
        action: ChainAction,
    },

    /// Connect to a WebSocket endpoint
    Ws {
        url: String,
        #[arg(short = 'H', long)]
        header: Vec<String>,
        #[arg(short, long)]
        message: Vec<String>,
        #[arg(short, long, default_value = "5")]
        timeout: u64,
    },

    /// Connect to an SSE endpoint
    Sse {
        url: String,
        #[arg(short = 'H', long)]
        header: Vec<String>,
        #[arg(short, long)]
        duration: Option<u64>,
        #[arg(short, long)]
        max_events: Option<usize>,
    },

    /// Run every request of a collection (or folder) and its assertions
    Test(TestArgs),
}

/// Report format of `hitt test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Reporter {
    Text,
    Json,
    Junit,
}

/// Arguments of `hitt test`. Exits non-zero if any request fails.
#[derive(clap::Args, Debug, Clone)]
pub struct TestArgs {
    /// Collection name
    pub collection: String,
    /// Only run this folder (name or path like `Users/Admin`)
    #[arg(short, long)]
    pub folder: Option<String>,
    /// Environment name
    #[arg(short, long)]
    pub env: Option<String>,
    /// Stop after the first failed request
    #[arg(long)]
    pub bail: bool,
    /// Report format
    #[arg(short, long, value_enum, default_value = "text")]
    pub reporter: Reporter,
    /// Write the report to this file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Delay between requests in milliseconds
    #[arg(long, default_value = "0")]
    pub delay: u64,
    /// Number of times to run the collection (at least 1)
    #[arg(long, default_value = "1", value_parser = clap::value_parser!(u32).range(1..))]
    pub iterations: u32,
}

#[derive(Subcommand, Debug)]
pub enum ChainAction {
    /// List chains in a collection
    List {
        #[arg(short, long)]
        collection: String,
    },

    /// Execute a chain
    Run {
        name: String,
        #[arg(short, long)]
        collection: String,
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Create a simple chain from request names
    Create {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        collection: String,
        #[arg(short, long)]
        step: Vec<String>,
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Import a chain from a YAML file
    Import {
        file: PathBuf,
        #[arg(short, long)]
        collection: String,
    },
}

/// Main dispatcher for CLI subcommands.
pub async fn run(cmd: Commands, config: &AppConfig) -> Result<()> {
    match cmd {
        Commands::Collections => cmd_collections(config),
        Commands::Requests { collection } => cmd_requests(&collection, config),
        Commands::Run {
            name,
            collection,
            env,
        } => cmd_run(&name, collection.as_deref(), env.as_deref(), config).await,
        Commands::Send {
            method,
            url,
            header,
            body,
            auth_bearer,
            auth_basic,
        } => {
            cmd_send(
                &method,
                &url,
                &header,
                body.as_deref(),
                auth_bearer.as_deref(),
                auth_basic.as_deref(),
                config,
            )
            .await
        }
        Commands::Create {
            name,
            method,
            url,
            collection,
            create_collection,
            header,
            body,
            body_type,
        } => cmd_create(
            &CreateArgs {
                name: &name,
                method: &method,
                url: &url,
                collection: &collection,
                create_collection,
                headers: &header,
                body: body.as_deref(),
                body_type,
            },
            config,
        ),
        Commands::Chain { action } => match action {
            ChainAction::List { collection } => cmd_chain_list(&collection, config),
            ChainAction::Run {
                name,
                collection,
                env,
            } => cmd_chain_run(&name, &collection, env.as_deref(), config).await,
            ChainAction::Create {
                name,
                collection,
                step,
                description,
            } => cmd_chain_create(&name, &collection, &step, description.as_deref(), config),
            ChainAction::Import { file, collection } => {
                cmd_chain_import(&file, &collection, config)
            }
        },
        Commands::Ws {
            url,
            header,
            message,
            timeout,
        } => cmd_ws(&url, &header, &message, timeout).await,
        Commands::Sse {
            url,
            header,
            duration,
            max_events,
        } => cmd_sse(&url, &header, duration, max_events).await,
        Commands::Test(args) => cmd_test(&args, config).await,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The environment named `name` (`None` if no name was given); an unknown
/// name is an error rather than silently running without variables.
fn require_environment(name: Option<&str>, config: &AppConfig) -> Result<Option<Environment>> {
    match name {
        Some(name) => {
            Ok(Some(load_environment(name, config)?.ok_or_else(|| {
                anyhow::anyhow!("Environment '{name}' not found")
            })?))
        }
        None => Ok(None),
    }
}

fn response_to_json(response: &Response) -> serde_json::Value {
    let headers: serde_json::Map<String, serde_json::Value> = response
        .headers
        .iter()
        .map(|h| (h.key.clone(), json!(h.value)))
        .collect();

    let body: serde_json::Value = response
        .body_text()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| json!(response.body_text().unwrap_or("")));

    json!({
        "status": response.status,
        "status_text": response.status_text,
        "headers": headers,
        "body": body,
        "timing_ms": u64::try_from(response.timing.total.as_millis()).unwrap_or(u64::MAX),
        "size_bytes": response.size.total(),
    })
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn cmd_collections(config: &AppConfig) -> Result<()> {
    let collections = load_collections(config)?;
    let output: Vec<serde_json::Value> = collections
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "description": c.description,
                "request_count": c.request_count(),
                "chain_count": c.chains.len(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn cmd_requests(collection_name: &str, config: &AppConfig) -> Result<()> {
    let collections = load_collections(config)?;
    let collection = find_collection(collection_name, &collections)?;
    let requests: Vec<serde_json::Value> = collection
        .all_requests()
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "method": r.method.as_str(),
                "url": r.url,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&requests)?);
    Ok(())
}

async fn cmd_run(
    name: &str,
    collection_name: Option<&str>,
    env_name: Option<&str>,
    config: &AppConfig,
) -> Result<()> {
    let collections = load_collections(config)?;

    let (collection, request) = if let Some(coll_name) = collection_name {
        let coll = find_collection(coll_name, &collections)?;
        let req = find_request_by_name(name, coll).ok_or_else(|| {
            anyhow::anyhow!("Request '{name}' not found in collection '{coll_name}'")
        })?;
        (coll, req)
    } else {
        // Search across all collections
        let mut found = None;
        for coll in &collections {
            if let Some(req) = find_request_by_name(name, coll) {
                found = Some((coll, req));
                break;
            }
        }
        found.ok_or_else(|| anyhow::anyhow!("Request '{name}' not found in any collection"))?
    };

    let environment = require_environment(env_name, config)?;

    let resolver = VariableResolver::from_context(
        None,
        &collection.variables,
        environment.as_ref(),
        None,
        None,
    );

    let client = HttpClient::new()?;
    let response = client.send(request, &resolver).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&response_to_json(&response))?
    );
    Ok(())
}

async fn cmd_send(
    method: &str,
    url: &str,
    headers: &[String],
    body: Option<&str>,
    auth_bearer: Option<&str>,
    auth_basic: Option<&str>,
    _config: &AppConfig,
) -> Result<()> {
    #[allow(deprecated)]
    let http_method = HttpMethod::from_str(method)
        .ok_or_else(|| anyhow::anyhow!("Invalid HTTP method: '{method}'"))?;

    let mut request = Request::new("ad-hoc", http_method, url);
    request.headers = parse_headers(headers);

    if let Some(body_str) = body {
        request.body = Some(RequestBody::Json(body_str.to_string()));
    }

    if let Some(token) = auth_bearer {
        request.auth = Some(AuthConfig::bearer(token));
    } else if let Some(basic) = auth_basic {
        if let Some((user, pass)) = basic.split_once(':') {
            request.auth = Some(AuthConfig::basic(user, pass));
        } else {
            bail!("--auth-basic must be in format 'username:password'");
        }
    }

    let resolver = VariableResolver::new();
    let client = HttpClient::new()?;
    let response = client.send(&request, &resolver).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&response_to_json(&response))?
    );
    Ok(())
}

/// Arguments of `hitt create`.
struct CreateArgs<'a> {
    name: &'a str,
    method: &'a str,
    url: &'a str,
    collection: &'a str,
    create_collection: bool,
    headers: &'a [String],
    body: Option<&'a str>,
    body_type: BodyType,
}

fn cmd_create(args: &CreateArgs<'_>, config: &AppConfig) -> Result<()> {
    let &CreateArgs {
        name,
        method,
        url,
        collection: collection_name,
        create_collection,
        headers,
        body,
        body_type,
    } = args;
    #[allow(deprecated)]
    let http_method = HttpMethod::from_str(method)
        .ok_or_else(|| anyhow::anyhow!("Invalid HTTP method: '{method}'"))?;

    let mut request = Request::new(name, http_method, url);
    request.headers = parse_headers(headers);

    if let Some(body_str) = body {
        request.body = Some(match body_type {
            BodyType::Json => RequestBody::Json(body_str.to_string()),
            BodyType::Raw => RequestBody::Raw {
                content: body_str.to_string(),
                content_type: "text/plain".to_string(),
            },
        });
    }

    let store = CollectionsStore::new(config.collections_dir.clone())?;
    let mut collections = store.load_all()?;

    let collection = collections
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(collection_name));

    if let Some(coll) = collection {
        if find_request_by_name(name, coll).is_some() {
            bail!(
                "Request '{name}' already exists in collection '{}'",
                coll.name
            );
        }
        coll.add_request(request);
        store.save_collection(coll)?;
    } else if create_collection {
        let mut coll = Collection::new(collection_name);
        coll.add_request(request);
        store.save_collection(&coll)?;
    } else {
        bail!("Collection '{collection_name}' not found (use --create-collection to create it)");
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "created": true,
            "name": name,
            "collection": collection_name,
        }))?
    );
    Ok(())
}

fn cmd_chain_list(collection_name: &str, config: &AppConfig) -> Result<()> {
    let collections = load_collections(config)?;
    let collection = find_collection(collection_name, &collections)?;
    let chains: Vec<serde_json::Value> = collection
        .chains
        .iter()
        .map(|ch| {
            json!({
                "name": ch.name,
                "description": ch.description,
                "step_count": ch.steps.len(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&chains)?);
    Ok(())
}

async fn cmd_chain_run(
    name: &str,
    collection_name: &str,
    env_name: Option<&str>,
    config: &AppConfig,
) -> Result<()> {
    let collections = load_collections(config)?;
    let collection = find_collection(collection_name, &collections)?;

    let chain = find_chain_by_name(name, collection).ok_or_else(|| {
        anyhow::anyhow!("Chain '{name}' not found in collection '{collection_name}'")
    })?;
    // Names are unique now, but older versions allowed duplicates: refuse to
    // guess which one was meant.
    let same_name = collection
        .chains
        .iter()
        .filter(|ch| ch.name.trim().eq_ignore_ascii_case(name.trim()))
        .count();
    if same_name > 1 {
        bail!(
            "{same_name} chains are named '{name}' in collection '{collection_name}'; rename or delete the extra ones first"
        );
    }

    let environment = require_environment(env_name, config)?;

    let client = HttpClient::new()?;
    let mut steps_output: Vec<serde_json::Value> = Vec::new();
    let mut failure: Option<String> = None;

    let extracted_variables = chain_executor::execute_chain(
        &client,
        collection,
        chain,
        environment.as_ref(),
        |outcome| match outcome {
            StepOutcome::Success {
                step_index,
                request_name,
                status,
                duration_ms,
                extracted,
                ..
            } => {
                let extractions_json: serde_json::Map<String, serde_json::Value> = extracted
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(v)))
                    .collect();
                steps_output.push(json!({
                    "step": step_index + 1,
                    "request": request_name,
                    "status": status,
                    "timing_ms": duration_ms,
                    "extractions": extractions_json,
                }));
            }
            StepOutcome::Failed { step_index, error } => {
                let request_name = chain
                    .steps
                    .get(*step_index)
                    .and_then(|step| collection.find_request(&step.request_id))
                    .map(|r| r.name.clone());
                let label = request_name
                    .as_ref()
                    .map_or_else(String::new, |n| format!(" ('{n}')"));
                failure = Some(format!("step {}{label}: {error}", step_index + 1));
                steps_output.push(json!({
                    "step": step_index + 1,
                    "request": request_name,
                    "error": error,
                }));
            }
            StepOutcome::Skipped { step_index, reason } => {
                steps_output.push(json!({
                    "step": step_index + 1,
                    "skipped": true,
                    "reason": reason,
                }));
            }
        },
    )
    .await;

    let variables_json: serde_json::Map<String, serde_json::Value> = extracted_variables
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "chain": name,
            "steps": steps_output,
            "variables": variables_json,
        }))?
    );
    match failure {
        Some(failure) => bail!("Chain '{name}' failed at {failure}"),
        None => Ok(()),
    }
}

fn cmd_chain_create(
    name: &str,
    collection_name: &str,
    step_names: &[String],
    description: Option<&str>,
    config: &AppConfig,
) -> Result<()> {
    let store = CollectionsStore::new(config.collections_dir.clone())?;
    let mut collections = store.load_all()?;

    let collection = collections
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(collection_name))
        .ok_or_else(|| anyhow::anyhow!("Collection '{collection_name}' not found"))?;

    if let Some(existing) = find_chain_by_name(name, collection) {
        bail!(
            "Chain '{}' already exists in collection '{}'",
            existing.name,
            collection.name
        );
    }

    let all_requests = collection.all_requests();

    let mut chain = RequestChain::new(name);
    chain.description = description.map(std::string::ToString::to_string);

    let mut not_found = Vec::new();
    for step_name in step_names {
        match all_requests
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(step_name))
        {
            Some(req) => {
                chain.add_step(req.id);
            }
            None => {
                not_found.push(step_name.clone());
            }
        }
    }

    if !not_found.is_empty() {
        bail!(
            "Request(s) not found in collection '{}': {}",
            collection_name,
            not_found.join(", ")
        );
    }

    collection.chains.push(chain);
    store.save_collection(collection)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "created": true,
            "chain": name,
            "collection": collection_name,
            "step_count": step_names.len(),
        }))?
    );
    Ok(())
}

fn cmd_chain_import(file: &PathBuf, collection_name: &str, config: &AppConfig) -> Result<()> {
    let content = std::fs::read_to_string(file)?;

    let store = CollectionsStore::new(config.collections_dir.clone())?;
    let mut collections = store.load_all()?;

    let collection = collections
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(collection_name))
        .ok_or_else(|| anyhow::anyhow!("Collection '{collection_name}' not found"))?;

    let mut chain = hitt_core::importers::chain::import_chain(&content, collection)?;
    // Chain names are unique per collection: "Flow" -> "Flow (2)".
    chain.name = unique_name(&chain.name, |n| find_chain_by_name(n, collection).is_some());
    let chain_name = chain.name.clone();
    let step_count = chain.steps.len();
    collection.chains.push(chain);
    store.save_collection(collection)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "imported": true,
            "chain": chain_name,
            "collection": collection_name,
            "step_count": step_count,
        }))?
    );
    Ok(())
}

async fn cmd_ws(url: &str, headers: &[String], messages: &[String], timeout: u64) -> Result<()> {
    use hitt_core::protocols::websocket::{self, WsCommand, WsEvent};
    use tokio::sync::mpsc;

    let parsed_headers = parse_headers(headers);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<WsEvent>();

    let cmd_tx = websocket::connect(url, &parsed_headers, event_tx).await?;

    // Wait for connection
    let timeout_dur = std::time::Duration::from_secs(timeout);
    let connected = tokio::time::timeout(timeout_dur, async {
        while let Some(event) = event_rx.recv().await {
            match event {
                WsEvent::Connected => return Ok(()),
                WsEvent::Error(e) => return Err(anyhow::anyhow!("WebSocket error: {e}")),
                _ => {}
            }
        }
        Err(anyhow::anyhow!("WebSocket connection closed unexpectedly"))
    })
    .await;

    match connected {
        Ok(Ok(())) => {}
        Ok(Err(e)) => bail!(e),
        Err(_) => bail!("WebSocket connection timed out after {timeout}s"),
    }

    // Send messages
    for msg in messages {
        let _ = cmd_tx.send(WsCommand::SendText(msg.clone()));
    }

    // Collect responses until timeout
    let mut received = Vec::new();
    let collect_timeout = std::time::Duration::from_secs(timeout);

    let _ = tokio::time::timeout(collect_timeout, async {
        while let Some(event) = event_rx.recv().await {
            match event {
                WsEvent::MessageReceived(msg) => {
                    received.push(json!({
                        "direction": "received",
                        "content": msg.content.display(),
                        "timestamp": msg.timestamp.to_rfc3339(),
                    }));
                }
                WsEvent::Disconnected => break,
                WsEvent::Error(e) => {
                    received.push(json!({
                        "error": e,
                    }));
                    break;
                }
                WsEvent::Connected => {}
            }
        }
    })
    .await;

    // Disconnect
    let _ = cmd_tx.send(WsCommand::Disconnect);

    let sent: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| json!({"direction": "sent", "content": m}))
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "url": url,
            "sent": sent,
            "received": received,
        }))?
    );
    Ok(())
}

async fn cmd_sse(
    url: &str,
    headers: &[String],
    duration: Option<u64>,
    max_events: Option<usize>,
) -> Result<()> {
    use hitt_core::protocols::sse::{self, SseCommand, SseOutput};
    use tokio::sync::mpsc;

    let parsed_headers = parse_headers(headers);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<SseOutput>();

    let cmd_tx = sse::connect(url, &parsed_headers, event_tx)?;

    let timeout_dur = duration.map_or_else(
        || std::time::Duration::from_secs(30),
        std::time::Duration::from_secs,
    );

    let max = max_events.unwrap_or(usize::MAX);
    let mut count = 0;
    let mut failure: Option<String> = None;

    let _ = tokio::time::timeout(timeout_dur, async {
        while let Some(event) = event_rx.recv().await {
            match event {
                SseOutput::Connected => {
                    // Connected, waiting for events
                }
                SseOutput::Event(sse_event) => {
                    let event_json = json!({
                        "event_type": sse_event.event_type.unwrap_or_else(|| "message".to_string()),
                        "data": sse_event.data,
                        "id": sse_event.id,
                    });
                    // Print as JSONL (one JSON per line)
                    println!("{}", serde_json::to_string(&event_json).unwrap_or_default());
                    count += 1;
                    if count >= max {
                        break;
                    }
                }
                SseOutput::Error(e) => {
                    eprintln!(
                        "{}",
                        serde_json::to_string(&json!({"error": e})).unwrap_or_default()
                    );
                    failure = Some(e);
                    break;
                }
                SseOutput::Disconnected => break,
            }
        }
    })
    .await;

    // Disconnect
    let _ = cmd_tx.send(SseCommand::Disconnect);

    match failure {
        Some(e) => bail!("SSE error: {e}"),
        None => Ok(()),
    }
}

/// `hitt test` — run a collection and report; errors if any request failed.
async fn cmd_test(args: &TestArgs, config: &AppConfig) -> Result<()> {
    use hitt_core::testing::collection_runner::{
        self as runner, result_line, RunEvent, RunnerOptions,
    };

    // Fail before running anything if the report couldn't be written.
    if let Some(path) = &args.output {
        validate_output_path(path)?;
    }
    let collections = load_collections(config)?;
    let collection = find_collection(&args.collection, &collections)?;
    let environment = require_environment(args.env.as_deref(), config)?;
    let options = RunnerOptions {
        folder: args.folder.clone(),
        stop_on_failure: args.bail,
        delay_ms: args.delay,
        iterations: args.iterations,
    };

    // Live progress goes to stderr unless the text report is printed to stdout
    // (it already lists every request).
    let live = args.reporter != Reporter::Text || args.output.is_some();
    let client = HttpClient::from_config(config)?;
    let summary = runner::run_collection(
        &client,
        collection,
        environment.as_ref(),
        &options,
        |event| {
            if let (true, RunEvent::RequestFinished(result)) = (live, event) {
                eprintln!("{}", result_line(result));
            }
        },
    )
    .await?;

    let report = match args.reporter {
        Reporter::Text => runner::to_text(&summary),
        Reporter::Json => runner::to_json(&summary)?,
        Reporter::Junit => runner::to_junit_xml(&summary),
    };
    match &args.output {
        Some(path) => {
            std::fs::write(path, &report)?;
            eprintln!(
                "{} passed, {} failed, {} errors, {} skipped -> {}",
                summary.passed,
                summary.failed,
                summary.errors,
                summary.skipped,
                path.display()
            );
        }
        None => print!("{report}"),
    }

    if summary.is_success() {
        Ok(())
    } else {
        bail!(
            "{} of {} request(s) failed",
            summary.failed + summary.errors,
            summary.total
        )
    }
}

/// Check that a report can be written to `path`: its directory must exist and
/// it must not itself be a directory.
fn validate_output_path(path: &std::path::Path) -> Result<()> {
    if path.is_dir() {
        bail!("Output path '{}' is a directory", path.display());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if !parent.is_dir() {
            bail!("Output directory '{}' does not exist", parent.display());
        }
    }
    Ok(())
}
