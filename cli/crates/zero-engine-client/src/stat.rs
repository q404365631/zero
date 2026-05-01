//! `Stat<T>` — the honesty primitive.
//!
//! Every numeric value the TUI renders passes through this type. The
//! renderer refuses to display a `Stat` whose freshness violates the
//! configured threshold, and it always shows `n` and `source` when
//! relevant.
//!
//! See spec §3.1 ("honesty is render-native") and ADR-003.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where a value came from. Used in rendering for source attribution
/// and in debugging to trace drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// HTTP REST response from the engine.
    Http,
    /// WebSocket push from the engine bus poller.
    Ws,
    /// MCP tool call response.
    Mcp,
    /// Derived on CLI side from other `Stat`s (presentation only).
    Derived,
    /// Fixture or mock — never rendered in production.
    Mock,
}

/// A value with the metadata required to render honestly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stat<T> {
    /// The actual value.
    pub value: T,
    /// When the engine produced this reading.
    pub as_of: DateTime<Utc>,
    /// Sample size, when the value is a summary statistic. `None` when
    /// the value is a live reading (price, position size, etc.).
    pub n: Option<u64>,
    /// Where it came from.
    pub source: Source,
}

impl<T> Stat<T> {
    pub fn new(value: T, source: Source) -> Self {
        Self {
            value,
            as_of: Utc::now(),
            n: None,
            source,
        }
    }

    #[must_use]
    pub fn with_n(mut self, n: u64) -> Self {
        self.n = Some(n);
        self
    }

    #[must_use]
    pub fn with_as_of(mut self, as_of: DateTime<Utc>) -> Self {
        self.as_of = as_of;
        self
    }

    /// Age of the reading at the given instant.
    #[must_use]
    pub fn age(&self, now: DateTime<Utc>) -> chrono::Duration {
        now.signed_duration_since(self.as_of)
    }

    #[must_use]
    pub fn is_stale(&self, now: DateTime<Utc>, threshold: chrono::Duration) -> bool {
        self.age(now) > threshold
    }
}
