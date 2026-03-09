//! Core notification data model with grouping support.
//!
//! Defines the [`Notification`] struct used throughout shirase, along with
//! urgency levels and application-based grouping.

use std::collections::BTreeMap;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Notification urgency level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    /// Informational, non-intrusive.
    Low,
    /// Standard notification.
    Normal,
    /// Requires immediate attention.
    Critical,
}

impl Default for Urgency {
    fn default() -> Self {
        Self::Normal
    }
}

impl std::fmt::Display for Urgency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Normal => write!(f, "normal"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

impl std::str::FromStr for Urgency {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "critical" | "high" => Ok(Self::Critical),
            other => Err(format!("unknown urgency: {other}")),
        }
    }
}

/// A notification managed by shirase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique identifier.
    pub id: Uuid,
    /// Source application name.
    pub app_name: String,
    /// Notification title.
    pub title: String,
    /// Notification body text.
    pub body: String,
    /// Urgency level.
    pub urgency: Urgency,
    /// When the notification was received.
    pub timestamp: DateTime<Local>,
    /// Whether the notification has been read/seen.
    pub read: bool,
    /// Whether the notification has been dismissed.
    pub dismissed: bool,
}

impl Notification {
    /// Create a new notification with a generated UUID and current timestamp.
    #[must_use]
    pub fn new(app_name: impl Into<String>, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            app_name: app_name.into(),
            title: title.into(),
            body: body.into(),
            urgency: Urgency::default(),
            timestamp: Local::now(),
            read: false,
            dismissed: false,
        }
    }

    /// Set the urgency level.
    #[must_use]
    pub fn with_urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    /// Create from a tsuuchi notification, adding shirase metadata.
    #[must_use]
    pub fn from_tsuuchi(notif: &tsuuchi::Notification, app_name: &str) -> Self {
        let urgency = match notif.urgency {
            tsuuchi::Urgency::Low => Urgency::Low,
            tsuuchi::Urgency::Normal => Urgency::Normal,
            tsuuchi::Urgency::Critical => Urgency::Critical,
        };
        Self {
            id: Uuid::new_v4(),
            app_name: app_name.to_string(),
            title: notif.title.clone(),
            body: notif.body.clone(),
            urgency,
            timestamp: Local::now(),
            read: false,
            dismissed: false,
        }
    }
}

/// A group of notifications from the same application.
#[derive(Debug, Clone)]
pub struct NotificationGroup {
    /// Application name for this group.
    pub app_name: String,
    /// Notifications in this group, newest first.
    pub notifications: Vec<Notification>,
}

impl NotificationGroup {
    /// Count of unread notifications in this group.
    #[must_use]
    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    /// Total notification count.
    #[must_use]
    pub fn count(&self) -> usize {
        self.notifications.len()
    }
}

/// Group a flat list of notifications by application name.
///
/// Returns groups sorted by the most recent notification timestamp (newest first).
/// Within each group, notifications are also sorted newest first.
#[must_use]
pub fn group_by_app(notifications: &[Notification]) -> Vec<NotificationGroup> {
    let mut groups: BTreeMap<String, Vec<Notification>> = BTreeMap::new();
    for notif in notifications {
        groups
            .entry(notif.app_name.clone())
            .or_default()
            .push(notif.clone());
    }

    let mut result: Vec<NotificationGroup> = groups
        .into_iter()
        .map(|(app_name, mut notifs)| {
            notifs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            NotificationGroup {
                app_name,
                notifications: notifs,
            }
        })
        .collect();

    // Sort groups by most recent notification
    result.sort_by(|a, b| {
        let a_latest = a.notifications.first().map(|n| n.timestamp);
        let b_latest = b.notifications.first().map(|n| n.timestamp);
        b_latest.cmp(&a_latest)
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_notification_defaults() {
        let n = Notification::new("TestApp", "Title", "Body");
        assert_eq!(n.app_name, "TestApp");
        assert_eq!(n.title, "Title");
        assert_eq!(n.body, "Body");
        assert_eq!(n.urgency, Urgency::Normal);
        assert!(!n.read);
        assert!(!n.dismissed);
    }

    #[test]
    fn with_urgency() {
        let n = Notification::new("App", "T", "B").with_urgency(Urgency::Critical);
        assert_eq!(n.urgency, Urgency::Critical);
    }

    #[test]
    fn urgency_ordering() {
        assert!(Urgency::Low < Urgency::Normal);
        assert!(Urgency::Normal < Urgency::Critical);
    }

    #[test]
    fn urgency_parse() {
        assert_eq!("low".parse::<Urgency>().unwrap(), Urgency::Low);
        assert_eq!("normal".parse::<Urgency>().unwrap(), Urgency::Normal);
        assert_eq!("critical".parse::<Urgency>().unwrap(), Urgency::Critical);
        assert_eq!("high".parse::<Urgency>().unwrap(), Urgency::Critical);
        assert!("bogus".parse::<Urgency>().is_err());
    }

    #[test]
    fn urgency_display() {
        assert_eq!(Urgency::Low.to_string(), "low");
        assert_eq!(Urgency::Normal.to_string(), "normal");
        assert_eq!(Urgency::Critical.to_string(), "critical");
    }

    #[test]
    fn group_by_app_basic() {
        let notifs = vec![
            Notification::new("Mail", "Subject 1", "Body 1"),
            Notification::new("Slack", "Message", "Hello"),
            Notification::new("Mail", "Subject 2", "Body 2"),
        ];
        let groups = group_by_app(&notifs);
        assert_eq!(groups.len(), 2);
        // Mail has 2 notifications
        let mail = groups.iter().find(|g| g.app_name == "Mail").unwrap();
        assert_eq!(mail.count(), 2);
        let slack = groups.iter().find(|g| g.app_name == "Slack").unwrap();
        assert_eq!(slack.count(), 1);
    }

    #[test]
    fn group_unread_count() {
        let mut n1 = Notification::new("App", "T1", "B1");
        let n2 = Notification::new("App", "T2", "B2");
        n1.read = true;
        let group = NotificationGroup {
            app_name: "App".to_string(),
            notifications: vec![n1, n2],
        };
        assert_eq!(group.unread_count(), 1);
        assert_eq!(group.count(), 2);
    }

    #[test]
    fn notification_serialization() {
        let n = Notification::new("App", "Title", "Body").with_urgency(Urgency::Low);
        let json = serde_json::to_string(&n).unwrap();
        let deserialized: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.app_name, "App");
        assert_eq!(deserialized.urgency, Urgency::Low);
    }

    #[test]
    fn from_tsuuchi_conversion() {
        let t = tsuuchi::Notification::new("Alert", "Disk full")
            .urgency(tsuuchi::Urgency::Critical);
        let n = Notification::from_tsuuchi(&t, "System");
        assert_eq!(n.app_name, "System");
        assert_eq!(n.title, "Alert");
        assert_eq!(n.body, "Disk full");
        assert_eq!(n.urgency, Urgency::Critical);
    }
}
