//! macOS notification source using UNUserNotificationCenter / distributed notifications.

use crate::platform::{NotificationSource, NotificationStream, SystemNotification, Urgency};

/// macOS notification source.
pub struct MacOSNotificationSource;

impl MacOSNotificationSource {
    pub fn new() -> Self {
        Self
    }
}

impl NotificationSource for MacOSNotificationSource {
    fn subscribe(&self) -> Result<Box<dyn NotificationStream>, Box<dyn std::error::Error>> {
        // TODO: implement via NSDistributedNotificationCenter or UNUserNotificationCenter
        tracing::warn!("notification subscription not yet implemented");
        Ok(Box::new(MacOSNotificationStream))
    }
}

/// macOS notification stream.
struct MacOSNotificationStream;

impl NotificationStream for MacOSNotificationStream {
    fn next(&mut self) -> Result<Option<SystemNotification>, Box<dyn std::error::Error>> {
        // TODO: implement notification polling/receiving
        // For now, return a placeholder to demonstrate the API
        let _placeholder = SystemNotification {
            id: String::new(),
            app_name: String::new(),
            title: String::new(),
            body: String::new(),
            timestamp: chrono::Local::now().naive_local(),
            urgency: Urgency::Normal,
            actions: Vec::new(),
        };
        Ok(None)
    }
}
