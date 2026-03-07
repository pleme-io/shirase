//! Platform abstraction traits for notification handling.
//!
//! Each platform provides a `NotificationSource` implementation that
//! subscribes to system notifications via native APIs.

#[cfg(target_os = "macos")]
pub mod macos;

/// Notification urgency level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// Low priority notification.
    Low,
    /// Normal priority notification.
    Normal,
    /// Critical/high priority notification.
    Critical,
}

/// An action button on a notification.
#[derive(Debug, Clone)]
pub struct NotificationAction {
    /// Action identifier.
    pub id: String,
    /// Human-readable label.
    pub label: String,
}

/// A notification received from the system.
#[derive(Debug, Clone)]
pub struct SystemNotification {
    /// Unique notification identifier.
    pub id: String,
    /// Source application name.
    pub app_name: String,
    /// Notification title.
    pub title: String,
    /// Notification body text.
    pub body: String,
    /// When the notification was received.
    pub timestamp: chrono::NaiveDateTime,
    /// Notification urgency.
    pub urgency: Urgency,
    /// Available actions for this notification.
    pub actions: Vec<NotificationAction>,
}

/// A stream of incoming notifications.
pub trait NotificationStream: Send {
    /// Get the next notification, or None if the stream is closed.
    fn next(&mut self) -> Result<Option<SystemNotification>, Box<dyn std::error::Error>>;
}

/// Source of system notifications.
pub trait NotificationSource: Send + Sync {
    /// Subscribe to incoming notifications.
    fn subscribe(&self) -> Result<Box<dyn NotificationStream>, Box<dyn std::error::Error>>;
}

/// Create a platform-specific notification source.
pub fn create_source() -> Box<dyn NotificationSource> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSNotificationSource::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        panic!("notification source not implemented for this platform")
    }
}
