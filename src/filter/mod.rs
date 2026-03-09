//! Notification filtering: blocked apps, priority apps, quiet hours, DND, urgency.
//!
//! The [`NotificationFilter`] evaluates incoming notifications against the
//! configured filter rules and Do-Not-Disturb state. Returns a [`FilterResult`]
//! indicating whether the notification should be shown, suppressed, or modified.

use chrono::{Local, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::config::{FilterConfig, QuietHours};
use crate::notification::{Notification, Urgency};

/// Result of filtering a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterResult {
    /// Show the notification normally.
    Allow,
    /// Suppress the notification (still goes to history).
    Suppress(String),
}

/// Do-Not-Disturb state with optional schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DndState {
    /// Whether DND is manually enabled.
    pub enabled: bool,
    /// When DND was manually enabled (for timed DND).
    pub enabled_at: Option<chrono::DateTime<Local>>,
    /// Duration in minutes for timed DND (None = indefinite).
    pub duration_minutes: Option<u32>,
    /// Quiet hours schedule.
    pub quiet_hours: Option<QuietHoursSchedule>,
}

impl Default for DndState {
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_at: None,
            duration_minutes: None,
            quiet_hours: None,
        }
    }
}

impl DndState {
    /// Check if DND is currently active (manual or quiet hours).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.is_manually_active() || self.is_quiet_hours()
    }

    /// Check if manual DND is active (accounting for timed expiry).
    #[must_use]
    pub fn is_manually_active(&self) -> bool {
        if !self.enabled {
            return false;
        }
        // Check if timed DND has expired
        if let (Some(enabled_at), Some(duration)) = (self.enabled_at, self.duration_minutes) {
            let elapsed = Local::now()
                .signed_duration_since(enabled_at)
                .num_minutes();
            if elapsed >= i64::from(duration) {
                return false;
            }
        }
        true
    }

    /// Check if currently within quiet hours.
    #[must_use]
    pub fn is_quiet_hours(&self) -> bool {
        self.quiet_hours
            .as_ref()
            .is_some_and(QuietHoursSchedule::is_active)
    }

    /// Enable DND indefinitely.
    pub fn enable(&mut self) {
        self.enabled = true;
        self.enabled_at = Some(Local::now());
        self.duration_minutes = None;
    }

    /// Enable DND for a duration.
    pub fn enable_for(&mut self, minutes: u32) {
        self.enabled = true;
        self.enabled_at = Some(Local::now());
        self.duration_minutes = Some(minutes);
    }

    /// Disable DND.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.enabled_at = None;
        self.duration_minutes = None;
    }
}

/// Parsed quiet hours schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHoursSchedule {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

impl QuietHoursSchedule {
    /// Parse from config strings (e.g., "22:00" and "07:00").
    pub fn from_config(config: &QuietHours) -> Result<Self, String> {
        let start = NaiveTime::parse_from_str(&config.start, "%H:%M")
            .map_err(|e| format!("invalid quiet hours start: {e}"))?;
        let end = NaiveTime::parse_from_str(&config.end, "%H:%M")
            .map_err(|e| format!("invalid quiet hours end: {e}"))?;
        Ok(Self { start, end })
    }

    /// Check if the current time is within quiet hours.
    #[must_use]
    pub fn is_active(&self) -> bool {
        let now = Local::now().time();
        if self.start <= self.end {
            // Same-day range (e.g., 09:00 - 17:00)
            now >= self.start && now < self.end
        } else {
            // Overnight range (e.g., 22:00 - 07:00)
            now >= self.start || now < self.end
        }
    }
}

/// Notification filter that applies rules, DND, and app filters.
pub struct NotificationFilter {
    /// Blocked application names (lowercase).
    blocked_apps: Vec<String>,
    /// Priority application names that bypass DND (lowercase).
    priority_apps: Vec<String>,
    /// Current DND state.
    dnd: DndState,
}

impl NotificationFilter {
    /// Create a filter from config.
    #[must_use]
    pub fn from_config(config: &FilterConfig, dnd_enabled: bool) -> Self {
        let quiet_hours = config
            .quiet_hours
            .as_ref()
            .and_then(|qh| QuietHoursSchedule::from_config(qh).ok());

        let mut dnd = DndState::default();
        dnd.enabled = dnd_enabled;
        dnd.quiet_hours = quiet_hours;

        Self {
            blocked_apps: config
                .blocked_apps
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            priority_apps: config
                .priority_apps
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            dnd,
        }
    }

    /// Evaluate a notification against all filter rules.
    #[must_use]
    pub fn evaluate(&self, notification: &Notification) -> FilterResult {
        let app_lower = notification.app_name.to_lowercase();

        // Blocked apps are always suppressed
        if self.blocked_apps.contains(&app_lower) {
            return FilterResult::Suppress(format!("app '{}' is blocked", notification.app_name));
        }

        // Critical notifications always pass through
        if notification.urgency == Urgency::Critical {
            return FilterResult::Allow;
        }

        // Priority apps bypass DND
        if self.priority_apps.contains(&app_lower) {
            return FilterResult::Allow;
        }

        // Check DND
        if self.dnd.is_active() {
            return FilterResult::Suppress("do-not-disturb is active".to_string());
        }

        FilterResult::Allow
    }

    /// Get a reference to the DND state.
    #[must_use]
    pub fn dnd(&self) -> &DndState {
        &self.dnd
    }

    /// Get a mutable reference to the DND state.
    pub fn dnd_mut(&mut self) -> &mut DndState {
        &mut self.dnd
    }

    /// Check if an app is blocked.
    #[must_use]
    pub fn is_blocked(&self, app_name: &str) -> bool {
        self.blocked_apps.contains(&app_name.to_lowercase())
    }

    /// Check if an app is a priority app.
    #[must_use]
    pub fn is_priority(&self, app_name: &str) -> bool {
        self.priority_apps.contains(&app_name.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FilterConfig;

    fn make_filter(blocked: &[&str], priority: &[&str], dnd: bool) -> NotificationFilter {
        let config = FilterConfig {
            blocked_apps: blocked.iter().map(|s| (*s).to_string()).collect(),
            priority_apps: priority.iter().map(|s| (*s).to_string()).collect(),
            quiet_hours: None,
        };
        NotificationFilter::from_config(&config, dnd)
    }

    #[test]
    fn normal_notification_passes() {
        let filter = make_filter(&[], &[], false);
        let n = Notification::new("App", "Title", "Body");
        assert_eq!(filter.evaluate(&n), FilterResult::Allow);
    }

    #[test]
    fn blocked_app_suppressed() {
        let filter = make_filter(&["Finder"], &[], false);
        let n = Notification::new("Finder", "Title", "Body");
        assert!(matches!(filter.evaluate(&n), FilterResult::Suppress(_)));
    }

    #[test]
    fn blocked_app_case_insensitive() {
        let filter = make_filter(&["finder"], &[], false);
        let n = Notification::new("Finder", "Title", "Body");
        assert!(matches!(filter.evaluate(&n), FilterResult::Suppress(_)));
    }

    #[test]
    fn dnd_suppresses_normal() {
        let filter = make_filter(&[], &[], true);
        let n = Notification::new("App", "Title", "Body");
        assert!(matches!(filter.evaluate(&n), FilterResult::Suppress(_)));
    }

    #[test]
    fn dnd_allows_critical() {
        let filter = make_filter(&[], &[], true);
        let n = Notification::new("App", "Title", "Body").with_urgency(Urgency::Critical);
        assert_eq!(filter.evaluate(&n), FilterResult::Allow);
    }

    #[test]
    fn priority_app_bypasses_dnd() {
        let filter = make_filter(&[], &["Messages"], true);
        let n = Notification::new("Messages", "Title", "Body");
        assert_eq!(filter.evaluate(&n), FilterResult::Allow);
    }

    #[test]
    fn dnd_enable_disable() {
        let mut dnd = DndState::default();
        assert!(!dnd.is_active());

        dnd.enable();
        assert!(dnd.is_active());

        dnd.disable();
        assert!(!dnd.is_active());
    }

    #[test]
    fn dnd_timed() {
        let mut dnd = DndState::default();
        dnd.enable_for(60);
        assert!(dnd.is_manually_active());
    }

    #[test]
    fn quiet_hours_overnight() {
        use chrono::Timelike;
        let schedule = QuietHoursSchedule {
            start: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
        };
        let now = Local::now().time();
        let hour = now.hour();
        // The schedule is active between 22:00 and 07:00
        let expected_active = hour >= 22 || hour < 7;
        assert_eq!(schedule.is_active(), expected_active);
    }

    #[test]
    fn quiet_hours_from_config() {
        let qh = QuietHours {
            start: "22:00".to_string(),
            end: "07:00".to_string(),
        };
        let schedule = QuietHoursSchedule::from_config(&qh).unwrap();
        assert_eq!(schedule.start, NaiveTime::from_hms_opt(22, 0, 0).unwrap());
        assert_eq!(schedule.end, NaiveTime::from_hms_opt(7, 0, 0).unwrap());
    }

    #[test]
    fn quiet_hours_invalid_time() {
        let qh = QuietHours {
            start: "invalid".to_string(),
            end: "07:00".to_string(),
        };
        assert!(QuietHoursSchedule::from_config(&qh).is_err());
    }

    #[test]
    fn is_blocked_check() {
        let filter = make_filter(&["Chess", "Automator"], &[], false);
        assert!(filter.is_blocked("Chess"));
        assert!(filter.is_blocked("chess")); // case insensitive
        assert!(!filter.is_blocked("Mail"));
    }

    #[test]
    fn is_priority_check() {
        let filter = make_filter(&[], &["Messages", "Calendar"], false);
        assert!(filter.is_priority("Messages"));
        assert!(filter.is_priority("messages")); // case insensitive
        assert!(!filter.is_priority("Slack"));
    }
}
