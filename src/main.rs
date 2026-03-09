mod config;
mod daemon;
mod filter;
mod history;
mod input;
mod notification;
mod platform;
mod render;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::config::ShiraseConfig;
use crate::daemon::{DaemonCommand, DaemonResponse};
use crate::filter::DndState;
use crate::history::HistoryStore;
use crate::input::UiState;
use crate::notification::Notification;

#[derive(Parser)]
#[command(name = "shirase", about = "Shirase (知らせ) — notification center")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the notification daemon (background service).
    Daemon,
    /// Show notification history.
    History {
        /// Maximum entries to show.
        #[arg(short, long, default_value = "50")]
        limit: usize,
        /// Filter by app name.
        #[arg(short, long)]
        app: Option<String>,
    },
    /// Clear all notifications.
    Clear {
        /// Clear only for this app.
        #[arg(short, long)]
        app: Option<String>,
    },
    /// Toggle do-not-disturb mode.
    Dnd {
        #[command(subcommand)]
        action: DndAction,
    },
    /// Send a test notification.
    Send {
        /// Notification title.
        title: String,
        /// Notification body.
        body: String,
        /// Urgency level (low, normal, critical).
        #[arg(short, long, default_value = "normal")]
        urgency: String,
        /// Source app name.
        #[arg(short, long, default_value = "shirase")]
        app: String,
    },
    /// Show daemon status.
    Status,
    /// Search notification history.
    Search {
        /// Search query.
        query: String,
    },
}

#[derive(Subcommand)]
enum DndAction {
    /// Enable do-not-disturb.
    On {
        /// Duration in minutes (omit for indefinite).
        #[arg(short, long)]
        minutes: Option<u32>,
    },
    /// Disable do-not-disturb.
    Off,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Load config via shikumi
    let config = load_config();

    match cli.command {
        Some(Command::Daemon) => {
            tracing::info!(
                "starting daemon (socket: {})",
                config.daemon.socket_path,
            );
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async {
                if let Err(e) = daemon::run_daemon(config).await {
                    tracing::error!("daemon error: {e}");
                    std::process::exit(1);
                }
            });
        }

        Some(Command::Send {
            title,
            body,
            urgency,
            app,
        }) => {
            let socket_path = PathBuf::from(&config.daemon.socket_path);
            let cmd = DaemonCommand::Send {
                app_name: app,
                title,
                body,
                urgency: Some(urgency),
            };
            run_client_command(&socket_path, &cmd);
        }

        Some(Command::History { limit, app }) => {
            let socket_path = PathBuf::from(&config.daemon.socket_path);

            // Try daemon first, fall back to direct file access
            if socket_path.exists() {
                let cmd = DaemonCommand::List {
                    limit: Some(limit),
                    app,
                };
                run_client_command(&socket_path, &cmd);
            } else {
                // Direct file access when daemon is not running
                show_history_direct(&config, limit, app.as_deref());
            }
        }

        Some(Command::Clear { app }) => {
            let socket_path = PathBuf::from(&config.daemon.socket_path);
            if socket_path.exists() {
                let cmd = DaemonCommand::Clear { app };
                run_client_command(&socket_path, &cmd);
            } else {
                clear_history_direct(&config, app.as_deref());
            }
        }

        Some(Command::Dnd { action }) => {
            let socket_path = PathBuf::from(&config.daemon.socket_path);
            let cmd = match action {
                DndAction::On { minutes } => DaemonCommand::DndOn { minutes },
                DndAction::Off => DaemonCommand::DndOff,
            };
            if socket_path.exists() {
                run_client_command(&socket_path, &cmd);
            } else {
                match cmd {
                    DaemonCommand::DndOn { minutes } => {
                        if let Some(m) = minutes {
                            println!("DND enabled for {m} minutes (daemon not running, state not persisted)");
                        } else {
                            println!("DND enabled (daemon not running, state not persisted)");
                        }
                    }
                    DaemonCommand::DndOff => {
                        println!("DND disabled (daemon not running, state not persisted)");
                    }
                    _ => unreachable!(),
                }
            }
        }

        Some(Command::Status) => {
            let socket_path = PathBuf::from(&config.daemon.socket_path);
            if socket_path.exists() {
                let cmd = DaemonCommand::Status;
                run_client_command(&socket_path, &cmd);
            } else {
                println!("Daemon is not running.");
                show_history_direct(&config, 0, None);
            }
        }

        Some(Command::Search { query }) => {
            let socket_path = PathBuf::from(&config.daemon.socket_path);
            if socket_path.exists() {
                let cmd = DaemonCommand::Search { query };
                run_client_command(&socket_path, &cmd);
            } else {
                search_history_direct(&config, &query);
            }
        }

        None => {
            // Default: show notification center (interactive view)
            show_notification_center(&config);
        }
    }
}

/// Load configuration via shikumi.
fn load_config() -> ShiraseConfig {
    match shikumi::ConfigDiscovery::new("shirase")
        .env_override("SHIRASE_CONFIG")
        .discover()
    {
        Ok(path) => {
            tracing::info!("loading config from {}", path.display());
            let store = shikumi::ConfigStore::<ShiraseConfig>::load(&path, "SHIRASE_")
                .unwrap_or_else(|e| {
                    tracing::warn!("failed to load config: {e}, using defaults");
                    let tmp = std::env::temp_dir().join("shirase-default.yaml");
                    std::fs::write(&tmp, "{}").ok();
                    shikumi::ConfigStore::load(&tmp, "SHIRASE_").unwrap()
                });
            ShiraseConfig::clone(&store.get())
        }
        Err(_) => {
            tracing::info!("no config file found, using defaults");
            ShiraseConfig::default()
        }
    }
}

/// Send a command to the daemon and display the response.
fn run_client_command(socket_path: &std::path::Path, cmd: &DaemonCommand) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    match rt.block_on(daemon::send_command(socket_path, cmd)) {
        Ok(response) => {
            match response {
                DaemonResponse::Ok { message, data } => {
                    if let Some(msg) = message {
                        println!("{msg}");
                    }
                    if let Some(data) = data {
                        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
                    }
                }
                DaemonResponse::Error { message } => {
                    eprintln!("Error: {message}");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to communicate with daemon: {e}");
            eprintln!("Is the daemon running? Start with: shirase daemon");
            std::process::exit(1);
        }
    }
}

/// Show notification history directly from file (no daemon needed).
fn show_history_direct(config: &ShiraseConfig, limit: usize, app: Option<&str>) {
    let history_path = history::default_history_path();
    match HistoryStore::open(&history_path, config.history.max_entries, config.history.retention_days) {
        Ok(store) => {
            let notifications = if let Some(app_name) = app {
                store.by_app(app_name).into_iter().cloned().collect::<Vec<_>>()
            } else if limit > 0 {
                store.recent(limit).into_iter().cloned().collect::<Vec<_>>()
            } else {
                store.all().into_iter().cloned().collect::<Vec<_>>()
            };

            if notifications.is_empty() {
                println!("No notification history.");
            } else {
                let dnd = DndState::default();
                let ui = UiState::default();
                render::render_header(&dnd, store.unread_count(), store.len());
                let refs: Vec<&Notification> = notifications.iter().collect();
                render::render_notification_list(&refs, &ui);
            }
        }
        Err(e) => {
            eprintln!("Failed to open history: {e}");
        }
    }
}

/// Clear history directly from file.
fn clear_history_direct(config: &ShiraseConfig, app: Option<&str>) {
    let history_path = history::default_history_path();
    match HistoryStore::open(&history_path, config.history.max_entries, config.history.retention_days) {
        Ok(mut store) => {
            if let Some(app_name) = app {
                match store.clear_app(app_name) {
                    Ok(count) => println!("{count} entries cleared for {app_name}."),
                    Err(e) => eprintln!("Failed to clear: {e}"),
                }
            } else {
                match store.clear() {
                    Ok(()) => println!("All history cleared."),
                    Err(e) => eprintln!("Failed to clear: {e}"),
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open history: {e}");
        }
    }
}

/// Search history directly from file.
fn search_history_direct(config: &ShiraseConfig, query: &str) {
    let history_path = history::default_history_path();
    match HistoryStore::open(&history_path, config.history.max_entries, config.history.retention_days) {
        Ok(store) => {
            let results = store.search(query);
            if results.is_empty() {
                println!("No results for \"{query}\".");
            } else {
                println!("Search results for \"{query}\" ({} found):", results.len());
                let ui = UiState::default();
                render::render_notification_list(&results, &ui);
            }
        }
        Err(e) => {
            eprintln!("Failed to open history: {e}");
        }
    }
}

/// Show the interactive notification center.
fn show_notification_center(config: &ShiraseConfig) {
    let history_path = history::default_history_path();
    match HistoryStore::open(&history_path, config.history.max_entries, config.history.retention_days) {
        Ok(store) => {
            let notifications: Vec<Notification> = store.all().into_iter().cloned().collect();
            let dnd = DndState::default();

            let mut ui = UiState::default();
            let ids: Vec<uuid::Uuid> = notifications.iter().map(|n| n.id).collect();
            ui.update_visible(ids);

            render::render_center(&notifications, &dnd, &ui);

            println!();
            println!("  (Interactive mode requires a running daemon for real-time updates.)");
            println!("  Start the daemon with: shirase daemon");
            println!("  Send notifications with: shirase send \"Title\" \"Body\"");
        }
        Err(e) => {
            eprintln!("Failed to open history: {e}");
        }
    }
}
