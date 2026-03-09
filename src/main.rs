use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use hitt::app::{self, App};
use hitt::core;
use hitt::event::{handle_event, EventHandler};
use hitt::importers;
use hitt::postman;
use hitt::storage::{self, config::AppConfig};
use hitt::ui;

#[derive(Parser, Debug)]
#[command(
    name = "hitt",
    version,
    about = "A fast, beautiful TUI alternative to Postman"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<hitt::cli::Commands>,

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
        return hitt::cli::run(cmd, &config).await;
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

    // Load theme
    if let Ok(theme) = ui::theme::Theme::load(app.config.theme.as_str()) {
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

    // Load environment if specified
    if let Some(path) = &cli.environment {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // Try Postman environment format first
                if let Ok(env) = postman::env_import::import_postman_environment(&content) {
                    app.environments.push(env);
                    app.active_env = Some(0);
                }
                // Then try our native format
                else if let Ok(env) =
                    serde_json::from_str::<core::environment::Environment>(&content)
                {
                    app.environments.push(env);
                    app.active_env = Some(0);
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

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Event handler
    let mut events = EventHandler::new(Duration::from_millis(
        hitt::core::constants::TUI_TICK_RATE_MS,
    ));
    app.event_sender = Some(events.sender());

    // Main loop
    let result = run_app(&mut terminal, &mut app, &mut events).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err}");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    events: &mut EventHandler,
) -> Result<()> {
    loop {
        // Render
        terminal.draw(|frame| {
            ui::layout::render(app, frame);
        })?;

        // Handle events
        if let Some(event) = events.next().await {
            handle_event(app, event).await?;
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
