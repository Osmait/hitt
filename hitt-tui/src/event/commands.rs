use anyhow::Result;

use crate::app::{App, AppMode, ModalKind, NotificationKind, SidebarSection};

use super::actions::{
    disconnect_sse, disconnect_ws, request_delete_collection, request_delete_selected_request,
    request_quit, save_active_request,
};
use super::chain::start_chain_execution;
use super::editors::open_env_editor;
use super::environments::{rename_environment, request_delete_environment};
use super::import_export::{
    active_request_curl, execute_export_command, execute_import, execute_import_chain,
    push_imported_environment,
};
use super::persistence::{
    chain_name_taken, collection_name_taken, environment_name_taken, request_name_taken,
    save_all_collections, save_environment,
};
use super::sidebar::{build_sidebar_items, SidebarItem};
use crate::utils::expand_tilde;

#[allow(clippy::missing_errors_doc, clippy::too_many_lines)]
pub async fn execute_command(app: &mut App, cmd: &str) -> Result<()> {
    let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
    let command = parts[0];
    let args = parts.get(1).copied();

    match command {
        "q" | "quit" => {
            request_quit(app);
        }
        "q!" | "quit!" => {
            app.should_quit = true;
        }
        "env" => {
            if let Some(name) = args.map(str::trim).filter(|a| !a.is_empty()) {
                select_environment(app, name);
            } else {
                let msg = if app.environments.is_empty() {
                    "Usage: :env <name|none>. No environments yet (:newenv <name>)".to_string()
                } else {
                    let names: Vec<&str> =
                        app.environments.iter().map(|e| e.name.as_str()).collect();
                    format!(
                        "Usage: :env <name|none>. Environments: {}",
                        names.join(", ")
                    )
                };
                app.notify(msg, NotificationKind::Info);
            }
        }
        "curl" => {
            // Copy current request as curl (with inherited collection auth)
            let curl = active_request_curl(app);
            match arboard::Clipboard::new().and_then(|mut c| c.set_text(curl)) {
                Ok(()) => app.notify("Copied curl command".into(), NotificationKind::Success),
                Err(e) => app.notify(
                    format!("Clipboard unavailable: {e}"),
                    NotificationKind::Error,
                ),
            }
        }
        "paste-curl" => match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Err(e) => {
                app.notify(
                    format!("Can't read the clipboard: {e}"),
                    NotificationKind::Error,
                );
            }
            Ok(text) => match hitt_core::importers::curl::parse_curl(&text) {
                Ok(request) => {
                    app.tabs
                        .push(crate::app::RequestTab::from_request(request, None));
                    app.switch_tab(app.tabs.len() - 1);
                    app.notify("Imported from curl".into(), NotificationKind::Success);
                }
                Err(e) => {
                    app.notify(
                        format!("Clipboard doesn't contain a curl command: {e}"),
                        NotificationKind::Error,
                    );
                }
            },
        },
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
                        app.config.theme = name.into();
                        app.persist_theme();
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
            // No path: open the import dialog (like Ctrl+O).
            if let Some(path) = args.map(str::trim).filter(|p| !p.is_empty()) {
                execute_import(app, path);
            } else {
                app.modal_input.clear();
                app.mode = AppMode::Modal(ModalKind::Import);
            }
        }
        "export" => {
            // `:export <path> [collection name]`
            if let Some(rest) = args.map(str::trim).filter(|p| !p.is_empty()) {
                execute_export_command(app, rest);
            } else {
                app.modal_input.clear();
                app.mode = AppMode::Modal(ModalKind::Export);
            }
        }
        "loadtest" => {
            start_load_test(app, args).await;
        }
        "run" => {
            super::runner::run_command(app, args).await;
        }
        "runexport" => {
            super::runner::export_run_report(app, args.unwrap_or(""));
        }
        "docs" => {
            // Export current collection as markdown documentation
            if let Some(coll_idx) = app.active_tab().collection_index {
                if let Some(coll) = app.collections.get(coll_idx) {
                    let docs = hitt_core::exporters::markdown_docs::generate_docs(coll);
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
                } else if collection_name_taken(app, name, None) {
                    app.notify(
                        format!("Collection '{name}' already exists"),
                        NotificationKind::Error,
                    );
                } else {
                    let coll = hitt_core::core::collection::Collection::new(name);
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
            request_delete_selected_request(app);
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
                                apply_client_setting(app, format!("timeout = {ms}ms"));
                                app.persist_config_change(|c| c.timeout_ms = ms);
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
                                apply_client_setting(app, format!("follow_redirects = {b}"));
                                app.persist_config_change(|c| c.follow_redirects = b);
                            } else {
                                app.notify(format!("Invalid bool: {val}"), NotificationKind::Error);
                            }
                        }
                        "verify_ssl" => {
                            if let Ok(b) = val.parse::<bool>() {
                                app.config.verify_ssl = b;
                                apply_client_setting(app, format!("verify_ssl = {b}"));
                                app.persist_config_change(|c| c.verify_ssl = b);
                            } else {
                                app.notify(format!("Invalid bool: {val}"), NotificationKind::Error);
                            }
                        }
                        "history_limit" => {
                            if let Some(n) = val.parse::<usize>().ok().filter(|n| *n > 0) {
                                app.config.history_limit = n;
                                // Enforce the new limit right away (and on disk).
                                app.history.set_max_entries(n);
                                app.save_history();
                                app.persist_config_change(|c| c.history_limit = n);
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
                        "persist_history" => {
                            if let Ok(b) = val.parse::<bool>() {
                                set_persist_history(app, b);
                                app.persist_config_change(|c| c.persist_history = b);
                                app.notify(
                                    format!("persist_history = {b}"),
                                    NotificationKind::Success,
                                );
                            } else {
                                app.notify(format!("Invalid bool: {val}"), NotificationKind::Error);
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
                                app.persist_theme();
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
                        match hitt_core::importers::dotenv::dotenv_to_environment(
                            env_name, &content,
                        ) {
                            Ok(env) => {
                                app.session_env_ids.insert(env.id);
                                // Same "Name (2)" renaming as `:import`.
                                let name = push_imported_environment(app, env);
                                app.active_env = Some(app.environments.len() - 1);
                                // .env files are session-only layers: not persisted.
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
                    open_realtime_tab(app, hitt_core::core::request::Protocol::WebSocket, url);
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
                    open_realtime_tab(app, hitt_core::core::request::Protocol::Sse, url);
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
                let found = find_chain_named(app, name)
                    .map(|(ci, chain_i)| (app.collections[ci].chains[chain_i].clone(), ci));
                if let Some((chain, coll_idx)) = found {
                    app.selected_chain = Some(chain.id);
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
                    // Add to first collection (or the collection of the active tab if any)
                    let coll_idx = app.active_tab().collection_index.unwrap_or(0);
                    let coll_idx = coll_idx.min(app.collections.len() - 1);
                    if chain_name_taken(app, coll_idx, name) {
                        app.notify(
                            format!(
                                "Chain '{name}' already exists in '{}'",
                                app.collections[coll_idx].name
                            ),
                            NotificationKind::Error,
                        );
                        return Ok(());
                    }
                    let chain = hitt_core::core::chain::RequestChain::new(name);
                    // New chains become the `:addstep` target.
                    app.selected_chain = Some(chain.id);
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
            add_chain_step(app, args.unwrap_or(""));
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
                } else if environment_name_taken(app, name) {
                    // Saving would overwrite the existing environment's file.
                    app.notify(
                        format!("Environment '{name}' already exists"),
                        NotificationKind::Error,
                    );
                } else {
                    let env = hitt_core::core::environment::Environment::new(name);
                    app.environments.push(env);
                    app.active_env = Some(app.environments.len() - 1);
                    app.remember_active_env();
                    save_environment(app, app.environments.len() - 1);
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
                    if environment_name_taken(app, &new_name) {
                        app.notify(
                            format!("Environment '{new_name}' already exists"),
                            NotificationKind::Error,
                        );
                        return Ok(());
                    }
                    let mut dup = source;
                    dup.id = uuid::Uuid::new_v4();
                    dup.name.clone_from(&new_name);
                    app.environments.push(dup);
                    app.active_env = Some(app.environments.len() - 1);
                    app.remember_active_env();
                    save_environment(app, app.environments.len() - 1);
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
        "renameenv" => match (app.active_env, args.map(str::trim)) {
            (_, None | Some("")) => {
                app.notify(
                    "Usage: :renameenv <new name>".into(),
                    NotificationKind::Warning,
                );
            }
            (None, _) => {
                app.notify(
                    "No active environment to rename".into(),
                    NotificationKind::Warning,
                );
            }
            (Some(idx), Some(name)) => {
                if let Err(e) = rename_environment(app, idx, name) {
                    app.notify(e, NotificationKind::Error);
                }
            }
        },
        "delenv" => {
            let target = match args.map(str::trim).filter(|n| !n.is_empty()) {
                Some(name) => app
                    .environments
                    .iter()
                    .position(|e| e.name == name)
                    .ok_or_else(|| {
                        (
                            format!("Environment '{name}' not found"),
                            NotificationKind::Error,
                        )
                    }),
                None => app.active_env.ok_or_else(|| {
                    (
                        "No active environment to delete".to_string(),
                        NotificationKind::Warning,
                    )
                }),
            };
            match target {
                Ok(idx) => request_delete_environment(app, idx),
                Err((msg, kind)) => app.notify(msg, kind),
            }
        }
        "delcol" => {
            if let Some(name) = args {
                let name = name.trim();
                if let Some(pos) = app.collections.iter().position(|c| c.name == name) {
                    request_delete_collection(app, pos);
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
                        request_delete_collection(app, coll_idx);
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
                            let verb = if let Some(existing) =
                                coll.variables.iter_mut().find(|v| v.key == key)
                            {
                                existing.value = val.to_string();
                                "Updated"
                            } else {
                                coll.variables
                                    .push(hitt_core::core::request::KeyValuePair::new(key, val));
                                "Added"
                            };
                            save_all_collections(app);
                            app.notify(
                                format!("{verb} variable '{key}' = '{val}'"),
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
        "envedit" => {
            open_env_editor(app, crate::editor::VarScope::Environment);
        }
        "colvars" => {
            open_env_editor(app, crate::editor::VarScope::Collection);
        }
        "clearhistory" => {
            app.clear_history();
            app.notify("History cleared".into(), NotificationKind::Success);
        }
        "replay" => {
            // `:replay [n]`: n = 1 is the newest entry; default = selected in sidebar History.
            if app.history.is_empty() {
                app.notify("History is empty".into(), NotificationKind::Warning);
                return Ok(());
            }
            let index = match args.map(str::trim).filter(|a| !a.is_empty()) {
                Some(n) => match n.parse::<usize>() {
                    Ok(n) if n >= 1 => Some(n - 1),
                    _ => None,
                },
                None if app.sidebar_state.section == SidebarSection::History => {
                    Some(app.sidebar_state.selected)
                }
                None => Some(0),
            };
            match index {
                Some(i) if app.replay_history_entry(i).await? => {}
                _ => {
                    app.notify(
                        format!("Usage: :replay [1-{}]", app.history.len()),
                        NotificationKind::Warning,
                    );
                }
            }
        }
        "rename" => {
            if let Some(name) = args {
                let name = name.trim();
                if name.is_empty() {
                    app.notify("Usage: :rename <name>".into(), NotificationKind::Warning);
                } else if app.active_tab().collection_index.is_some_and(|ci| {
                    request_name_taken(app, ci, name, Some(app.active_tab().request.id))
                }) {
                    app.notify(
                        format!("A request named '{name}' already exists in this collection"),
                        NotificationKind::Error,
                    );
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

/// `:env <name>` (exact name first, then case-insensitive) or `:env none`.
fn select_environment(app: &mut App, name: &str) {
    let idx = app
        .environments
        .iter()
        .position(|e| e.name == name)
        .or_else(|| {
            app.environments
                .iter()
                .position(|e| e.name.eq_ignore_ascii_case(name))
        });
    match idx {
        Some(idx) => {
            app.active_env = Some(idx);
            app.remember_active_env();
            let name = app.environments[idx].name.clone();
            app.notify(format!("Environment: {name}"), NotificationKind::Info);
        }
        None if name.eq_ignore_ascii_case("none") => {
            app.active_env = None;
            app.remember_active_env();
            app.notify("Environment: No Env".into(), NotificationKind::Info);
        }
        None => app.notify(
            format!("Environment '{name}' not found"),
            NotificationKind::Error,
        ),
    }
}

/// `:set persist_history <b>`. Turning it off leaves the history file alone;
/// turning it on merges this session's entries with the saved ones (which
/// were not loaded while persistence was off) and writes the file.
fn set_persist_history(app: &mut App, on: bool) {
    let was_on = app.config.persist_history;
    app.config.persist_history = on;
    if !on || was_on {
        return;
    }
    let limit = app.config.history_limit;
    let saved = match hitt_core::storage::history_store::load_history(&app.history_path(), limit) {
        Ok(saved) => saved,
        Err(e) => {
            // Don't overwrite a file we couldn't read.
            tracing::warn!("Not saving history, file unreadable: {e:#}");
            return;
        }
    };
    let mut merged: Vec<_> = app.history.entries().to_vec();
    for entry in saved {
        if !merged.iter().any(|e| e.id == entry.id) {
            merged.push(entry);
        }
    }
    app.history = hitt_core::core::history::HistoryStore::with_entries(limit, merged);
    app.save_history();
}

/// Rebuild the HTTP client after a client-level `:set` and report the result.
fn apply_client_setting(app: &mut App, applied: String) {
    match app.rebuild_http_client() {
        Ok(()) => app.notify(applied, NotificationKind::Success),
        Err(e) => app.notify(
            format!("{applied} (failed to rebuild HTTP client: {e})"),
            NotificationKind::Error,
        ),
    }
}

/// Point a tab at a WS/SSE `url`: reuse the current tab only if it is blank,
/// otherwise open a new one so unsaved work isn't overwritten.
fn open_realtime_tab(app: &mut App, protocol: hitt_core::core::request::Protocol, url: &str) {
    if !app.active_tab().is_blank() {
        app.new_tab();
    }
    let tab = app.active_tab_mut();
    tab.request.protocol = protocol;
    tab.request.url = url.to_string();
}

/// `:loadtest <n> <c>` — runs in the background and notifies on completion.
async fn start_load_test(app: &mut App, args: Option<&str>) {
    const USAGE: &str = "Usage: :loadtest <requests> <concurrency>";
    let parts: Vec<&str> = args.unwrap_or("").split_whitespace().collect();
    let (Some(n), Some(c)) = (
        parts.first().and_then(|p| p.parse::<u32>().ok()),
        parts.get(1).and_then(|p| p.parse::<u32>().ok()),
    ) else {
        app.notify(USAGE.into(), NotificationKind::Warning);
        return;
    };
    if n == 0 || c == 0 {
        app.notify(USAGE.into(), NotificationKind::Warning);
        return;
    }
    if app.active_tab().request.url.trim().is_empty() {
        app.notify(
            "Enter a URL before running a load test".into(),
            NotificationKind::Warning,
        );
        return;
    }
    if app.load_test_running {
        app.notify(
            "A load test is already running".into(),
            NotificationKind::Warning,
        );
        return;
    }

    let config =
        hitt_core::testing::load_test::LoadTestConfig::new(app.active_tab().request.id, n, c);
    let request = app.active_tab().request.clone();
    let resolver = app.build_resolver();
    app.load_test_running = true;
    app.notify(
        format!("Running load test: {n} requests, {c} concurrency"),
        NotificationKind::Info,
    );

    let run = async move {
        match hitt_core::testing::load_test::run_load_test(&config, &request, &resolver).await {
            Ok(result) => (
                format!(
                    "Load test complete: {}/{} ok, {:.1} rps, p50={}ms p99={}ms",
                    result.successful,
                    result.total_requests,
                    result.rps,
                    result.latency.median.as_millis(),
                    result.latency.p99.as_millis(),
                ),
                NotificationKind::Success,
            ),
            Err(e) => (format!("Load test failed: {e}"), NotificationKind::Error),
        }
    };

    if let Some(tx) = app.event_sender.clone() {
        tokio::spawn(async move {
            let (msg, kind) = run.await;
            let _ = tx.send(super::AppEvent::LoadTestComplete(msg, kind));
        });
    } else {
        // No event loop attached (tests / headless use): run inline.
        let (msg, kind) = run.await;
        app.load_test_running = false;
        app.notify(msg, kind);
    }
}

/// `(coll_idx, chain_idx)` of the chain named `name` (case-insensitive).
///
/// Chain names are unique within a collection, but the same name may exist in
/// several collections (or twice in data saved by older versions), so prefer
/// the active chain, then the active tab's collection, then the first match.
fn find_chain_named(app: &App, name: &str) -> Option<(usize, usize)> {
    let matches =
        |c: &hitt_core::core::chain::RequestChain| c.name.trim().eq_ignore_ascii_case(name.trim());
    let all = || {
        app.collections.iter().enumerate().flat_map(|(ci, coll)| {
            coll.chains
                .iter()
                .enumerate()
                .map(move |(chain_i, chain)| (ci, chain_i, chain))
        })
    };
    let active_coll = app.active_tab().collection_index;
    all()
        .find(|(_, _, c)| Some(c.id) == app.selected_chain && matches(c))
        .or_else(|| all().find(|(ci, _, c)| Some(*ci) == active_coll && matches(c)))
        .or_else(|| all().find(|(_, _, c)| matches(c)))
        .map(|(ci, chain_i, _)| (ci, chain_i))
}

/// `:addstep <request> [--chain <name>]` — append a step to a chain.
///
/// Targets `--chain <name>` if given, otherwise the active chain (last created,
/// run or imported). Only requests from the chain's own collection are allowed.
fn add_chain_step(app: &mut App, args: &str) {
    const USAGE: &str = "Usage: :addstep <request_name> [--chain <chain_name>]";
    let (req_name, chain_name) = match args.split_once("--chain") {
        Some((r, c)) => (r.trim(), Some(c.trim())),
        None => (args.trim(), None),
    };
    if req_name.is_empty() || chain_name == Some("") {
        app.notify(USAGE.into(), NotificationKind::Warning);
        return;
    }

    let target = if let Some(chain_name) = chain_name {
        let found = find_chain_named(app, chain_name);
        if found.is_none() {
            app.notify(
                format!("Chain '{chain_name}' not found"),
                NotificationKind::Error,
            );
            return;
        }
        found
    } else if let Some(id) = app.selected_chain {
        let found = app.collections.iter().enumerate().find_map(|(ci, coll)| {
            coll.chains
                .iter()
                .position(|c| c.id == id)
                .map(|chain_i| (ci, chain_i))
        });
        if found.is_none() {
            app.notify(
                "Active chain no longer exists. Use --chain <name>".into(),
                NotificationKind::Error,
            );
            return;
        }
        found
    } else {
        app.notify(
            "No active chain. Use :newchain <name> or --chain <name>".into(),
            NotificationKind::Warning,
        );
        return;
    };
    let Some((ci, chain_i)) = target else {
        return;
    };

    let coll = &app.collections[ci];
    let chain_name = coll.chains[chain_i].name.clone();
    let Some(request_id) = coll
        .all_requests()
        .into_iter()
        .find(|r| r.name == req_name)
        .map(|r| r.id)
    else {
        let coll_name = coll.name.clone();
        app.notify(
            format!(
                "Request '{req_name}' not found in '{coll_name}' (chain '{chain_name}' can only use requests from its collection)"
            ),
            NotificationKind::Error,
        );
        return;
    };

    let chain = &mut app.collections[ci].chains[chain_i];
    chain.add_step(request_id);
    app.selected_chain = Some(chain.id);
    save_all_collections(app);
    app.notify(
        format!("Added '{req_name}' to chain '{chain_name}'"),
        NotificationKind::Success,
    );
}
