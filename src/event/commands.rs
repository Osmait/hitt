use anyhow::Result;
use uuid::Uuid;

use crate::app::{App, AppMode, ModalKind, NotificationKind, SidebarSection};

use super::actions::{
    delete_selected_request, disconnect_sse, disconnect_ws, remove_collection,
    save_active_request,
};
use super::chain::start_chain_execution;
use super::import_export::{execute_export, execute_import, execute_import_chain};
use super::persistence::save_all_collections;
use super::sidebar::{build_sidebar_items, SidebarItem};
use crate::utils::expand_tilde;

#[allow(clippy::missing_errors_doc, clippy::too_many_lines)]
pub async fn execute_command(app: &mut App, cmd: &str) -> Result<()> {
    let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
    let command = parts[0];
    let args = parts.get(1).copied();

    match command {
        "q" | "quit" => {
            app.should_quit = true;
        }
        "env" => {
            if let Some(name) = args {
                if let Some(idx) = app.environments.iter().position(|e| e.name == name) {
                    app.active_env = Some(idx);
                    app.notify(format!("Environment: {name}"), NotificationKind::Info);
                } else {
                    app.notify(
                        format!("Environment '{name}' not found"),
                        NotificationKind::Error,
                    );
                }
            }
        }
        "curl" => {
            // Copy current request as curl
            let tab = app.active_tab();
            let resolver = app.build_resolver();
            let curl = crate::exporters::curl::to_curl(&tab.request, &resolver);
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&curl);
                app.notify("Copied curl command".into(), NotificationKind::Success);
            }
        }
        "paste-curl" => {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(text) = clipboard.get_text() {
                    match crate::importers::curl::parse_curl(&text) {
                        Ok(request) => {
                            app.tabs
                                .push(crate::app::RequestTab::from_request(request, None));
                            app.active_tab = app.tabs.len() - 1;
                            app.notify("Imported from curl".into(), NotificationKind::Success);
                        }
                        Err(e) => {
                            app.notify(
                                format!("Failed to parse curl: {e}"),
                                NotificationKind::Error,
                            );
                        }
                    }
                }
            }
        }
        "theme" => {
            if let Some(name) = args {
                match crate::ui::theme::Theme::load(name) {
                    Ok(mut theme) => {
                        if let Some(ref colors) = app.config.colors {
                            theme.apply_overrides(colors);
                        }
                        if let Some(ref borders) = app.config.borders {
                            theme.apply_border_overrides(borders);
                        }
                        app.theme = theme;
                        app.notify(format!("Theme: {name}"), NotificationKind::Info);
                    }
                    Err(_) => {
                        app.notify(format!("Theme '{name}' not found"), NotificationKind::Error);
                    }
                }
            } else {
                // No args — open theme picker modal
                app.theme_before_preview = Some(app.theme.clone());
                app.theme_picker_selected = crate::ui::theme::AVAILABLE_THEMES
                    .iter()
                    .position(|&t| t == app.theme.name)
                    .unwrap_or(0);
                app.mode = AppMode::Modal(ModalKind::ThemePicker);
            }
        }
        "import" => {
            if let Some(path) = args {
                execute_import(app, path.trim());
            } else {
                app.modal_input.clear();
                app.mode = AppMode::Modal(ModalKind::Import);
            }
        }
        "export" => {
            if let Some(path) = args {
                execute_export(app, path.trim());
            } else {
                app.modal_input.clear();
                app.mode = AppMode::Modal(ModalKind::Export);
            }
        }
        "loadtest" => {
            if let Some(args_str) = args {
                let parts: Vec<&str> = args_str.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let (Ok(n), Ok(c)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        let config = crate::testing::load_test::LoadTestConfig::new(
                            app.active_tab().request.id,
                            n,
                            c,
                        );
                        let request = app.active_tab().request.clone();
                        let resolver = app.build_resolver();
                        app.notify(
                            format!("Running load test: {n} requests, {c} concurrency"),
                            NotificationKind::Info,
                        );
                        match crate::testing::load_test::run_load_test(&config, &request, &resolver)
                            .await
                        {
                            Ok(result) => {
                                app.notify(
                                    format!(
                                        "Load test complete: {}/{} ok, {:.1} rps, p50={}ms p99={}ms",
                                        result.successful,
                                        result.total_requests,
                                        result.rps,
                                        result.latency.median.as_millis(),
                                        result.latency.p99.as_millis(),
                                    ),
                                    NotificationKind::Success,
                                );
                            }
                            Err(e) => {
                                app.notify(
                                    format!("Load test failed: {e}"),
                                    NotificationKind::Error,
                                );
                            }
                        }
                    }
                }
            }
        }
        "docs" => {
            // Export current collection as markdown documentation
            if let Some(coll_idx) = app.active_tab().collection_index {
                if let Some(coll) = app.collections.get(coll_idx) {
                    let docs = crate::exporters::markdown_docs::generate_docs(coll);
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(&docs);
                        app.notify(
                            "Markdown docs copied to clipboard".into(),
                            NotificationKind::Success,
                        );
                    }
                }
            } else {
                app.notify("No collection selected".into(), NotificationKind::Warning);
            }
        }
        "help" => {
            app.mode = AppMode::Modal(ModalKind::Help);
        }
        "newcol" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :newcol <name>".into(), NotificationKind::Warning);
                } else {
                    let coll = crate::core::collection::Collection::new(name);
                    app.collections.push(coll);
                    save_all_collections(app);
                    app.notify(
                        format!("Created collection '{name}'"),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify("Usage: :newcol <name>".into(), NotificationKind::Warning);
            }
        }
        "save" => {
            save_active_request(app);
        }
        "delreq" => {
            delete_selected_request(app);
        }
        "set" => {
            if let Some(rest) = args {
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    app.notify(
                        "Usage: :set <key> <value>".into(),
                        NotificationKind::Warning,
                    );
                } else {
                    let key = parts[0];
                    let val = parts[1].trim();
                    match key {
                        "timeout" => {
                            if let Ok(ms) = val.parse::<u64>() {
                                app.config.timeout_ms = ms;
                                app.notify(format!("timeout = {ms}ms"), NotificationKind::Success);
                            } else {
                                app.notify(
                                    format!("Invalid timeout value: {val}"),
                                    NotificationKind::Error,
                                );
                            }
                        }
                        "follow_redirects" => {
                            if let Ok(b) = val.parse::<bool>() {
                                app.config.follow_redirects = b;
                                app.notify(
                                    format!("follow_redirects = {b}"),
                                    NotificationKind::Success,
                                );
                            } else {
                                app.notify(format!("Invalid bool: {val}"), NotificationKind::Error);
                            }
                        }
                        "verify_ssl" => {
                            if let Ok(b) = val.parse::<bool>() {
                                app.config.verify_ssl = b;
                                app.notify(format!("verify_ssl = {b}"), NotificationKind::Success);
                            } else {
                                app.notify(format!("Invalid bool: {val}"), NotificationKind::Error);
                            }
                        }
                        "vim_mode" => {
                            if let Ok(b) = val.parse::<bool>() {
                                app.config.vim_mode = b;
                                app.notify(format!("vim_mode = {b}"), NotificationKind::Success);
                            } else {
                                app.notify(format!("Invalid bool: {val}"), NotificationKind::Error);
                            }
                        }
                        "history_limit" => {
                            if let Ok(n) = val.parse::<usize>() {
                                app.config.history_limit = n;
                                app.notify(
                                    format!("history_limit = {n}"),
                                    NotificationKind::Success,
                                );
                            } else {
                                app.notify(
                                    format!("Invalid number: {val}"),
                                    NotificationKind::Error,
                                );
                            }
                        }
                        "theme" => match crate::ui::theme::Theme::load(val) {
                            Ok(mut theme) => {
                                if let Some(ref colors) = app.config.colors {
                                    theme.apply_overrides(colors);
                                }
                                if let Some(ref borders) = app.config.borders {
                                    theme.apply_border_overrides(borders);
                                }
                                app.config.theme = val.into();
                                app.theme = theme;
                                app.notify(format!("theme = {val}"), NotificationKind::Success);
                            }
                            Err(_) => {
                                app.notify(
                                    format!("Theme '{val}' not found"),
                                    NotificationKind::Error,
                                );
                            }
                        },
                        _ => {
                            app.notify(format!("Unknown setting: {key}"), NotificationKind::Error);
                        }
                    }
                }
            } else {
                app.notify(
                    "Usage: :set <key> <value>".into(),
                    NotificationKind::Warning,
                );
            }
        }
        "env-file" => {
            if let Some(path_str) = args {
                let path_str = path_str.trim();
                let expanded = expand_tilde(path_str);
                match std::fs::read_to_string(&expanded) {
                    Ok(content) => {
                        let env_name = expanded
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(".env");
                        match crate::importers::dotenv::dotenv_to_environment(env_name, &content) {
                            Ok(env) => {
                                let name = env.name.clone();
                                app.environments.push(env);
                                app.active_env = Some(app.environments.len() - 1);
                                app.notify(
                                    format!("Loaded environment '{name}'"),
                                    NotificationKind::Success,
                                );
                            }
                            Err(e) => {
                                app.notify(
                                    format!("Failed to parse env file: {e}"),
                                    NotificationKind::Error,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        app.notify(format!("Cannot read file: {e}"), NotificationKind::Error);
                    }
                }
            } else {
                app.notify("Usage: :env-file <path>".into(), NotificationKind::Warning);
            }
        }
        "diff" => {
            app.mode = AppMode::Modal(ModalKind::DiffSelector);
        }
        "ws" => {
            if let Some(url) = args {
                let url = url.trim();
                if url.is_empty() {
                    app.notify("Usage: :ws <url>".into(), NotificationKind::Warning);
                } else {
                    app.active_tab_mut().request.protocol =
                        crate::core::request::Protocol::WebSocket;
                    app.active_tab_mut().request.url = url.to_string();
                    app.send_request().await?;
                }
            } else {
                app.notify("Usage: :ws <url>".into(), NotificationKind::Warning);
            }
        }
        "sse" => {
            if let Some(url) = args {
                let url = url.trim();
                if url.is_empty() {
                    app.notify("Usage: :sse <url>".into(), NotificationKind::Warning);
                } else {
                    app.active_tab_mut().request.protocol = crate::core::request::Protocol::Sse;
                    app.active_tab_mut().request.url = url.to_string();
                    app.send_request().await?;
                }
            } else {
                app.notify("Usage: :sse <url>".into(), NotificationKind::Warning);
            }
        }
        "ws-disconnect" => {
            disconnect_ws(app);
        }
        "sse-disconnect" => {
            disconnect_sse(app);
        }
        "chain" => {
            if let Some(name) = args {
                let name = name.trim();
                // Search all collections for a chain matching name
                let mut found = None;
                for (coll_idx, coll) in app.collections.iter().enumerate() {
                    if let Some(chain) = coll.chains.iter().find(|c| c.name == name) {
                        found = Some((chain.clone(), coll_idx));
                        break;
                    }
                }
                if let Some((chain, coll_idx)) = found {
                    start_chain_execution(app, chain, coll_idx);
                } else {
                    app.notify(
                        format!("Chain '{name}' not found"),
                        NotificationKind::Warning,
                    );
                }
            } else {
                app.notify("Usage: :chain <name>".into(), NotificationKind::Warning);
            }
        }
        "newchain" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :newchain <name>".into(), NotificationKind::Warning);
                } else if app.collections.is_empty() {
                    app.notify(
                        "No collections. Use :newcol <name> first".into(),
                        NotificationKind::Warning,
                    );
                } else {
                    let chain = crate::core::chain::RequestChain::new(name);
                    // Add to first collection (or the collection of the active tab if any)
                    let coll_idx = app.active_tab().collection_index.unwrap_or(0);
                    let coll_idx = coll_idx.min(app.collections.len() - 1);
                    app.collections[coll_idx].chains.push(chain);
                    save_all_collections(app);
                    app.notify(
                        format!(
                            "Created chain '{}' in '{}'",
                            name, app.collections[coll_idx].name
                        ),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify("Usage: :newchain <name>".into(), NotificationKind::Warning);
            }
        }
        "addstep" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify(
                        "Usage: :addstep <request_name>".into(),
                        NotificationKind::Warning,
                    );
                } else {
                    // Find request by name across collections
                    let found_req: Option<Uuid> = app
                        .collections
                        .iter()
                        .flat_map(|c| c.all_requests())
                        .find(|r| r.name == name)
                        .map(|r| r.id);

                    if let Some(request_id) = found_req {
                        // Find the last chain in any collection and add step
                        let mut target: Option<(usize, usize)> = None;
                        for (ci, coll) in app.collections.iter().enumerate().rev() {
                            if !coll.chains.is_empty() {
                                target = Some((ci, coll.chains.len() - 1));
                                break;
                            }
                        }
                        if let Some((ci, chain_i)) = target {
                            app.collections[ci].chains[chain_i].add_step(request_id);
                            let chain_name = app.collections[ci].chains[chain_i].name.clone();
                            save_all_collections(app);
                            app.notify(
                                format!("Added '{name}' to chain '{chain_name}'"),
                                NotificationKind::Success,
                            );
                        } else {
                            app.notify(
                                "No chains exist. Use :newchain <name> first".into(),
                                NotificationKind::Warning,
                            );
                        }
                    } else {
                        app.notify(
                            format!("Request '{name}' not found"),
                            NotificationKind::Warning,
                        );
                    }
                }
            } else {
                app.notify(
                    "Usage: :addstep <request_name>".into(),
                    NotificationKind::Warning,
                );
            }
        }
        "importchain" => {
            if let Some(path) = args {
                let path_str = path.trim();
                if path_str.is_empty() {
                    app.notify(
                        "Usage: :importchain <path>".into(),
                        NotificationKind::Warning,
                    );
                } else if app.collections.is_empty() {
                    app.notify(
                        "No collections. Use :newcol <name> first".into(),
                        NotificationKind::Warning,
                    );
                } else {
                    execute_import_chain(app, path_str);
                }
            } else {
                app.notify(
                    "Usage: :importchain <path>".into(),
                    NotificationKind::Warning,
                );
            }
        }
        "proxy" => {
            app.notify(
                "Proxy inspector not yet available".into(),
                NotificationKind::Warning,
            );
        }
        "newenv" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :newenv <name>".into(), NotificationKind::Warning);
                } else {
                    let env = crate::core::environment::Environment::new(name);
                    app.environments.push(env);
                    app.active_env = Some(app.environments.len() - 1);
                    app.notify(
                        format!("Created environment '{name}'"),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify("Usage: :newenv <name>".into(), NotificationKind::Warning);
            }
        }
        "dupenv" => {
            if let Some(idx) = app.active_env {
                if let Some(source) = app.environments.get(idx).cloned() {
                    let new_name = if let Some(name) = args {
                        let name = name.trim();
                        if name.is_empty() {
                            format!("{} (copy)", source.name)
                        } else {
                            name.to_string()
                        }
                    } else {
                        format!("{} (copy)", source.name)
                    };
                    let mut dup = source;
                    dup.id = uuid::Uuid::new_v4();
                    dup.name.clone_from(&new_name);
                    app.environments.push(dup);
                    app.active_env = Some(app.environments.len() - 1);
                    app.notify(
                        format!("Duplicated environment as '{new_name}'"),
                        NotificationKind::Success,
                    );
                }
            } else {
                app.notify(
                    "No active environment to duplicate".into(),
                    NotificationKind::Warning,
                );
            }
        }
        "delcol" => {
            if let Some(name) = args {
                let name = name.trim();
                if let Some(pos) = app.collections.iter().position(|c| c.name == name) {
                    remove_collection(app, pos);
                } else {
                    app.notify(
                        format!("Collection '{name}' not found"),
                        NotificationKind::Error,
                    );
                }
            } else {
                // Try to delete the collection selected in sidebar
                if app.sidebar_state.section == SidebarSection::Collections {
                    let items = build_sidebar_items(app);
                    if let Some(SidebarItem::Collection { coll_idx, .. }) =
                        items.get(app.sidebar_state.selected)
                    {
                        let coll_idx = *coll_idx;
                        remove_collection(app, coll_idx);
                    } else {
                        app.notify(
                            "Select a collection in the sidebar or provide a name".into(),
                            NotificationKind::Warning,
                        );
                    }
                } else {
                    app.notify(
                        "Usage: :delcol <name> or select a collection in the sidebar".into(),
                        NotificationKind::Warning,
                    );
                }
            }
        }
        "addvar" => {
            if let Some(rest) = args {
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    app.notify(
                        "Usage: :addvar <key> <value>".into(),
                        NotificationKind::Warning,
                    );
                } else {
                    let key = parts[0].trim();
                    let val = parts[1].trim();
                    // Add to the active tab's collection
                    if let Some(coll_idx) = app.active_tab().collection_index {
                        if let Some(coll) = app.collections.get_mut(coll_idx) {
                            coll.variables
                                .push(crate::core::request::KeyValuePair::new(key, val));
                            save_all_collections(app);
                            app.notify(
                                format!("Added variable '{key}' = '{val}'"),
                                NotificationKind::Success,
                            );
                        }
                    } else {
                        app.notify(
                            "No collection selected. Save to a collection first".into(),
                            NotificationKind::Warning,
                        );
                    }
                }
            } else {
                app.notify(
                    "Usage: :addvar <key> <value>".into(),
                    NotificationKind::Warning,
                );
            }
        }
        "clearhistory" => {
            app.history.clear();
            app.notify("History cleared".into(), NotificationKind::Success);
        }
        "rename" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :rename <name>".into(), NotificationKind::Warning);
                } else {
                    app.active_tab_mut().request.name = name.to_string();
                    app.active_tab_mut().dirty = true;
                    app.notify(format!("Renamed to '{name}'"), NotificationKind::Success);
                }
            } else {
                app.notify("Usage: :rename <name>".into(), NotificationKind::Warning);
            }
        }
        _ => {
            app.notify(
                format!("Unknown command: {command}"),
                NotificationKind::Error,
            );
        }
    }
    Ok(())
}
