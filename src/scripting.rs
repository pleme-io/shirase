//! Rhai scripting plugin system.
//!
//! Loads user scripts from `~/.config/shirase/scripts/*.rhai` and registers
//! app-specific functions for notification management automation.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use soushi::ScriptEngine;

/// Event hooks that scripts can define.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptEvent {
    /// Fired when the app starts.
    OnStart,
    /// Fired when the app is quitting.
    OnQuit,
    /// Fired on key press with the key name.
    OnKey(String),
}

/// Manages the Rhai scripting engine with shirase-specific functions.
pub struct ShiraseScriptEngine {
    engine: ScriptEngine,
    /// Shared state for script-triggered actions.
    pub pending_actions: Arc<Mutex<Vec<ScriptAction>>>,
}

/// Actions that scripts can trigger.
#[derive(Debug, Clone)]
pub enum ScriptAction {
    /// Send a notification.
    Send { title: String, body: String },
    /// Dismiss all notifications.
    DismissAll,
    /// Toggle Do Not Disturb.
    Dnd(bool),
}

impl ShiraseScriptEngine {
    /// Create a new scripting engine with shirase-specific functions registered.
    #[must_use]
    pub fn new() -> Self {
        let mut engine = ScriptEngine::new();
        engine.register_builtin_log();
        engine.register_builtin_env();
        engine.register_builtin_string();

        let pending = Arc::new(Mutex::new(Vec::<ScriptAction>::new()));

        // Register shirase.send(title, body)
        let p = Arc::clone(&pending);
        engine.register_fn("shirase_send", move |title: &str, body: &str| {
            if let Ok(mut actions) = p.lock() {
                actions.push(ScriptAction::Send {
                    title: title.to_string(),
                    body: body.to_string(),
                });
            }
        });

        // Register shirase.dismiss_all()
        let p = Arc::clone(&pending);
        engine.register_fn("shirase_dismiss_all", move || {
            if let Ok(mut actions) = p.lock() {
                actions.push(ScriptAction::DismissAll);
            }
        });

        // Register shirase.dnd(enabled)
        let p = Arc::clone(&pending);
        engine.register_fn("shirase_dnd", move |enabled: bool| {
            if let Ok(mut actions) = p.lock() {
                actions.push(ScriptAction::Dnd(enabled));
            }
        });

        // Register shirase.get_count() — returns 0 (placeholder for live state)
        engine.register_fn("shirase_get_count", || -> i64 {
            0
        });

        Self {
            engine,
            pending_actions: pending,
        }
    }

    /// Load scripts from the default config directory.
    pub fn load_user_scripts(&mut self) {
        let scripts_dir = scripts_dir();
        if scripts_dir.is_dir() {
            match self.engine.load_scripts_dir(&scripts_dir) {
                Ok(names) => {
                    if !names.is_empty() {
                        tracing::info!(count = names.len(), "loaded shirase scripts: {names:?}");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to load shirase scripts");
                }
            }
        }
    }

    /// Fire an event hook.
    pub fn fire_event(&self, event: &ScriptEvent) {
        let hook_name = match event {
            ScriptEvent::OnStart => "on_start",
            ScriptEvent::OnQuit => "on_quit",
            ScriptEvent::OnKey(_) => "on_key",
        };

        let script = match event {
            ScriptEvent::OnKey(key) => format!("if is_def_fn(\"{hook_name}\", 1) {{ {hook_name}(\"{key}\"); }}"),
            _ => format!("if is_def_fn(\"{hook_name}\", 0) {{ {hook_name}(); }}"),
        };

        if let Err(e) = self.engine.eval(&script) {
            tracing::debug!(hook = hook_name, error = %e, "script hook not defined or failed");
        }
    }

    /// Drain any pending actions triggered by scripts.
    pub fn drain_actions(&self) -> Vec<ScriptAction> {
        if let Ok(mut actions) = self.pending_actions.lock() {
            actions.drain(..).collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for ShiraseScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Default scripts directory: `~/.config/shirase/scripts/`.
fn scripts_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("shirase")
        .join("scripts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_creation() {
        let _engine = ShiraseScriptEngine::new();
    }

    #[test]
    fn send_action() {
        let engine = ShiraseScriptEngine::new();
        engine
            .engine
            .eval(r#"shirase_send("Alert", "Something happened")"#)
            .unwrap();
        let actions = engine.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], ScriptAction::Send { title, body } if title == "Alert" && body == "Something happened")
        );
    }

    #[test]
    fn dismiss_all_action() {
        let engine = ShiraseScriptEngine::new();
        engine.engine.eval("shirase_dismiss_all()").unwrap();
        let actions = engine.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ScriptAction::DismissAll));
    }

    #[test]
    fn dnd_action() {
        let engine = ShiraseScriptEngine::new();
        engine.engine.eval("shirase_dnd(true)").unwrap();
        let actions = engine.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ScriptAction::Dnd(true)));
    }

    #[test]
    fn get_count_returns_int() {
        let engine = ShiraseScriptEngine::new();
        let result = engine.engine.eval("shirase_get_count()").unwrap();
        assert_eq!(result.as_int().unwrap(), 0);
    }

    #[test]
    fn fire_event_does_not_panic() {
        let engine = ShiraseScriptEngine::new();
        engine.fire_event(&ScriptEvent::OnStart);
        engine.fire_event(&ScriptEvent::OnQuit);
        engine.fire_event(&ScriptEvent::OnKey("d".to_string()));
    }

    #[test]
    fn drain_actions_clears() {
        let engine = ShiraseScriptEngine::new();
        engine
            .engine
            .eval(r#"shirase_send("Test", "Body")"#)
            .unwrap();
        assert_eq!(engine.drain_actions().len(), 1);
        assert!(engine.drain_actions().is_empty());
    }
}
