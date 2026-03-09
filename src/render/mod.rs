//! Terminal-based rendering for the notification center.
//!
//! Renders notification lists, groups, search results, and status to stdout.
//! This is a text-mode renderer suitable for both inline CLI display and
//! future GPU rendering integration via garasu/madori/egaku.

use crate::notification::{group_by_app, Notification, NotificationGroup, Urgency};
use crate::filter::DndState;
use crate::input::{InputMode, UiState};

/// Render the notification center header.
pub fn render_header(dnd: &DndState, unread: usize, total: usize) {
    let dnd_status = if dnd.is_active() { "ON" } else { "OFF" };
    println!();
    println!(
        "  Notifications  [DnD: {dnd_status}]  [{unread} unread / {total} total]"
    );
    println!("  {}", "-".repeat(54));
}

/// Render notification groups.
pub fn render_groups(groups: &[NotificationGroup], ui: &UiState) {
    if groups.is_empty() {
        println!("  No notifications.");
        return;
    }

    let mut global_index = 0;
    for group in groups {
        println!();
        println!(
            "  {} ({}){}",
            group.app_name,
            group.count(),
            if group.unread_count() > 0 {
                format!("  [{} unread]", group.unread_count())
            } else {
                String::new()
            }
        );
        println!("  {}", "-".repeat(50));

        for notif in &group.notifications {
            let is_selected = global_index == ui.selected_index;
            render_notification(notif, is_selected, is_selected && ui.expanded);
            global_index += 1;
        }
    }
}

/// Render a flat list of notifications (for search results).
pub fn render_notification_list(notifications: &[&Notification], ui: &UiState) {
    if notifications.is_empty() {
        println!("  No results.");
        return;
    }

    for (i, notif) in notifications.iter().enumerate() {
        let is_selected = i == ui.selected_index;
        render_notification(notif, is_selected, is_selected && ui.expanded);
    }
}

/// Render a single notification line.
fn render_notification(notif: &Notification, selected: bool, expanded: bool) {
    let cursor = if selected { ">" } else { " " };
    let read_marker = if notif.read { " " } else { "*" };
    let urgency_marker = match notif.urgency {
        Urgency::Low => " ",
        Urgency::Normal => " ",
        Urgency::Critical => "!",
    };
    let time = notif.timestamp.format("%H:%M");
    let dismissed = if notif.dismissed { " [dismissed]" } else { "" };

    println!(
        "  {cursor}{read_marker}{urgency_marker} {title:<40} {time}{dismissed}",
        title = truncate(&notif.title, 40),
    );

    if expanded {
        println!("      App: {}", notif.app_name);
        println!("      Urgency: {}", notif.urgency);
        println!("      ID: {}", notif.id);
        if !notif.body.is_empty() {
            for line in notif.body.lines() {
                println!("      {line}");
            }
        }
        println!();
    }
}

/// Render the mode indicator / input line.
pub fn render_mode_line(ui: &UiState) {
    println!();
    match &ui.mode {
        InputMode::Normal => {
            println!("  [Normal] j/k:nav  d:dismiss  D:all  n:dnd  /:search  q:quit  ?:help");
        }
        InputMode::History => {
            println!("  [History] j/k:nav  d:delete  c:clear  /:search  u:urgency  Esc:back");
        }
        InputMode::Search => {
            println!("  [Search] /{}", ui.input_buffer);
        }
        InputMode::Command => {
            println!("  [Command] :{}", ui.input_buffer);
        }
    }
}

/// Render a status summary (for CLI `status` command).
pub fn render_status(
    dnd: &DndState,
    total: usize,
    unread: usize,
    active: usize,
    groups: &[NotificationGroup],
) {
    println!("Shirase Notification Center");
    println!();
    println!("  DnD:     {}", if dnd.is_active() { "ON" } else { "OFF" });
    if dnd.is_manually_active() {
        println!("  DnD mode: manual");
    }
    if dnd.is_quiet_hours() {
        println!("  DnD mode: quiet hours");
    }
    println!("  Total:   {total}");
    println!("  Unread:  {unread}");
    println!("  Active:  {active}");
    println!();

    if !groups.is_empty() {
        println!("  Groups:");
        for group in groups {
            println!(
                "    {}: {} ({} unread)",
                group.app_name,
                group.count(),
                group.unread_count()
            );
        }
    }
}

/// Render the full notification center view.
pub fn render_center(
    notifications: &[Notification],
    dnd: &DndState,
    ui: &UiState,
) {
    // Clear screen (ANSI)
    print!("\x1b[2J\x1b[H");

    let unread = notifications.iter().filter(|n| !n.read).count();
    render_header(dnd, unread, notifications.len());

    let active: Vec<Notification> = notifications
        .iter()
        .filter(|n| !n.dismissed)
        .cloned()
        .collect();

    let groups = group_by_app(&active);
    render_groups(&groups, ui);
    render_mode_line(ui);
}

/// Truncate a string to max length, appending "..." if needed.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long() {
        let result = truncate("this is a very long title that needs truncation", 20);
        assert!(result.len() <= 20);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn truncate_exact() {
        assert_eq!(truncate("12345", 5), "12345");
    }
}
