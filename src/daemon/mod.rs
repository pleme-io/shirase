//! Daemon mode: Unix socket listener for IPC from other pleme-io apps.
//!
//! The daemon listens on a Unix socket for JSON-encoded commands from
//! CLI tools and other applications. It manages the notification lifecycle:
//! receiving, filtering, storing to history, and responding to queries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

use crate::config::ShiraseConfig;
use crate::filter::{FilterResult, NotificationFilter};
use crate::history::HistoryStore;
use crate::notification::{group_by_app, Notification, Urgency};

/// IPC command sent to the daemon over the Unix socket.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum DaemonCommand {
    /// Send a notification to the daemon.
    Send {
        app_name: String,
        title: String,
        body: String,
        #[serde(default)]
        urgency: Option<String>,
    },
    /// Dismiss a notification by ID.
    Dismiss { id: String },
    /// Dismiss all notifications.
    DismissAll,
    /// Get recent notifications.
    List {
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        app: Option<String>,
    },
    /// Search notification history.
    Search { query: String },
    /// Get current status (DND, counts).
    Status,
    /// Enable DND.
    DndOn {
        #[serde(default)]
        minutes: Option<u32>,
    },
    /// Disable DND.
    DndOff,
    /// Clear history.
    Clear {
        #[serde(default)]
        app: Option<String>,
    },
    /// Mark notification as read.
    MarkRead { id: String },
    /// Mark all as read.
    MarkAllRead,
    /// Health check.
    Health,
}

/// IPC response from the daemon.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DaemonResponse {
    Ok {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },
    Error {
        message: String,
    },
}

impl DaemonResponse {
    fn ok(message: impl Into<String>) -> Self {
        Self::Ok {
            message: Some(message.into()),
            data: None,
        }
    }

    fn ok_data(data: serde_json::Value) -> Self {
        Self::Ok {
            message: None,
            data: Some(data),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}

/// Shared daemon state protected by a mutex.
pub struct DaemonState {
    pub history: HistoryStore,
    pub filter: NotificationFilter,
    pub config: ShiraseConfig,
    pub start_time: std::time::Instant,
}

/// Run the daemon, listening on a Unix socket.
pub async fn run_daemon(config: ShiraseConfig) -> Result<(), DaemonError> {
    let socket_path = PathBuf::from(&config.daemon.socket_path);

    // Use tsunagu for daemon lifecycle
    let daemon_process = tsunagu::DaemonProcess::with_paths(
        "shirase",
        tsunagu::SocketPath::pid_file("shirase"),
        socket_path.clone(),
    );

    daemon_process.acquire().map_err(|e| DaemonError::Lock(e.to_string()))?;
    tracing::info!("daemon lock acquired, PID file written");

    // Clean up stale socket
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Open history store
    let history_path = crate::history::default_history_path();
    let history = HistoryStore::open(
        &history_path,
        config.history.max_entries,
        config.history.retention_days,
    )
    .map_err(|e| DaemonError::History(e.to_string()))?;

    tracing::info!("history store opened at {}", history_path.display());

    // Set up filter
    let filter = NotificationFilter::from_config(
        &config.filters,
        config.behavior.do_not_disturb,
    );

    let state = Arc::new(Mutex::new(DaemonState {
        history,
        filter,
        config: config.clone(),
        start_time: std::time::Instant::now(),
    }));

    // Bind Unix socket
    let listener = UnixListener::bind(&socket_path)?;
    tracing::info!("daemon listening on {}", socket_path.display());

    // Set socket permissions (world-writable for IPC from any user process)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o666))?;
    }

    // Handle shutdown
    let state_clone = Arc::clone(&state);
    let socket_path_clone = socket_path.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutting down daemon");
        // Clean up socket file on shutdown
        let _ = std::fs::remove_file(&socket_path_clone);
        // DaemonProcess drop will clean up PID
        drop(state_clone);
        std::process::exit(0);
    });

    // Accept connections
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        tracing::error!("connection error: {e}");
                    }
                });
            }
            Err(e) => {
                tracing::error!("accept error: {e}");
            }
        }
    }
}

/// Handle a single client connection.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: Arc<Mutex<DaemonState>>,
) -> Result<(), DaemonError> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one line (one JSON command per connection)
    reader.read_line(&mut line).await?;
    let line = line.trim();

    if line.is_empty() {
        return Ok(());
    }

    let response = match serde_json::from_str::<DaemonCommand>(line) {
        Ok(cmd) => handle_command(cmd, &state).await,
        Err(e) => DaemonResponse::error(format!("invalid command: {e}")),
    };

    let response_json = serde_json::to_string(&response)?;
    writer.write_all(response_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.shutdown().await?;

    Ok(())
}

/// Process a daemon command and return a response.
async fn handle_command(
    cmd: DaemonCommand,
    state: &Arc<Mutex<DaemonState>>,
) -> DaemonResponse {
    let mut state = state.lock().await;

    match cmd {
        DaemonCommand::Send {
            app_name,
            title,
            body,
            urgency,
        } => {
            let urgency_level = urgency
                .as_deref()
                .map(|u| u.parse::<Urgency>().unwrap_or_default())
                .unwrap_or_default();

            let notification = Notification::new(&app_name, &title, &body)
                .with_urgency(urgency_level);

            let filter_result = state.filter.evaluate(&notification);
            let id = notification.id;

            // Always store to history
            if let Err(e) = state.history.push(notification) {
                return DaemonResponse::error(format!("failed to store: {e}"));
            }

            match filter_result {
                FilterResult::Allow => {
                    tracing::info!(%app_name, %title, "notification received and displayed");
                    DaemonResponse::Ok {
                        message: Some("notification received".to_string()),
                        data: Some(serde_json::json!({
                            "id": id.to_string(),
                            "displayed": true,
                        })),
                    }
                }
                FilterResult::Suppress(reason) => {
                    tracing::info!(%app_name, %title, %reason, "notification suppressed");
                    DaemonResponse::Ok {
                        message: Some(format!("notification suppressed: {reason}")),
                        data: Some(serde_json::json!({
                            "id": id.to_string(),
                            "displayed": false,
                            "reason": reason,
                        })),
                    }
                }
            }
        }

        DaemonCommand::Dismiss { id } => {
            match uuid::Uuid::parse_str(&id) {
                Ok(uuid) => match state.history.dismiss(uuid) {
                    Ok(true) => DaemonResponse::ok("notification dismissed"),
                    Ok(false) => DaemonResponse::error("notification not found"),
                    Err(e) => DaemonResponse::error(format!("dismiss failed: {e}")),
                },
                Err(e) => DaemonResponse::error(format!("invalid ID: {e}")),
            }
        }

        DaemonCommand::DismissAll => {
            let active: Vec<uuid::Uuid> = state.history.all()
                .iter()
                .filter(|n| !n.dismissed)
                .map(|n| n.id)
                .collect();
            let mut count = 0;
            for id in active {
                if state.history.dismiss(id).unwrap_or(false) {
                    count += 1;
                }
            }
            DaemonResponse::ok(format!("{count} notifications dismissed"))
        }

        DaemonCommand::List { limit, app } => {
            let notifications = if let Some(app_name) = app {
                state.history.by_app(&app_name)
            } else {
                state.history.recent(limit.unwrap_or(50))
            };

            let data: Vec<serde_json::Value> = notifications
                .iter()
                .take(limit.unwrap_or(50))
                .map(|n| {
                    serde_json::json!({
                        "id": n.id.to_string(),
                        "app": n.app_name,
                        "title": n.title,
                        "body": n.body,
                        "urgency": n.urgency.to_string(),
                        "timestamp": n.timestamp.to_rfc3339(),
                        "read": n.read,
                        "dismissed": n.dismissed,
                    })
                })
                .collect();

            DaemonResponse::ok_data(serde_json::json!({
                "notifications": data,
                "total": state.history.len(),
            }))
        }

        DaemonCommand::Search { query } => {
            let results = state.history.search(&query);
            let data: Vec<serde_json::Value> = results
                .iter()
                .map(|n| {
                    serde_json::json!({
                        "id": n.id.to_string(),
                        "app": n.app_name,
                        "title": n.title,
                        "body": n.body,
                        "urgency": n.urgency.to_string(),
                        "timestamp": n.timestamp.to_rfc3339(),
                    })
                })
                .collect();
            DaemonResponse::ok_data(serde_json::json!({ "results": data }))
        }

        DaemonCommand::Status => {
            let dnd = state.filter.dnd();
            let uptime = state.start_time.elapsed().as_secs();

            // Group notifications for summary
            let active: Vec<Notification> = state.history.all()
                .iter()
                .filter(|n| !n.dismissed)
                .cloned()
                .cloned()
                .collect();
            let groups = group_by_app(&active);

            let group_summary: Vec<serde_json::Value> = groups
                .iter()
                .map(|g| {
                    serde_json::json!({
                        "app": g.app_name,
                        "count": g.count(),
                        "unread": g.unread_count(),
                    })
                })
                .collect();

            DaemonResponse::ok_data(serde_json::json!({
                "dnd": dnd.is_active(),
                "dnd_manual": dnd.is_manually_active(),
                "dnd_quiet_hours": dnd.is_quiet_hours(),
                "total": state.history.len(),
                "unread": state.history.unread_count(),
                "active": state.history.active_count(),
                "uptime_secs": uptime,
                "groups": group_summary,
            }))
        }

        DaemonCommand::DndOn { minutes } => {
            if let Some(mins) = minutes {
                state.filter.dnd_mut().enable_for(mins);
                DaemonResponse::ok(format!("DND enabled for {mins} minutes"))
            } else {
                state.filter.dnd_mut().enable();
                DaemonResponse::ok("DND enabled")
            }
        }

        DaemonCommand::DndOff => {
            state.filter.dnd_mut().disable();
            DaemonResponse::ok("DND disabled")
        }

        DaemonCommand::Clear { app } => {
            if let Some(app_name) = app {
                match state.history.clear_app(&app_name) {
                    Ok(count) => DaemonResponse::ok(format!("{count} entries cleared for {app_name}")),
                    Err(e) => DaemonResponse::error(format!("clear failed: {e}")),
                }
            } else {
                match state.history.clear() {
                    Ok(()) => DaemonResponse::ok("all history cleared"),
                    Err(e) => DaemonResponse::error(format!("clear failed: {e}")),
                }
            }
        }

        DaemonCommand::MarkRead { id } => {
            match uuid::Uuid::parse_str(&id) {
                Ok(uuid) => match state.history.mark_read(uuid) {
                    Ok(true) => DaemonResponse::ok("marked as read"),
                    Ok(false) => DaemonResponse::error("notification not found"),
                    Err(e) => DaemonResponse::error(format!("mark read failed: {e}")),
                },
                Err(e) => DaemonResponse::error(format!("invalid ID: {e}")),
            }
        }

        DaemonCommand::MarkAllRead => {
            match state.history.mark_all_read() {
                Ok(count) => DaemonResponse::ok(format!("{count} notifications marked as read")),
                Err(e) => DaemonResponse::error(format!("mark all read failed: {e}")),
            }
        }

        DaemonCommand::Health => {
            let hc = tsunagu::HealthCheck::healthy("shirase", env!("CARGO_PKG_VERSION"))
                .with_uptime(state.start_time.elapsed().as_secs());
            DaemonResponse::ok_data(serde_json::to_value(&hc).unwrap_or_default())
        }
    }
}

/// Send a command to the running daemon via Unix socket.
pub async fn send_command(socket_path: &Path, command: &DaemonCommand) -> Result<DaemonResponse, DaemonError> {
    let stream = tokio::net::UnixStream::connect(socket_path).await
        .map_err(|e| DaemonError::Connect(format!(
            "cannot connect to daemon at {}: {e} (is the daemon running?)",
            socket_path.display()
        )))?;

    let (reader, mut writer) = stream.into_split();

    let cmd_json = serde_json::to_string(command)?;
    writer.write_all(cmd_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.shutdown().await?;

    let mut reader = BufReader::new(reader);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;

    let response: DaemonResponse = serde_json::from_str(response_line.trim())?;
    Ok(response)
}

/// Errors from the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("daemon lock error: {0}")]
    Lock(String),

    #[error("history error: {0}")]
    History(String),

    #[error("connection error: {0}")]
    Connect(String),
}
