//! Event model — what `events` rows look like in Rust.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Kinds the schema recognises. Mirrors the TUI's `EntryKind` plus
/// a `mode_change` variant used when the operator switches view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Prompt,
    System,
    Command,
    Warn,
    Alert,
    ModeChange,
}

impl EventKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::System => "system",
            Self::Command => "command",
            Self::Warn => "warn",
            Self::Alert => "alert",
            Self::ModeChange => "mode_change",
        }
    }

    /// Parse the SQL enum string back. Returns `None` on unknown
    /// input — the schema CHECK already rejects bad rows, but the
    /// Rust-side guard protects against schema drift post-migration.
    ///
    /// Deliberately not `FromStr` because the error type (`()`) we
    /// would return is less useful than `Option`; callers always
    /// handle "unknown" identically regardless.
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        Some(match s {
            "prompt" => Self::Prompt,
            "system" => Self::System,
            "command" => Self::Command,
            "warn" => Self::Warn,
            "alert" => Self::Alert,
            "mode_change" => Self::ModeChange,
            _ => return None,
        })
    }
}

/// A persisted event as it comes out of the DB. `seq` is the
/// per-session monotonic counter; ordering by `seq` is the
/// canonical replay order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    pub id: i64,
    pub session_id: i64,
    pub seq: i64,
    pub at: DateTime<Utc>,
    pub kind: EventKind,
    pub text: String,
}

/// Minimal session row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub id: i64,
    pub ulid: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub engine_base_url: Option<String>,
    pub cli_version: String,
    pub parent_ulid: Option<String>,
}
