//! Keyboard input handling with vim-style navigation.
//!
//! Defines input modes (Normal, History, Search, Command) and key bindings
//! for navigating the notification center.
//!
//! Key binding definitions use awase types for platform-agnostic hotkey
//! representation and serializable binding configuration.

use awase::{Hotkey, Key as AwaseKey, Modifiers as AwaseMods};
use uuid::Uuid;

/// A keybinding definition: an awase `Hotkey` paired with an action name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyBinding {
    /// The hotkey that triggers this binding (awase type).
    pub hotkey: Hotkey,
    /// The action name to perform.
    pub action: String,
}

/// Default keybindings using awase `Hotkey` types.
#[must_use]
pub fn default_bindings() -> Vec<KeyBinding> {
    vec![
        // Normal mode
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::J), action: "move_down".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::K), action: "move_up".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::D), action: "dismiss".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::SHIFT, AwaseKey::D), action: "dismiss_all".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::N), action: "toggle_dnd".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::C), action: "clear_history".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::F), action: "filter_by_app".into() },
        // Note: '/' key — awase Key enum doesn't have Slash yet,
        // so search is bound at the runtime dispatch layer only.
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::R), action: "mark_read".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::SHIFT, AwaseKey::R), action: "mark_all_read".into() },
        KeyBinding { hotkey: Hotkey::new(AwaseMods::NONE, AwaseKey::Q), action: "quit".into() },
        // Ctrl+C
        KeyBinding { hotkey: Hotkey::new(AwaseMods::CTRL, AwaseKey::C), action: "quit".into() },
    ]
}

/// Input mode for the notification center.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    /// Normal mode: navigate current notifications.
    Normal,
    /// History mode: navigate past notifications.
    History,
    /// Search mode: typing a search query.
    Search,
    /// Command mode: typing a command (`:` prefix).
    Command,
}

impl Default for InputMode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Actions that can be triggered by keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Move selection up.
    MoveUp,
    /// Move selection down.
    MoveDown,
    /// Expand/collapse the selected notification.
    ToggleExpand,
    /// Dismiss the selected notification.
    Dismiss,
    /// Dismiss all visible notifications.
    DismissAll,
    /// Toggle Do Not Disturb.
    ToggleDnd,
    /// Clear all history.
    ClearHistory,
    /// Enter filter-by-app mode.
    FilterByApp,
    /// Filter by urgency.
    FilterByUrgency,
    /// Start search.
    StartSearch,
    /// Mark selected as read.
    MarkRead,
    /// Mark all as read.
    MarkAllRead,
    /// Switch between current/history view.
    ToggleView,
    /// Close the notification center.
    Quit,
    /// Enter command mode.
    EnterCommand,
    /// Delete entry from history.
    DeleteEntry,
    /// Go back / escape current mode.
    Back,
    /// Submit current search or command input.
    Submit,
    /// Append a character to the input buffer.
    AppendChar(char),
    /// Delete last character from input buffer.
    Backspace,
    /// No action.
    None,
}

/// Map a key press to an action based on the current input mode.
#[must_use]
pub fn map_key(mode: &InputMode, key: char, ctrl: bool) -> Action {
    if ctrl {
        return match key {
            'c' => Action::Quit,
            _ => Action::None,
        };
    }

    match mode {
        InputMode::Normal => map_normal_key(key),
        InputMode::History => map_history_key(key),
        InputMode::Search | InputMode::Command => map_input_key(key),
    }
}

/// Map a special key (non-character) to an action.
#[must_use]
pub fn map_special_key(mode: &InputMode, key: SpecialKey) -> Action {
    match key {
        SpecialKey::Enter => match mode {
            InputMode::Normal | InputMode::History => Action::ToggleExpand,
            InputMode::Search | InputMode::Command => Action::Submit,
        },
        SpecialKey::Escape => match mode {
            InputMode::Normal => Action::Quit,
            InputMode::History => Action::Back,
            InputMode::Search | InputMode::Command => Action::Back,
        },
        SpecialKey::Backspace => match mode {
            InputMode::Search | InputMode::Command => Action::Backspace,
            _ => Action::None,
        },
        SpecialKey::Tab => Action::ToggleView,
        SpecialKey::Up => Action::MoveUp,
        SpecialKey::Down => Action::MoveDown,
    }
}

/// Special (non-character) keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecialKey {
    Enter,
    Escape,
    Backspace,
    Tab,
    Up,
    Down,
}

fn map_normal_key(key: char) -> Action {
    match key {
        'j' => Action::MoveDown,
        'k' => Action::MoveUp,
        'd' => Action::Dismiss,
        'D' => Action::DismissAll,
        'n' => Action::ToggleDnd,
        'c' => Action::ClearHistory,
        'f' => Action::FilterByApp,
        '/' => Action::StartSearch,
        'r' => Action::MarkRead,
        'R' => Action::MarkAllRead,
        'q' => Action::Quit,
        ':' => Action::EnterCommand,
        _ => Action::None,
    }
}

fn map_history_key(key: char) -> Action {
    match key {
        'j' => Action::MoveDown,
        'k' => Action::MoveUp,
        'd' => Action::DeleteEntry,
        'c' => Action::ClearHistory,
        'f' => Action::FilterByApp,
        'u' => Action::FilterByUrgency,
        '/' => Action::StartSearch,
        'q' => Action::Back,
        _ => Action::None,
    }
}

fn map_input_key(key: char) -> Action {
    Action::AppendChar(key)
}

/// State for the notification center UI.
#[derive(Debug)]
pub struct UiState {
    /// Current input mode.
    pub mode: InputMode,
    /// Index of the selected notification in the current view.
    pub selected_index: usize,
    /// Total number of items in the current view.
    pub total_items: usize,
    /// Whether the selected notification is expanded.
    pub expanded: bool,
    /// Current search/command input buffer.
    pub input_buffer: String,
    /// Current app filter (None = show all).
    pub app_filter: Option<String>,
    /// Current urgency filter (None = show all).
    pub urgency_filter: Option<crate::notification::Urgency>,
    /// IDs of notifications in the current view (for mapping index to ID).
    pub visible_ids: Vec<Uuid>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            mode: InputMode::Normal,
            selected_index: 0,
            total_items: 0,
            expanded: false,
            input_buffer: String::new(),
            app_filter: None,
            urgency_filter: None,
            visible_ids: Vec::new(),
        }
    }
}

impl UiState {
    /// Move selection up, wrapping at the top.
    pub fn move_up(&mut self) {
        if self.total_items == 0 {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.total_items.saturating_sub(1);
        } else {
            self.selected_index -= 1;
        }
        self.expanded = false;
    }

    /// Move selection down, wrapping at the bottom.
    pub fn move_down(&mut self) {
        if self.total_items == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.total_items;
        self.expanded = false;
    }

    /// Get the ID of the currently selected notification.
    #[must_use]
    pub fn selected_id(&self) -> Option<Uuid> {
        self.visible_ids.get(self.selected_index).copied()
    }

    /// Reset selection to the beginning.
    pub fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.expanded = false;
    }

    /// Enter search mode.
    pub fn enter_search(&mut self) {
        self.mode = InputMode::Search;
        self.input_buffer.clear();
    }

    /// Enter command mode.
    pub fn enter_command(&mut self) {
        self.mode = InputMode::Command;
        self.input_buffer.clear();
    }

    /// Exit back to normal/history mode.
    pub fn exit_input_mode(&mut self, previous: InputMode) {
        self.mode = previous;
        self.input_buffer.clear();
    }

    /// Update the visible items (call after any data change).
    pub fn update_visible(&mut self, ids: Vec<Uuid>) {
        self.visible_ids = ids;
        self.total_items = self.visible_ids.len();
        if self.selected_index >= self.total_items && self.total_items > 0 {
            self.selected_index = self.total_items - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_are_valid() {
        let bindings = default_bindings();
        assert!(!bindings.is_empty());
        let has_quit = bindings.iter().any(|b| b.action == "quit");
        assert!(has_quit, "should have a quit binding");
    }

    #[test]
    fn bindings_are_serializable() {
        let bindings = default_bindings();
        let json = serde_json::to_string(&bindings).unwrap();
        let deserialized: Vec<KeyBinding> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.len(), bindings.len());
    }

    #[test]
    fn normal_mode_keys() {
        assert_eq!(map_key(&InputMode::Normal, 'j', false), Action::MoveDown);
        assert_eq!(map_key(&InputMode::Normal, 'k', false), Action::MoveUp);
        assert_eq!(map_key(&InputMode::Normal, 'd', false), Action::Dismiss);
        assert_eq!(map_key(&InputMode::Normal, 'D', false), Action::DismissAll);
        assert_eq!(map_key(&InputMode::Normal, 'n', false), Action::ToggleDnd);
        assert_eq!(map_key(&InputMode::Normal, '/', false), Action::StartSearch);
        assert_eq!(map_key(&InputMode::Normal, 'q', false), Action::Quit);
        assert_eq!(map_key(&InputMode::Normal, ':', false), Action::EnterCommand);
        assert_eq!(map_key(&InputMode::Normal, 'r', false), Action::MarkRead);
        assert_eq!(map_key(&InputMode::Normal, 'R', false), Action::MarkAllRead);
    }

    #[test]
    fn history_mode_keys() {
        assert_eq!(map_key(&InputMode::History, 'j', false), Action::MoveDown);
        assert_eq!(map_key(&InputMode::History, 'k', false), Action::MoveUp);
        assert_eq!(map_key(&InputMode::History, 'd', false), Action::DeleteEntry);
        assert_eq!(map_key(&InputMode::History, '/', false), Action::StartSearch);
        assert_eq!(map_key(&InputMode::History, 'u', false), Action::FilterByUrgency);
    }

    #[test]
    fn search_mode_appends() {
        assert_eq!(
            map_key(&InputMode::Search, 'a', false),
            Action::AppendChar('a')
        );
    }

    #[test]
    fn ctrl_c_quits() {
        assert_eq!(map_key(&InputMode::Normal, 'c', true), Action::Quit);
        assert_eq!(map_key(&InputMode::Search, 'c', true), Action::Quit);
    }

    #[test]
    fn special_keys_normal() {
        assert_eq!(
            map_special_key(&InputMode::Normal, SpecialKey::Enter),
            Action::ToggleExpand
        );
        assert_eq!(
            map_special_key(&InputMode::Normal, SpecialKey::Escape),
            Action::Quit
        );
        assert_eq!(
            map_special_key(&InputMode::Normal, SpecialKey::Tab),
            Action::ToggleView
        );
    }

    #[test]
    fn special_keys_search() {
        assert_eq!(
            map_special_key(&InputMode::Search, SpecialKey::Enter),
            Action::Submit
        );
        assert_eq!(
            map_special_key(&InputMode::Search, SpecialKey::Escape),
            Action::Back
        );
        assert_eq!(
            map_special_key(&InputMode::Search, SpecialKey::Backspace),
            Action::Backspace
        );
    }

    #[test]
    fn ui_state_navigation() {
        let mut state = UiState::default();
        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        state.update_visible(ids.clone());

        assert_eq!(state.selected_index, 0);
        state.move_down();
        assert_eq!(state.selected_index, 1);
        state.move_down();
        assert_eq!(state.selected_index, 2);

        state.move_up();
        assert_eq!(state.selected_index, 1);

        // Wrap at top
        state.selected_index = 0;
        state.move_up();
        assert_eq!(state.selected_index, 4);

        // Wrap at bottom
        state.selected_index = 4;
        state.move_down();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn ui_state_selected_id() {
        let mut state = UiState::default();
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        state.update_visible(ids.clone());

        assert_eq!(state.selected_id(), Some(ids[0]));
        state.move_down();
        assert_eq!(state.selected_id(), Some(ids[1]));
    }

    #[test]
    fn ui_state_empty() {
        let mut state = UiState::default();
        state.update_visible(vec![]);

        assert_eq!(state.selected_id(), None);
        state.move_down(); // should not panic
        state.move_up(); // should not panic
    }

    #[test]
    fn ui_state_modes() {
        let mut state = UiState::default();
        assert_eq!(state.mode, InputMode::Normal);

        state.enter_search();
        assert_eq!(state.mode, InputMode::Search);
        assert!(state.input_buffer.is_empty());

        state.exit_input_mode(InputMode::Normal);
        assert_eq!(state.mode, InputMode::Normal);

        state.enter_command();
        assert_eq!(state.mode, InputMode::Command);
    }
}
