//! Persistent notification history with JSON file storage.
//!
//! Stores notifications to a JSON file for persistence across restarts.
//! Supports querying by app, urgency, text search, and date range.
//! Enforces max entry count and retention period.

use std::path::{Path, PathBuf};

use chrono::{Days, Local};

use crate::notification::{Notification, Urgency};

/// Persistent notification history backed by a JSON file.
#[derive(Debug)]
pub struct HistoryStore {
    /// Path to the JSON history file.
    path: PathBuf,
    /// In-memory notification list (newest first after sort).
    entries: Vec<Notification>,
    /// Maximum number of entries to retain.
    max_entries: u32,
    /// Days to keep entries before eviction.
    retention_days: u32,
}

impl HistoryStore {
    /// Open or create a history store at the given path.
    pub fn open(path: impl Into<PathBuf>, max_entries: u32, retention_days: u32) -> Result<Self, HistoryError> {
        let path = path.into();
        let entries = if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            if data.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&data).map_err(|e| HistoryError::Corrupt(e.to_string()))?
            }
        } else {
            // Create parent directories
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Vec::new()
        };

        let mut store = Self {
            path,
            entries,
            max_entries,
            retention_days,
        };
        store.enforce_limits();
        Ok(store)
    }

    /// Add a notification to history.
    pub fn push(&mut self, notification: Notification) -> Result<(), HistoryError> {
        self.entries.push(notification);
        self.enforce_limits();
        self.flush()
    }

    /// Get all notifications (newest first).
    #[must_use]
    pub fn all(&self) -> Vec<&Notification> {
        let mut sorted: Vec<&Notification> = self.entries.iter().collect();
        sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        sorted
    }

    /// Get the N most recent notifications.
    #[must_use]
    pub fn recent(&self, limit: usize) -> Vec<&Notification> {
        self.all().into_iter().take(limit).collect()
    }

    /// Query notifications by app name (case-insensitive).
    #[must_use]
    pub fn by_app(&self, app_name: &str) -> Vec<&Notification> {
        let lower = app_name.to_lowercase();
        self.all()
            .into_iter()
            .filter(|n| n.app_name.to_lowercase() == lower)
            .collect()
    }

    /// Query notifications by urgency level.
    #[must_use]
    pub fn by_urgency(&self, urgency: Urgency) -> Vec<&Notification> {
        self.all()
            .into_iter()
            .filter(|n| n.urgency == urgency)
            .collect()
    }

    /// Full-text search across title and body (case-insensitive).
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&Notification> {
        let lower = query.to_lowercase();
        self.all()
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&lower)
                    || n.body.to_lowercase().contains(&lower)
            })
            .collect()
    }

    /// Mark a notification as read by ID.
    pub fn mark_read(&mut self, id: uuid::Uuid) -> Result<bool, HistoryError> {
        let found = self.entries.iter_mut().find(|n| n.id == id);
        if let Some(notif) = found {
            notif.read = true;
            self.flush()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Mark all notifications as read.
    pub fn mark_all_read(&mut self) -> Result<usize, HistoryError> {
        let mut count = 0;
        for notif in &mut self.entries {
            if !notif.read {
                notif.read = true;
                count += 1;
            }
        }
        if count > 0 {
            self.flush()?;
        }
        Ok(count)
    }

    /// Dismiss a notification by ID.
    pub fn dismiss(&mut self, id: uuid::Uuid) -> Result<bool, HistoryError> {
        let found = self.entries.iter_mut().find(|n| n.id == id);
        if let Some(notif) = found {
            notif.dismissed = true;
            self.flush()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Remove a notification from history entirely.
    pub fn remove(&mut self, id: uuid::Uuid) -> Result<bool, HistoryError> {
        let len_before = self.entries.len();
        self.entries.retain(|n| n.id != id);
        let removed = self.entries.len() < len_before;
        if removed {
            self.flush()?;
        }
        Ok(removed)
    }

    /// Clear all history entries.
    pub fn clear(&mut self) -> Result<(), HistoryError> {
        self.entries.clear();
        self.flush()
    }

    /// Clear history for a specific app.
    pub fn clear_app(&mut self, app_name: &str) -> Result<usize, HistoryError> {
        let lower = app_name.to_lowercase();
        let len_before = self.entries.len();
        self.entries
            .retain(|n| n.app_name.to_lowercase() != lower);
        let removed = len_before - self.entries.len();
        if removed > 0 {
            self.flush()?;
        }
        Ok(removed)
    }

    /// Total entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count of unread notifications.
    #[must_use]
    pub fn unread_count(&self) -> usize {
        self.entries.iter().filter(|n| !n.read).count()
    }

    /// Count of undismissed (active) notifications.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.entries.iter().filter(|n| !n.dismissed).count()
    }

    /// Path to the history file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Enforce max entries and retention limits.
    fn enforce_limits(&mut self) {
        // Remove expired entries
        let cutoff = Local::now()
            .checked_sub_days(Days::new(u64::from(self.retention_days)));
        if let Some(cutoff) = cutoff {
            self.entries.retain(|n| n.timestamp >= cutoff);
        }

        // Enforce max entry count (keep newest)
        if self.entries.len() > self.max_entries as usize {
            self.entries
                .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            self.entries.truncate(self.max_entries as usize);
        }
    }

    /// Write current state to disk.
    fn flush(&self) -> Result<(), HistoryError> {
        let json = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

/// Errors from the history store.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("corrupt history file: {0}")]
    Corrupt(String),
}

/// Get the default history file path.
#[must_use]
pub fn default_history_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("shirase")
        .join("history.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::Urgency;

    fn temp_store() -> (tempfile::TempDir, HistoryStore) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("history.json");
        let store = HistoryStore::open(&path, 100, 30).unwrap();
        (dir, store)
    }

    #[test]
    fn open_creates_new_store() {
        let (_dir, store) = temp_store();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn push_and_retrieve() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "Title", "Body")).unwrap();
        assert_eq!(store.len(), 1);
        let recent = store.recent(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "Title");
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("history.json");

        {
            let mut store = HistoryStore::open(&path, 100, 30).unwrap();
            store.push(Notification::new("App", "Persisted", "Body")).unwrap();
        }

        let store = HistoryStore::open(&path, 100, 30).unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store.recent(1)[0].title, "Persisted");
    }

    #[test]
    fn by_app_filter() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("Mail", "Email", "Body")).unwrap();
        store.push(Notification::new("Slack", "Message", "Body")).unwrap();
        store.push(Notification::new("Mail", "Email 2", "Body")).unwrap();

        let mail = store.by_app("Mail");
        assert_eq!(mail.len(), 2);
        let slack = store.by_app("slack"); // case-insensitive
        assert_eq!(slack.len(), 1);
    }

    #[test]
    fn by_urgency_filter() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "Low", "Body").with_urgency(Urgency::Low)).unwrap();
        store.push(Notification::new("App", "Critical", "Body").with_urgency(Urgency::Critical)).unwrap();
        store.push(Notification::new("App", "Normal", "Body")).unwrap();

        assert_eq!(store.by_urgency(Urgency::Critical).len(), 1);
        assert_eq!(store.by_urgency(Urgency::Normal).len(), 1);
        assert_eq!(store.by_urgency(Urgency::Low).len(), 1);
    }

    #[test]
    fn search_text() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "Meeting reminder", "Standup at 9")).unwrap();
        store.push(Notification::new("App", "Build", "CI passed")).unwrap();

        let results = store.search("meeting");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Meeting reminder");

        let results = store.search("PASSED"); // case-insensitive
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn mark_read() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "T", "B")).unwrap();
        let id = store.all()[0].id;

        assert_eq!(store.unread_count(), 1);
        assert!(store.mark_read(id).unwrap());
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn mark_all_read() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "T1", "B")).unwrap();
        store.push(Notification::new("App", "T2", "B")).unwrap();

        assert_eq!(store.unread_count(), 2);
        let count = store.mark_all_read().unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn dismiss_notification() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "T", "B")).unwrap();
        let id = store.all()[0].id;

        assert_eq!(store.active_count(), 1);
        assert!(store.dismiss(id).unwrap());
        assert_eq!(store.active_count(), 0);
    }

    #[test]
    fn remove_notification() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "T", "B")).unwrap();
        let id = store.all()[0].id;

        assert!(store.remove(id).unwrap());
        assert!(store.is_empty());
    }

    #[test]
    fn clear_all() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("App", "T1", "B")).unwrap();
        store.push(Notification::new("App", "T2", "B")).unwrap();
        store.clear().unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn clear_app() {
        let (_dir, mut store) = temp_store();
        store.push(Notification::new("Mail", "T1", "B")).unwrap();
        store.push(Notification::new("Slack", "T2", "B")).unwrap();
        store.push(Notification::new("Mail", "T3", "B")).unwrap();

        let removed = store.clear_app("Mail").unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn max_entries_enforced() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("history.json");
        let mut store = HistoryStore::open(&path, 3, 30).unwrap();

        for i in 0..5 {
            store.push(Notification::new("App", format!("N{i}"), "B")).unwrap();
        }
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn default_history_path_exists() {
        let path = default_history_path();
        assert!(path.to_string_lossy().contains("shirase"));
    }
}
