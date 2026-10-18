use std::path::Path;

use crate::app::{App, FocusArea, NotificationKind, SidebarSection};
use crate::utils::expand_tilde;

use super::persistence::{
    chain_name_taken, collection_name_taken, environment_name_taken, save_all_collections,
    save_environment, unique_name,
};
use super::sidebar::{build_sidebar_items, SidebarItem};

/// File formats recognised by `:import` / `:export`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Json,
    Har,
    Yaml,
    Markdown,
    /// `.env`, `*.env` and `.env.*` (e.g. `.env.local`).
    DotEnv,
    /// `.curl`, `.sh`, `.txt` or no extension.
    Curl,
    Unsupported,
}

fn file_kind(path: &Path) -> FileKind {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    // `Path::extension` is None for `.env`, so match dotenv names first
    // (`name` is already lowercase).
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if name == ".env" || name.starts_with(".env.") || name.ends_with(".env") {
        return FileKind::DotEnv;
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("json") => FileKind::Json,
        Some("har") => FileKind::Har,
        Some("yaml" | "yml") => FileKind::Yaml,
        Some("md" | "markdown") => FileKind::Markdown,
        Some("curl" | "sh" | "txt") | None => FileKind::Curl,
        Some(_) => FileKind::Unsupported,
    }
}

/// Add an imported collection, renaming it to "Name (2)"… if the name is taken.
/// Returns the final name.
fn push_imported_collection(
    app: &mut App,
    mut collection: hitt_core::core::collection::Collection,
) -> String {
    collection.name = unique_name(&collection.name, |n| collection_name_taken(app, n, None));
    let name = collection.name.clone();
    app.collections.push(collection);
    name
}

/// Add an imported environment, renaming it to "Name (2)"… if the name is
/// taken (saving it would otherwise overwrite the existing file).
/// Returns the final name.
pub(super) fn push_imported_environment(
    app: &mut App,
    mut env: hitt_core::core::environment::Environment,
) -> String {
    env.name = unique_name(&env.name, |n| environment_name_taken(app, n));
    let name = env.name.clone();
    app.environments.push(env);
    name
}

/// Import a file by path. Auto-detects format from the file name.
pub(super) fn execute_import(app: &mut App, path_str: &str) {
    let expanded = expand_tilde(path_str);
    let kind = file_kind(&expanded);
    if kind == FileKind::Unsupported || kind == FileKind::Markdown {
        app.notify(
            format!(
                "Unsupported file type: {}. Use .json, .har, .yaml, .env or a cURL file",
                expanded.display()
            ),
            NotificationKind::Error,
        );
        return;
    }

    let content = match std::fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(e) => {
            app.notify(format!("Cannot read file: {e}"), NotificationKind::Error);
            return;
        }
    };

    match kind {
        // Postman collection or environment JSON
        FileKind::Json => {
            // Try Postman collection first, then environment
            if let Ok(collection) = hitt_core::postman::import::import_postman_collection(&content)
            {
                let count = collection.request_count();
                let name = push_imported_collection(app, collection);
                app.notify(
                    format!("Imported collection '{name}' ({count} requests)"),
                    NotificationKind::Success,
                );
            } else if let Ok(env) =
                hitt_core::postman::env_import::import_postman_environment(&content)
            {
                let name = push_imported_environment(app, env);
                let idx = app.environments.len() - 1;
                if app.active_env.is_none() {
                    app.active_env = Some(idx);
                    app.remember_active_env();
                }
                save_environment(app, idx);
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
        FileKind::Har => match hitt_core::importers::har::import_har(&content) {
            Ok(collection) => {
                let count = collection.request_count();
                push_imported_collection(app, collection);
                app.notify(
                    format!("Imported HAR archive ({count} requests)"),
                    NotificationKind::Success,
                );
            }
            Err(e) => {
                app.notify(format!("Failed to parse HAR: {e}"), NotificationKind::Error);
            }
        },
        // Chain YAML (has a top-level `steps`) or OpenAPI / Swagger
        FileKind::Yaml => {
            if hitt_core::importers::chain::looks_like_chain(&content) {
                import_chain_content(app, &content);
            } else {
                match hitt_core::importers::openapi::import_openapi(&content) {
                    Ok(collection) => {
                        let count = collection.request_count();
                        let name = push_imported_collection(app, collection);
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
        FileKind::DotEnv => {
            match hitt_core::importers::dotenv::dotenv_to_environment(
                expanded
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(".env"),
                &content,
            ) {
                Ok(env) => {
                    app.session_env_ids.insert(env.id);
                    let name = push_imported_environment(app, env);
                    if app.active_env.is_none() {
                        app.active_env = Some(app.environments.len() - 1);
                    }
                    // .env files are session-only layers: not persisted.
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
        FileKind::Curl => match hitt_core::importers::curl::parse_curl(&content) {
            Ok(request) => {
                app.tabs
                    .push(crate::app::RequestTab::from_request(request, None));
                app.switch_tab(app.tabs.len() - 1);
                app.notify("Imported from cURL".into(), NotificationKind::Success);
            }
            Err(e) => {
                app.notify(format!("Not a cURL command: {e}"), NotificationKind::Error);
            }
        },
        FileKind::Markdown | FileKind::Unsupported => {}
    }

    // Persist all collections to disk after a successful import.
    save_all_collections(app);
}

/// Import chain YAML `content` into the active tab's collection (or the
/// first one), reporting the chain parser's error on failure.
fn import_chain_content(app: &mut App, content: &str) -> bool {
    if app.collections.is_empty() {
        app.notify(
            "Chain import needs a collection. Use :newcol <name> first".into(),
            NotificationKind::Warning,
        );
        return false;
    }
    let coll_idx = app
        .active_tab()
        .collection_index
        .unwrap_or(0)
        .min(app.collections.len() - 1);

    match hitt_core::importers::chain::import_chain(content, &app.collections[coll_idx]) {
        Ok(mut chain) => {
            // Chain names are unique per collection: "Flow" -> "Flow (2)".
            chain.name = unique_name(&chain.name, |n| chain_name_taken(app, coll_idx, n));
            let chain_name = chain.name.clone();
            let step_count = chain.steps.len();
            let coll_name = app.collections[coll_idx].name.clone();
            app.selected_chain = Some(chain.id);
            app.collections[coll_idx].chains.push(chain);
            app.notify(
                format!("Imported chain '{chain_name}' ({step_count} steps) into '{coll_name}'"),
                NotificationKind::Success,
            );
            true
        }
        Err(e) => {
            app.notify(
                format!("Failed to import chain: {e}"),
                NotificationKind::Error,
            );
            false
        }
    }
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

    if import_chain_content(app, &content) {
        save_all_collections(app);
    }
}

/// The active tab's request as a cURL command, with the auth it inherits
/// from its folder / collection.
pub(super) fn active_request_curl(app: &App) -> String {
    let resolver = app.build_resolver();
    let tab = app.active_tab();
    match tab.collection_index.and_then(|i| app.collections.get(i)) {
        Some(coll) => hitt_core::exporters::curl::to_curl_in(coll, &tab.request, &resolver),
        None => hitt_core::exporters::curl::to_curl(&tab.request, &resolver),
    }
}

/// `KEY=value` lines for an environment's enabled variables. Values that
/// would not survive a dotenv round trip unquoted are double-quoted.
fn environment_to_dotenv(env: &hitt_core::core::environment::Environment) -> String {
    let mut out = format!("# Environment: {}\n", env.name);
    for (key, value) in env.active_variables() {
        out.push_str(key);
        out.push('=');
        out.push_str(&dotenv_value(value));
        out.push('\n');
    }
    out
}

fn dotenv_value(value: &str) -> String {
    let needs_quotes = value != value.trim()
        || value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '#' | '"' | '\'' | '\\' | '$' | '`' | '='));
    if !needs_quotes {
        return value.to_string();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

/// The collection `:export` writes: `name` if given (case-insensitive), else
/// the collection selected in the focused sidebar, else the active tab's.
fn export_collection<'a>(
    app: &'a App,
    name: Option<&str>,
) -> Result<&'a hitt_core::core::collection::Collection, String> {
    if let Some(name) = name {
        return app
            .collections
            .iter()
            .find(|c| c.name.trim().eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| format!("Collection '{name}' not found"));
    }
    let from_sidebar = (app.focus == FocusArea::Sidebar
        && app.sidebar_state.section == SidebarSection::Collections)
        .then(|| build_sidebar_items(app))
        .and_then(|items| match items.get(app.sidebar_state.selected)? {
            SidebarItem::Collection { coll_idx, .. }
            | SidebarItem::Folder { coll_idx, .. }
            | SidebarItem::Request { coll_idx, .. } => Some(*coll_idx),
        });
    from_sidebar
        .or(app.active_tab().collection_index)
        .and_then(|i| app.collections.get(i))
        .ok_or_else(|| {
            "No collection selected. Select one in the sidebar or use :export <path> <collection>"
                .to_string()
        })
}

/// Split `:export` arguments into the path and an optional collection name.
/// A path with a known export extension may contain spaces; otherwise the
/// first word is the path and the rest names the collection.
fn split_export_args(args: &str) -> (&str, Option<&str>) {
    let args = args.trim();
    let whole_is_path = Path::new(args).extension().is_some()
        && !matches!(file_kind(Path::new(args)), FileKind::Unsupported);
    if whole_is_path {
        return (args, None);
    }
    match args.split_once(char::is_whitespace) {
        Some((path, name)) => (path, Some(name.trim()).filter(|n| !n.is_empty())),
        None => (args, None),
    }
}

/// `:export <path> [collection name]`.
pub(super) fn execute_export_command(app: &mut App, args: &str) {
    let (path, coll_name) = split_export_args(args);
    execute_export_collection(app, path, coll_name);
}

/// Export to a file. Format is auto-detected from the file name.
///
/// Supported:
///   .json  -> Postman collection
///   .md    -> Markdown documentation
///   .sh / .curl / .txt -> cURL command
///   .env (also `name.env`, `.env.*`) -> dotenv `KEY=value` lines of the active environment
pub(super) fn execute_export(app: &mut App, path_str: &str) {
    execute_export_collection(app, path_str, None);
}

/// [`execute_export`], exporting collection `coll_name` (see
/// [`export_collection`] for the default).
fn execute_export_collection(app: &mut App, path_str: &str, coll_name: Option<&str>) {
    let expanded = expand_tilde(path_str);

    if expanded.is_dir() {
        app.notify(
            format!(
                "Path is a directory: {}. Add a file name, e.g. api.json",
                expanded.display()
            ),
            NotificationKind::Error,
        );
        return;
    }

    let mut exported_name = None;
    let result: Result<String, String> = match file_kind(&expanded) {
        FileKind::Json => export_collection(app, coll_name).and_then(|coll| {
            exported_name = Some(coll.name.clone());
            hitt_core::postman::export::export_postman_collection(coll).map_err(|e| e.to_string())
        }),
        FileKind::Markdown => export_collection(app, coll_name).map(|coll| {
            exported_name = Some(coll.name.clone());
            hitt_core::exporters::markdown_docs::generate_docs(coll)
        }),
        FileKind::Curl if expanded.extension().is_some() => Ok(active_request_curl(app)),
        FileKind::DotEnv => app
            .active_environment()
            .map(environment_to_dotenv)
            .ok_or_else(|| "No active environment to export".to_string()),
        _ => Err(format!(
            "Unsupported export format: {}. Use .json, .md, .sh, .curl, .txt or .env",
            expanded.display()
        )),
    };

    match result {
        Ok(content) => match std::fs::write(&expanded, &content) {
            Ok(()) => {
                let what = exported_name.map_or_else(String::new, |n| format!("'{n}' "));
                app.notify(
                    format!("Exported {what}to {}", expanded.display()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dotenv_file_names() {
        for name in [".env", "dev.env", ".env.local", "/tmp/x/.env", "PROD.ENV"] {
            assert_eq!(file_kind(Path::new(name)), FileKind::DotEnv, "{name}");
        }
        assert_eq!(file_kind(Path::new("notes.xyz")), FileKind::Unsupported);
        assert_eq!(file_kind(Path::new("request")), FileKind::Curl);
        assert_eq!(file_kind(Path::new("environment.json")), FileKind::Json);
    }

    #[test]
    fn dotenv_values_round_trip() {
        let mut env = hitt_core::core::environment::Environment::new("dev");
        env.add_variable("PLAIN", "abc");
        env.add_variable("SPACED", "hello world");
        env.add_variable("QUOTE", "say \"hi\" # not a comment");
        env.add_variable("EMPTY", "");
        let text = environment_to_dotenv(&env);
        assert!(text.contains("PLAIN=abc\n"));
        assert!(text.contains("SPACED=\"hello world\"\n"));
        let parsed = hitt_core::importers::dotenv::parse_dotenv(&text).unwrap();
        assert_eq!(parsed["PLAIN"], "abc");
        assert_eq!(parsed["SPACED"], "hello world");
        assert_eq!(parsed["QUOTE"], "say \"hi\" # not a comment");
        assert_eq!(parsed["EMPTY"], "");
    }
}
