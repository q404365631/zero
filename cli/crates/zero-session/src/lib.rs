//! SQLite-backed session persistence.
//!
//! One database at `~/.zero/state.db`, WAL-journalled. Every
//! operator input and every dispatcher output is written on the
//! hot path so a crash preserves full replay (spec v2.1 §9).
//!
//! This crate deliberately does NOT persist operator-state events
//! — those live on the engine host (see ADR-016). The tables here
//! are CLI-local: conversation log and journey milestones.

#![allow(clippy::module_name_repetitions)]

pub mod event;
pub mod store;
pub mod wrap;

use thiserror::Error;

pub use event::{EventKind, SessionRow, StoredEvent};
pub use store::Store;
pub use wrap::{CommandCount, EventCounts, WrapReport};

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("migration: {0}")]
    Migration(String),
    #[error("serialize: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Milestone key constants. Keep them as `&'static str` here so a
/// typo-prone string literal never leaks into consumers.
pub mod milestones {
    /// The honest-welcome has been shown at least once.
    pub const WELCOME_SHOWN: &str = "welcome_shown";
    /// ISO-8601 timestamp of the operator's first live trade, set
    /// once and then never rewritten. Drives the first-live-trade
    /// ceremony.
    pub const FIRST_LIVE_TRADE_AT: &str = "first_live_trade_at";
    /// Last time the daily wrap was rendered to the operator.
    pub const LAST_DAILY_WRAP_AT: &str = "last_daily_wrap_at";
}
