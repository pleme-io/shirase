//! MCP server for Shirase notification center via kaname.
//!
//! Exposes notification management tools over the Model Context Protocol
//! (stdio transport), allowing AI assistants to send, query, dismiss,
//! and manage notifications.

use kaname::rmcp;
use kaname::ToolResponse;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::ShiraseConfig;
use crate::history::HistoryStore;
use crate::notification::{Notification, Urgency};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
struct SendNotificationRequest {
    /// Notification title.
    title: String,
    /// Notification body text.
    body: String,
    /// Urgency level: "low", "normal", or "critical". Defaults to "normal".
    urgency: Option<String>,
    /// Source application name. Defaults to "mcp".
    app: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListNotificationsRequest {
    /// Maximum number of notifications to return. Defaults to 50.
    limit: Option<usize>,
    /// Filter by application name.
    app: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DismissRequest {
    /// Notification ID to dismiss.
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ClearRequest {
    /// Clear only notifications from this app. Omit to clear all.
    app: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DndToggleRequest {
    /// Enable (true) or disable (false) Do Not Disturb.
    enabled: bool,
    /// Duration in minutes. Omit for indefinite.
    duration_minutes: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetHistoryRequest {
    /// Maximum entries to return. Defaults to 50.
    limit: Option<usize>,
    /// Filter by app name.
    app: Option<String>,
    /// Only show entries since this ISO date (YYYY-MM-DD).
    since: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ConfigGetRequest {
    /// Config key (dot-separated path).
    key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ConfigSetRequest {
    /// Config key.
    key: String,
    /// Value to set (as string).
    value: String,
}

// ---------------------------------------------------------------------------
// MCP Service
// ---------------------------------------------------------------------------

/// Shirase MCP server.
pub struct ShiraseMcpServer {
    tool_router: ToolRouter<Self>,
    config: ShiraseConfig,
}

impl std::fmt::Debug for ShiraseMcpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShiraseMcpServer").finish()
    }
}

fn open_history(config: &ShiraseConfig) -> Result<HistoryStore, String> {
    let path = crate::history::default_history_path();
    HistoryStore::open(path, config.history.max_entries, config.history.retention_days)
        .map_err(|e| format!("{e}"))
}

fn notification_to_json(n: &Notification) -> serde_json::Value {
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
}

#[tool_router]
impl ShiraseMcpServer {
    pub fn new(config: ShiraseConfig) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
        }
    }

    // -- Standard tools --

    #[tool(description = "Get Shirase notification center status.")]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        let (total, unread) = match open_history(&self.config) {
            Ok(store) => (store.len(), store.unread_count()),
            Err(_) => (0, 0),
        };
        Ok(ToolResponse::success(&serde_json::json!({
            "status": "running",
            "total_notifications": total,
            "unread": unread,
        })))
    }

    #[tool(description = "Get the Shirase version.")]
    async fn version(&self) -> Result<CallToolResult, McpError> {
        Ok(ToolResponse::success(&serde_json::json!({
            "name": "shirase",
            "version": env!("CARGO_PKG_VERSION"),
        })))
    }

    #[tool(description = "Get a configuration value by key.")]
    async fn config_get(
        &self,
        Parameters(req): Parameters<ConfigGetRequest>,
    ) -> Result<CallToolResult, McpError> {
        let json = serde_json::to_value(&self.config).unwrap_or_default();
        let value = req
            .key
            .split('.')
            .fold(Some(&json), |v, k| v.and_then(|v| v.get(k)));
        match value {
            Some(v) => Ok(ToolResponse::success(v)),
            None => Ok(ToolResponse::error(&format!("Key '{}' not found", req.key))),
        }
    }

    #[tool(description = "Set a configuration value (runtime only, not persisted).")]
    async fn config_set(
        &self,
        Parameters(req): Parameters<ConfigSetRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(ToolResponse::text(&format!(
            "Config key '{}' would be set to '{}'. Runtime config mutation not yet supported; \
             edit ~/.config/shirase/shirase.yaml instead.",
            req.key, req.value
        )))
    }

    // -- App-specific tools --

    #[tool(description = "Send a notification.")]
    async fn send_notification(
        &self,
        Parameters(req): Parameters<SendNotificationRequest>,
    ) -> Result<CallToolResult, McpError> {
        let urgency: Urgency = req
            .urgency
            .as_deref()
            .unwrap_or("normal")
            .parse()
            .unwrap_or(Urgency::Normal);
        let app = req.app.unwrap_or_else(|| "mcp".to_string());

        let notification = Notification::new(app, &req.title, &req.body).with_urgency(urgency);

        match open_history(&self.config) {
            Ok(mut store) => match store.push(notification.clone()) {
                Ok(()) => Ok(ToolResponse::success(&serde_json::json!({
                    "sent": true,
                    "id": notification.id.to_string(),
                    "title": notification.title,
                }))),
                Err(e) => Ok(ToolResponse::error(&format!("Failed to store: {e}"))),
            },
            Err(e) => Ok(ToolResponse::error(&format!("History unavailable: {e}"))),
        }
    }

    #[tool(description = "List recent notifications from history.")]
    async fn list_notifications(
        &self,
        Parameters(req): Parameters<ListNotificationsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let limit = req.limit.unwrap_or(50);
        match open_history(&self.config) {
            Ok(store) => {
                let notifications: Vec<&Notification> = if let Some(ref app) = req.app {
                    store.by_app(app).into_iter().take(limit).collect()
                } else {
                    store.recent(limit)
                };
                let items: Vec<serde_json::Value> =
                    notifications.iter().map(|n| notification_to_json(n)).collect();
                Ok(ToolResponse::success(&serde_json::json!({
                    "count": items.len(),
                    "notifications": items,
                })))
            }
            Err(e) => Ok(ToolResponse::error(&format!("History unavailable: {e}"))),
        }
    }

    #[tool(description = "Dismiss a notification by ID (marks it as dismissed in history).")]
    async fn dismiss(
        &self,
        Parameters(req): Parameters<DismissRequest>,
    ) -> Result<CallToolResult, McpError> {
        match open_history(&self.config) {
            Ok(mut store) => {
                let id: uuid::Uuid = req
                    .id
                    .parse()
                    .map_err(|e| McpError::invalid_params(format!("Invalid UUID: {e}"), None))?;
                match store.dismiss(id) {
                    Ok(_) => Ok(ToolResponse::success(&serde_json::json!({
                        "dismissed": true,
                        "id": req.id,
                    }))),
                    Err(e) => Ok(ToolResponse::error(&format!("Dismiss failed: {e}"))),
                }
            }
            Err(e) => Ok(ToolResponse::error(&format!("History unavailable: {e}"))),
        }
    }

    #[tool(description = "Clear notification history. Optionally filter by app.")]
    async fn clear(
        &self,
        Parameters(req): Parameters<ClearRequest>,
    ) -> Result<CallToolResult, McpError> {
        match open_history(&self.config) {
            Ok(mut store) => {
                let result = if let Some(ref app) = req.app {
                    store.clear_app(app).map(|count| {
                        serde_json::json!({
                            "cleared": true,
                            "app": app,
                            "count": count,
                        })
                    })
                } else {
                    store.clear().map(|()| {
                        serde_json::json!({
                            "cleared": true,
                            "all": true,
                        })
                    })
                };
                match result {
                    Ok(v) => Ok(ToolResponse::success(&v)),
                    Err(e) => Ok(ToolResponse::error(&format!("Clear failed: {e}"))),
                }
            }
            Err(e) => Ok(ToolResponse::error(&format!("History unavailable: {e}"))),
        }
    }

    #[tool(description = "Toggle Do Not Disturb mode. Optionally set a duration in minutes.")]
    async fn dnd_toggle(
        &self,
        Parameters(req): Parameters<DndToggleRequest>,
    ) -> Result<CallToolResult, McpError> {
        // DND state is managed by the daemon; this tool acknowledges the request
        // and reports what the state should be. Full integration requires the daemon.
        Ok(ToolResponse::success(&serde_json::json!({
            "dnd_enabled": req.enabled,
            "duration_minutes": req.duration_minutes,
            "note": "DND state change requires a running daemon for full effect.",
        })))
    }

    #[tool(description = "Get notification history with optional filters.")]
    async fn get_history(
        &self,
        Parameters(req): Parameters<GetHistoryRequest>,
    ) -> Result<CallToolResult, McpError> {
        let limit = req.limit.unwrap_or(50);
        match open_history(&self.config) {
            Ok(store) => {
                let notifications: Vec<&Notification> = if let Some(ref app) = req.app {
                    store.by_app(app).into_iter().take(limit).collect()
                } else {
                    store.recent(limit)
                };

                // Optionally filter by date
                let items: Vec<serde_json::Value> = if let Some(ref since) = req.since {
                    if let Ok(since_date) =
                        chrono::NaiveDate::parse_from_str(since, "%Y-%m-%d")
                    {
                        notifications
                            .iter()
                            .filter(|n| n.timestamp.date_naive() >= since_date)
                            .map(|n| notification_to_json(n))
                            .collect()
                    } else {
                        notifications.iter().map(|n| notification_to_json(n)).collect()
                    }
                } else {
                    notifications.iter().map(|n| notification_to_json(n)).collect()
                };
                Ok(ToolResponse::success(&serde_json::json!({
                    "count": items.len(),
                    "history": items,
                })))
            }
            Err(e) => Ok(ToolResponse::error(&format!("History unavailable: {e}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// ServerHandler
// ---------------------------------------------------------------------------

#[tool_handler]
impl ServerHandler for ShiraseMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: rmcp::model::Implementation {
                name: "shirase".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Shirase notification center MCP server. Send, query, dismiss, and manage \
                 notifications with DND and history."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the MCP server on stdio.
pub async fn run(config: ShiraseConfig) -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::{transport::stdio, ServiceExt};

    let service = ShiraseMcpServer::new(config);
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
