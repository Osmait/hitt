use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use hitt_core::core;
use hitt_core::importers;
use hitt_core::postman;
use hitt_core::storage::{self, config::AppConfig};
use hitt_tui::app::{self, App};
use hitt_tui::event::{handle_event, AppEvent, EventHandler};
use hitt_tui::ui;

#[derive(Parser, Debug)]
#[command(
    name = "hitt",
    version,
    about = "A fast, beautiful TUI alternative to Postman"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<hitt_tui::cli::Commands>,

    /// Collection file to open
    #[arg(short, long)]
    collection: Option<PathBuf>,

    /// Environment file to load
    #[arg(short = 'E', long)]
    environment: Option<PathBuf>,

    /// .env file(s) to load
    #[arg(long = "env")]
    env_files: Vec<PathBuf>,

    /// Theme to use
    #[arg(short, long)]
    theme: Option<String>,

    /// Import a file (auto-detect format)
    #[arg(short, long)]
    import: Option<PathBuf>,

    /// Start with a URL
    #[arg(short, long)]
    url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // If a subcommand was given, run CLI mode (no TUI)
    if let Some(cmd) = cli.command {
        let config = AppConfig::load().unwrap_or_default();
        return hitt_tui::cli::run(cmd, &config).await;
    }

    // Initialize logging to file
    let log_file = std::fs::File::create("/tmp/hitt.log").ok();
    if let Some(file) = log_file {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(file)
            .init();
    }

    // Load config
    let mut config = AppConfig::load().unwrap_or_default();

    // Apply CLI overrides
    if let Some(theme) = &cli.theme {
        config.theme = theme.as_str().into();
    }

    // Initialize app
    let mut app = App::new(config)?;
    app.config_path = Some(storage::config::config_file());

    // Load theme and apply config overrides
    if let Ok(mut theme) = ui::theme::Theme::load(app.config.theme.as_str()) {
        if let Some(ref colors) = app.config.colors {
            theme.apply_overrides(colors);
        }
        if let Some(ref borders) = app.config.borders {
            theme.apply_border_overrides(borders);
        }
        app.theme = theme;
    }

    // Load collection if specified
    if let Some(path) = &cli.collection {
        match storage::collections_store::CollectionsStore::new(app.config.collections_dir.clone())
        {
            Ok(store) => {
                if let Ok(collection) = store.load_collection(path) {
                    app.collections.push(collection);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to init collections store: {}", e);
            }
        }
    }

    // Load saved environments
    match storage::collections_store::CollectionsStore::new(app.config.collections_dir.clone()) {
        Ok(store) => match store.load_all_environments() {
            Ok(envs) => app.environments = envs,
            Err(e) => tracing::warn!("Failed to load saved environments: {}", e),
        },
        Err(e) => tracing::warn!("Failed to init collections store: {}", e),
    }
    app.restore_last_environment();

    // Restore request history from the previous session.
    app.load_history();

    // Load environment if specified
    if let Some(path) = &cli.environment {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // Try Postman environment format first, then our native format
                let env = postman::env_import::import_postman_environment(&content)
                    .ok()
                    .or_else(|| {
                        serde_json::from_str::<core::environment::Environment>(&content).ok()
                    });
                if let Some(env) = env {
                    // Replace a saved environment with the same name instead of duplicating it
                    let idx =
                        if let Some(i) = app.environments.iter().position(|e| e.name == env.name) {
                            app.environments[i] = env;
                            i
                        } else {
                            app.environments.push(env);
                            app.environments.len() - 1
                        };
                    app.active_env = Some(idx);
                } else {
                    tracing::warn!("Failed to parse environment file: {}", path.display());
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load environment: {}", e);
            }
        }
    }

    // Load .env files
    for env_path in &cli.env_files {
        if let Ok(vars) = importers::dotenv::parse_dotenv_file(env_path) {
            let name = env_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(".env")
                .to_string();
            let mut env = core::environment::Environment::new(name);
            for (k, v) in vars {
                env.add_variable(k, v);
            }
            app.session_env_ids.insert(env.id);
            app.environments.push(env);
            if app.active_env.is_none() {
                app.active_env = Some(app.environments.len() - 1);
            }
        }
    }

    // Import file if specified
    if let Some(path) = &cli.import {
        if let Ok(content) = std::fs::read_to_string(path) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "har" => {
                    if let Ok(collection) = importers::har::import_har(&content) {
                        app.collections.push(collection);
                    }
                }
                "yaml" | "yml" => {
                    if let Ok(collection) = importers::openapi::import_openapi(&content) {
                        app.collections.push(collection);
                    }
                }
                _ => {
                    // Try Postman collection
                    if let Ok(collection) = postman::import::import_postman_collection(&content) {
                        app.collections.push(collection);
                    }
                }
            }
        }
    }

    // Set initial URL if provided
    if let Some(url) = &cli.url {
        app.active_tab_mut().request.url.clone_from(url);
        app.focus = app::FocusArea::UrlBar;
    }

    // Load saved collections
    if app.collections.is_empty() {
        if let Ok(store) =
            storage::collections_store::CollectionsStore::new(app.config.collections_dir.clone())
        {
            if let Ok(collections) = store.load_all() {
                app.collections = collections;
            }
        }
    }

    // Restore the terminal before printing a panic, otherwise the message is
    // lost in the alternate screen and the shell is left in raw mode.
    install_panic_hook();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Event handler
    let mut events = EventHandler::new(Duration::from_millis(
        hitt_core::core::constants::TUI_TICK_RATE_MS,
    ));
    app.event_sender = Some(events.sender());

    // Main loop
    let result = run_app(&mut terminal, &mut app, &mut events).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err}");
    }

    Ok(())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste,
            crossterm::cursor::Show
        );
        previous(info);
    }));
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    events: &mut EventHandler,
) -> Result<()> {
    // Minimum time between two frames while background events (SSE /
    // WebSocket streams, ticks) keep arriving; user input redraws at once.
    const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_millis(30);

    loop {
        // Render
        terminal.draw(|frame| {
            ui::layout::render(app, frame);
        })?;
        let last_draw = std::time::Instant::now();

        // Handle events. A burst of stream events is coalesced into one
        // frame, so keypresses never queue behind one redraw per event.
        if let Some(event) = events.next().await {
            let mut input = is_user_input(&event);
            handle_event(app, event).await?;
            while !input && !app.should_quit {
                let remaining = FRAME_BUDGET.saturating_sub(last_draw.elapsed());
                if remaining.is_zero() {
                    break;
                }
                match tokio::time::timeout(remaining, events.next()).await {
                    Ok(Some(event)) => {
                        input = is_user_input(&event);
                        handle_event(app, event).await?;
                    }
                    _ => break,
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Events the user is waiting to see the effect of (drawn without delay).
fn is_user_input(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::Key(_) | AppEvent::Mouse(_) | AppEvent::Paste(_) | AppEvent::Resize(..)
    )
}
