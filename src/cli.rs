use anyhow::{bail, Result};
use clap::Subcommand;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::core::auth::AuthConfig;
use crate::core::chain::{evaluate_condition, extract_values, RequestChain};
use crate::core::client::HttpClient;
use crate::core::collection::{Collection, CollectionItem};
use crate::core::environment::Environment;
use crate::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use crate::core::response::Response;
use crate::core::variables::VariableResolver;
use crate::storage::collections_store::CollectionsStore;
use crate::storage::config::AppConfig;

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

    /// Create a new request in a collection
    Create {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        method: String,
        #[arg(short, long)]
        url: String,
        #[arg(short = 'C', long)]
        collection: String,
        #[arg(short = 'H', long)]
        header: Vec<String>,
        #[arg(short, long)]
        body: Option<String>,
        #[arg(long, default_value = "json")]
        body_type: String,
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
        } => cmd_send(&method, &url, &header, body.as_deref(), auth_bearer.as_deref(), auth_basic.as_deref(), config).await,
        Commands::Create {
            name,
            method,
            url,
            collection,
            header,
            body,
            body_type,
        } => cmd_create(&name, &method, &url, &collection, &header, body.as_deref(), &body_type, config),
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
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_collections(config: &AppConfig) -> Result<Vec<Collection>> {
    let store = CollectionsStore::new(config.collections_dir.clone())?;
    store.load_all()
}

fn find_collection<'a>(name: &str, collections: &'a [Collection]) -> Result<&'a Collection> {
    collections
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("Collection '{}' not found", name))
}

fn load_environment(name: &str, config: &AppConfig) -> Result<Option<Environment>> {
    let store = CollectionsStore::new(config.collections_dir.clone())?;
    let environments = store.load_environments()?;
    Ok(environments.into_iter().find(|e| e.name.eq_ignore_ascii_case(name)))
}

fn parse_headers(header_strings: &[String]) -> Vec<KeyValuePair> {
    header_strings
        .iter()
        .filter_map(|s| {
            let (key, value) = s.split_once(':')?;
            Some(KeyValuePair::new(key.trim(), value.trim()))
        })
        .collect()
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
        .unwrap_or_else(|| {
            json!(response.body_text().unwrap_or(""))
        });

    json!({
        "status": response.status,
        "status_text": response.status_text,
        "headers": headers,
        "body": body,
        "timing_ms": response.timing.total.as_millis() as u64,
        "size_bytes": response.size.total(),
    })
}

fn find_request_by_name<'a>(name: &str, collection: &'a Collection) -> Option<&'a Request> {
    collection
        .all_requests()
        .into_iter()
        .find(|r| r.name.eq_ignore_ascii_case(name))
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
        let req = find_request_by_name(name, coll)
            .ok_or_else(|| anyhow::anyhow!("Request '{}' not found in collection '{}'", name, coll_name))?;
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
        found.ok_or_else(|| anyhow::anyhow!("Request '{}' not found in any collection", name))?
    };

    let environment = if let Some(env) = env_name {
        load_environment(env, config)?
    } else {
        None
    };

    let resolver = VariableResolver::from_context(
        None,
        &collection.variables,
        environment.as_ref(),
        None,
        None,
    );

    let client = HttpClient::new()?;
    let response = client.send(request, &resolver).await?;
    println!("{}", serde_json::to_string_pretty(&response_to_json(&response))?);
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
    let http_method = HttpMethod::from_str(method)
        .ok_or_else(|| anyhow::anyhow!("Invalid HTTP method: '{}'", method))?;

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
    println!("{}", serde_json::to_string_pretty(&response_to_json(&response))?);
    Ok(())
}

fn cmd_create(
    name: &str,
    method: &str,
    url: &str,
    collection_name: &str,
    headers: &[String],
    body: Option<&str>,
    body_type: &str,
    config: &AppConfig,
) -> Result<()> {
    let http_method = HttpMethod::from_str(method)
        .ok_or_else(|| anyhow::anyhow!("Invalid HTTP method: '{}'", method))?;

    let mut request = Request::new(name, http_method, url);
    request.headers = parse_headers(headers);

    if let Some(body_str) = body {
        request.body = Some(match body_type {
            "json" => RequestBody::Json(body_str.to_string()),
            "raw" => RequestBody::Raw {
                content: body_str.to_string(),
                content_type: "text/plain".to_string(),
            },
            other => bail!("Unknown body type: '{}'. Use 'json' or 'raw'.", other),
        });
    }

    let store = CollectionsStore::new(config.collections_dir.clone())?;
    let mut collections = store.load_all()?;

    let collection = collections
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(collection_name));

    match collection {
        Some(coll) => {
            coll.add_request(request);
            store.save_collection(coll)?;
        }
        None => {
            let mut coll = Collection::new(collection_name);
            coll.add_request(request);
            store.save_collection(&coll)?;
        }
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

    let chain = collection
        .chains
        .iter()
        .find(|ch| ch.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("Chain '{}' not found in collection '{}'", name, collection_name))?;

    let environment = if let Some(env) = env_name {
        load_environment(env, config)?
    } else {
        None
    };

    let client = HttpClient::new()?;
    let mut extracted_variables: HashMap<String, String> = HashMap::new();
    let mut last_response: Option<Response> = None;
    let mut steps_output: Vec<serde_json::Value> = Vec::new();

    for (step_index, step) in chain.steps.iter().enumerate() {
        // Check condition
        if let Some(ref condition) = step.condition {
            if !evaluate_condition(condition, last_response.as_ref(), &extracted_variables) {
                steps_output.push(json!({
                    "step": step_index + 1,
                    "skipped": true,
                    "reason": "Condition not met",
                }));
                continue;
            }
        }

        // Find request
        let request = match collection.find_request(&step.request_id) {
            Some(req) => req.clone(),
            None => {
                steps_output.push(json!({
                    "step": step_index + 1,
                    "error": format!("Request {} not found in collection", step.request_id),
                }));
                break;
            }
        };

        let request_name = request.name.clone();

        // Build resolver with current extracted variables
        let resolver = VariableResolver::from_context(
            Some(&extracted_variables),
            &collection.variables,
            environment.as_ref(),
            None,
            None,
        );

        // Send request
        match client.send(&request, &resolver).await {
            Ok(response) => {
                let status = response.status;
                let duration_ms = response.timing.total.as_millis() as u64;

                // Extract values
                let new_vars = extract_values(&step.extractions, &response);
                extracted_variables.extend(new_vars.clone());

                let extractions_json: serde_json::Map<String, serde_json::Value> = new_vars
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

                last_response = Some(response);
            }
            Err(e) => {
                steps_output.push(json!({
                    "step": step_index + 1,
                    "request": request_name,
                    "error": e.to_string(),
                }));
                break;
            }
        }

        // Apply delay
        if let Some(delay) = step.delay_ms {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
    }

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
    Ok(())
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
        .ok_or_else(|| anyhow::anyhow!("Collection '{}' not found", collection_name))?;

    let all_requests = collection.all_requests();

    let mut chain = RequestChain::new(name);
    chain.description = description.map(|s| s.to_string());

    let mut not_found = Vec::new();
    for step_name in step_names {
        match all_requests.iter().find(|r| r.name.eq_ignore_ascii_case(step_name)) {
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
        .ok_or_else(|| anyhow::anyhow!("Collection '{}' not found", collection_name))?;

    let chain = crate::importers::chain::import_chain(&content, collection)?;
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

async fn cmd_ws(
    url: &str,
    headers: &[String],
    messages: &[String],
    timeout: u64,
) -> Result<()> {
    use crate::protocols::websocket::{self, WsCommand, WsEvent};
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
                WsEvent::Error(e) => return Err(anyhow::anyhow!("WebSocket error: {}", e)),
                _ => {}
            }
        }
        Err(anyhow::anyhow!("WebSocket connection closed unexpectedly"))
    })
    .await;

    match connected {
        Ok(Ok(())) => {}
        Ok(Err(e)) => bail!(e),
        Err(_) => bail!("WebSocket connection timed out after {}s", timeout),
    }

    // Send messages
    for msg in messages {
        let _ = cmd_tx.send(WsCommand::SendText(msg.clone()));
    }

    // Collect responses until timeout
    let mut received = Vec::new();
    let collect_timeout = if messages.is_empty() {
        std::time::Duration::from_secs(timeout)
    } else {
        std::time::Duration::from_secs(timeout)
    };

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
                _ => {}
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
    use crate::protocols::sse::{self, SseCommand, SseOutput};
    use tokio::sync::mpsc;

    let parsed_headers = parse_headers(headers);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<SseOutput>();

    let cmd_tx = sse::connect(url, &parsed_headers, event_tx).await?;

    let timeout_dur = duration
        .map(std::time::Duration::from_secs)
        .unwrap_or_else(|| std::time::Duration::from_secs(30));

    let max = max_events.unwrap_or(usize::MAX);
    let mut count = 0;

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
                    eprintln!("{}", serde_json::to_string(&json!({"error": e})).unwrap_or_default());
                    break;
                }
                SseOutput::Disconnected => break,
            }
        }
    })
    .await;

    // Disconnect
    let _ = cmd_tx.send(SseCommand::Disconnect);

    Ok(())
}
