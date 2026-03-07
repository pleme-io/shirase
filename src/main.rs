mod config;
mod platform;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::config::ShiraseConfig;

#[derive(Parser)]
#[command(name = "shirase", about = "Shirase (知らせ) — notification center")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the notification daemon.
    Daemon,
    /// Show notification history.
    History,
    /// Clear all notifications.
    Clear,
    /// Toggle do-not-disturb mode.
    Dnd {
        #[command(subcommand)]
        action: DndAction,
    },
}

#[derive(Subcommand)]
enum DndAction {
    /// Enable do-not-disturb.
    On,
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
    let config = match shikumi::ConfigDiscovery::new("shirase")
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
    };

    match cli.command {
        Some(Command::Daemon) => {
            tracing::info!(
                "starting daemon on {} (socket: {})",
                config.daemon.listen_addr,
                config.daemon.socket_path,
            );
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async {
                let source = platform::create_source();
                match source.subscribe() {
                    Ok(mut stream) => {
                        tracing::info!("subscribed to notifications");
                        loop {
                            match stream.next() {
                                Ok(Some(notif)) => {
                                    tracing::info!(
                                        "notification from {}: {}",
                                        notif.app_name,
                                        notif.title,
                                    );
                                }
                                Ok(None) => {
                                    // No notification available, wait briefly
                                    tokio::time::sleep(
                                        std::time::Duration::from_millis(100),
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    tracing::error!("notification stream error: {e}");
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => tracing::error!("failed to subscribe: {e}"),
                }
            });
        }
        Some(Command::History) => {
            tracing::info!(
                "showing notification history (max {} entries)",
                config.history.max_entries,
            );
            // TODO: read from history database
            println!("No notification history available.");
        }
        Some(Command::Clear) => {
            tracing::info!("clearing all notifications");
            // TODO: clear notification history
            println!("Notifications cleared.");
        }
        Some(Command::Dnd { action }) => match action {
            DndAction::On => {
                tracing::info!("enabling do-not-disturb");
                // TODO: persist DND state
                println!("Do-not-disturb enabled.");
            }
            DndAction::Off => {
                tracing::info!("disabling do-not-disturb");
                // TODO: persist DND state
                println!("Do-not-disturb disabled.");
            }
        },
        None => {
            // Default: launch GUI
            tracing::info!("launching notification center GUI (not yet implemented)");
        }
    }
}
