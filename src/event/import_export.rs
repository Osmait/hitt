use crate::app::{App, NotificationKind};
use crate::utils::expand_tilde;

use super::persistence::save_all_collections;

/// Import a file by path. Auto-detects format from extension.
pub(super) fn execute_import(app: &mut App, path_str: &str) {
    let expanded = expand_tilde(path_str);

    let content = match std::fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(e) => {
            app.notify(format!("Cannot read file: {e}"), NotificationKind::Error);
            return;
        }
    };

    let ext = expanded
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        // Postman collection or environment JSON
        "json" => {
            // Try Postman collection first, then environment
            if let Ok(collection) = crate::postman::import::import_postman_collection(&content) {
                let name = collection.name.clone();
                let count = collection.request_count();
                app.collections.push(collection);
                app.notify(
                    format!("Imported collection '{name}' ({count} requests)"),
                    NotificationKind::Success,
                );
            } else if let Ok(env) = crate::postman::env_import::import_postman_environment(&content)
            {
                let name = env.name.clone();
                app.environments.push(env);
                app.notify(
                    format!("Imported environment '{name}'"),
                    NotificationKind::Success,
                );
            } else {
                app.notify(
                    "Failed to parse JSON (not a Postman collection or environment)".into(),
                    NotificationKind::Error,
                );
            }
        }
        // HAR (HTTP Archive)
        "har" => match crate::importers::har::import_har(&content) {
            Ok(collection) => {
                let count = collection.request_count();
                app.collections.push(collection);
                app.notify(
                    format!("Imported HAR archive ({count} requests)"),
                    NotificationKind::Success,
                );
            }
            Err(e) => {
                app.notify(format!("Failed to parse HAR: {e}"), NotificationKind::Error);
            }
        },
        // Chain YAML or OpenAPI / Swagger
        "yaml" | "yml" => {
            if crate::importers::chain::looks_like_chain(&content) && !app.collections.is_empty() {
                let coll_idx = app.active_tab().collection_index.unwrap_or(0);
                let coll_idx = coll_idx.min(app.collections.len() - 1);
                match crate::importers::chain::import_chain(&content, &app.collections[coll_idx]) {
                    Ok(chain) => {
                        let chain_name = chain.name.clone();
                        let step_count = chain.steps.len();
                        let coll_name = app.collections[coll_idx].name.clone();
                        app.collections[coll_idx].chains.push(chain);
                        app.notify(
                            format!("Imported chain '{chain_name}' ({step_count} steps) into '{coll_name}'"),
                            NotificationKind::Success,
                        );
                    }
                    Err(_) => {
                        // Fall through to OpenAPI
                        match crate::importers::openapi::import_openapi(&content) {
                            Ok(collection) => {
                                let name = collection.name.clone();
                                let count = collection.request_count();
                                app.collections.push(collection);
                                app.notify(
                                    format!("Imported OpenAPI '{name}' ({count} requests)"),
                                    NotificationKind::Success,
                                );
                            }
                            Err(e) => {
                                app.notify(
                                    format!("Failed to parse YAML: {e}"),
                                    NotificationKind::Error,
                                );
                            }
                        }
                    }
                }
            } else {
                match crate::importers::openapi::import_openapi(&content) {
                    Ok(collection) => {
                        let name = collection.name.clone();
                        let count = collection.request_count();
                        app.collections.push(collection);
                        app.notify(
                            format!("Imported OpenAPI '{name}' ({count} requests)"),
                            NotificationKind::Success,
                        );
                    }
                    Err(e) => {
                        app.notify(
                            format!("Failed to parse OpenAPI: {e}"),
                            NotificationKind::Error,
                        );
                    }
                }
            }
        }
        // .env files
        "env" => {
            match crate::importers::dotenv::dotenv_to_environment(
                expanded
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(".env"),
                &content,
            ) {
                Ok(env) => {
                    let name = env.name.clone();
                    app.environments.push(env);
                    if app.active_env.is_none() {
                        app.active_env = Some(app.environments.len() - 1);
                    }
                    app.notify(
                        format!("Loaded environment '{name}'"),
                        NotificationKind::Success,
                    );
                }
                Err(e) => {
                    app.notify(
                        format!("Failed to parse .env: {e}"),
                        NotificationKind::Error,
                    );
                }
            }
        }
        // Try cURL (any other extension or no extension)
        _ => match crate::importers::curl::parse_curl(&content) {
            Ok(request) => {
                app.tabs
                    .push(crate::app::RequestTab::from_request(request, None));
                app.active_tab = app.tabs.len() - 1;
                app.notify("Imported from cURL".into(), NotificationKind::Success);
            }
            Err(e) => {
                app.notify(
                    format!("Unknown format. cURL parse failed: {e}"),
                    NotificationKind::Error,
                );
            }
        },
    }

    // Persist all collections to disk after a successful import.
    save_all_collections(app);
}

/// Read a YAML file and import it as a chain into the active collection.
pub(super) fn execute_import_chain(app: &mut App, path_str: &str) {
    let expanded = expand_tilde(path_str);

    let content = match std::fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(e) => {
            app.notify(format!("Cannot read file: {e}"), NotificationKind::Error);
            return;
        }
    };

    let coll_idx = app.active_tab().collection_index.unwrap_or(0);
    let coll_idx = coll_idx.min(app.collections.len() - 1);

    match crate::importers::chain::import_chain(&content, &app.collections[coll_idx]) {
        Ok(chain) => {
            let chain_name = chain.name.clone();
            let step_count = chain.steps.len();
            let coll_name = app.collections[coll_idx].name.clone();
            app.collections[coll_idx].chains.push(chain);
            save_all_collections(app);
            app.notify(
                format!("Imported chain '{chain_name}' ({step_count} steps) into '{coll_name}'"),
                NotificationKind::Success,
            );
        }
        Err(e) => {
            app.notify(
                format!("Failed to import chain: {e}"),
                NotificationKind::Error,
            );
        }
    }
}

/// Export to a file. Format is auto-detected from extension.
///
/// Supported extensions:
///   .json  -> Postman collection
///   .md    -> Markdown documentation
///   .sh / .curl / .txt -> cURL command(s)
///   .env   -> Environment variables
pub(super) fn execute_export(app: &mut App, path_str: &str) {
    let expanded = expand_tilde(path_str);

    let ext = expanded
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let result: Result<String, String> = match ext.as_str() {
        "json" => {
            // Export active collection as Postman, or current request if no collection
            if let Some(coll_idx) = app.active_tab().collection_index {
                if let Some(coll) = app.collections.get(coll_idx) {
                    crate::postman::export::export_postman_collection(coll)
                        .map_err(|e| e.to_string())
                } else {
                    Err("Collection not found".into())
                }
            } else if !app.collections.is_empty() {
                // Export the first collection
                crate::postman::export::export_postman_collection(&app.collections[0])
                    .map_err(|e| e.to_string())
            } else {
                Err("No collection to export".into())
            }
        }
        "md" | "markdown" => {
            if let Some(coll_idx) = app.active_tab().collection_index {
                if let Some(coll) = app.collections.get(coll_idx) {
                    Ok(crate::exporters::markdown_docs::generate_docs(coll))
                } else {
                    Err("Collection not found".into())
                }
            } else if !app.collections.is_empty() {
                Ok(crate::exporters::markdown_docs::generate_docs(
                    &app.collections[0],
                ))
            } else {
                Err("No collection to export".into())
            }
        }
        "sh" | "curl" | "txt" => {
            let resolver = app.build_resolver();
            let curl = crate::exporters::curl::to_curl(&app.active_tab().request, &resolver);
            Ok(curl)
        }
        "env" => {
            if let Some(env) = app.active_environment() {
                crate::postman::env_export::export_postman_environment(env)
                    .map_err(|e| e.to_string())
            } else {
                Err("No active environment to export".into())
            }
        }
        _ => Err(format!(
            "Unknown export format '.{ext}'. Use .json, .md, .sh, .curl, or .env"
        )),
    };

    match result {
        Ok(content) => match std::fs::write(&expanded, &content) {
            Ok(()) => {
                app.notify(
                    format!("Exported to {}", expanded.display()),
                    NotificationKind::Success,
                );
            }
            Err(e) => {
                app.notify(
                    format!("Failed to write file: {e}"),
                    NotificationKind::Error,
                );
            }
        },
        Err(e) => {
            app.notify(e, NotificationKind::Error);
        }
    }
}
