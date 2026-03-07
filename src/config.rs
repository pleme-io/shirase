//! Shirase configuration — uses shikumi for discovery and hot-reload.

use serde::{Deserialize, Serialize};

/// Top-level configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ShiraseConfig {
    pub appearance: AppearanceConfig,
    pub behavior: BehaviorConfig,
    pub filters: FilterConfig,
    pub history: HistoryConfig,
    pub daemon: DaemonConfig,
}

impl Default for ShiraseConfig {
    fn default() -> Self {
        Self {
            appearance: AppearanceConfig::default(),
            behavior: BehaviorConfig::default(),
            filters: FilterConfig::default(),
            history: HistoryConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

/// Visual appearance settings.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct AppearanceConfig {
    /// Notification popup width in pixels.
    pub width: u32,
    /// Maximum number of visible notifications at once.
    pub max_visible: u32,
    /// Notification opacity (0.0-1.0).
    pub opacity: f32,
    /// Screen position: "top-right", "top-left", or "bottom-right".
    pub position: String,
    /// Animation duration in milliseconds.
    pub animation_ms: u32,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            width: 360,
            max_visible: 5,
            opacity: 0.95,
            position: "top-right".into(),
            animation_ms: 200,
        }
    }
}

/// Notification behavior settings.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Seconds before auto-dismissing notifications.
    pub auto_dismiss_secs: u32,
    /// Enable do-not-disturb mode.
    pub do_not_disturb: bool,
    /// Group notifications by source app.
    pub group_by_app: bool,
    /// Enable notification sounds.
    pub sound_enabled: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            auto_dismiss_secs: 5,
            do_not_disturb: false,
            group_by_app: true,
            sound_enabled: true,
        }
    }
}

/// Notification filter settings.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct FilterConfig {
    /// Apps whose notifications are blocked.
    pub blocked_apps: Vec<String>,
    /// Apps whose notifications are always shown (even in DND).
    pub priority_apps: Vec<String>,
    /// Quiet hours (no notifications).
    pub quiet_hours: Option<QuietHours>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            blocked_apps: Vec::new(),
            priority_apps: Vec::new(),
            quiet_hours: None,
        }
    }
}

/// Quiet hours configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct QuietHours {
    /// Start time (e.g. "22:00").
    pub start: String,
    /// End time (e.g. "07:00").
    pub end: String,
}

/// Notification history settings.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct HistoryConfig {
    /// Maximum number of entries to keep.
    pub max_entries: u32,
    /// Days to retain history.
    pub retention_days: u32,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            retention_days: 30,
        }
    }
}

/// Daemon mode configuration.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct DaemonConfig {
    /// Enable daemon mode.
    pub enable: bool,
    /// Listen address for the daemon.
    pub listen_addr: String,
    /// Unix socket path for local IPC.
    pub socket_path: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            enable: false,
            listen_addr: "0.0.0.0:50053".into(),
            socket_path: "/tmp/shirase.sock".into(),
        }
    }
}
