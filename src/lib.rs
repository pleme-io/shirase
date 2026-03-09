//! Shirase (知らせ) — GPU notification center.
//!
//! Provides notification reception via Unix socket IPC, persistent history,
//! Do-Not-Disturb with scheduling, filtering by app/urgency, application
//! grouping, and vim-style keyboard navigation.

pub mod config;
pub mod daemon;
pub mod filter;
pub mod history;
pub mod input;
pub mod notification;
pub mod platform;
pub mod render;
