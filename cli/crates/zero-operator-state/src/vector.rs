//! The numeric state vector. Fed by [`Classifier`], read by
//! widgets (`zero-tui`) and the friction path (`zero-commands`).
//!
//! Every field is computed; none is hand-edited. The operator can
//! reset the persisted copy (with confirmation) via `/state reset`,
//! but they cannot poke at individual numbers. "Never operator-faked"
//! is the property — see Addendum A §2.2.
//!
//! [`Classifier`]: crate::Classifier

use serde::{Deserialize, Serialize};

/// Decision velocity over rolling windows, in decisions per hour.
/// Addendum A §2.1, §2.3.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Velocity {
    pub last_1h: u32,
    pub last_4h: u32,
    pub last_24h: u32,
    /// Operator's personal baseline decisions/h, derived from the
    /// last 30 days of session data. Present when enough history
    /// exists.
    pub baseline_1h: Option<f64>,
}

impl Velocity {
    #[must_use]
    pub fn ratio_to_baseline(&self) -> Option<f64> {
        self.baseline_1h
            .filter(|b| *b > 0.0)
            .map(|b| f64::from(self.last_1h) / b)
    }
}

/// Strategy-deviation rate over last N verdicts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Deviation {
    pub overrides_last_10: u32,
    pub verdicts_last_10: u32,
    pub overrides_last_50: u32,
    pub verdicts_last_50: u32,
}

impl Deviation {
    /// Override rate over the last 10 verdicts (0.0-1.0).
    #[must_use]
    pub fn rate_last_10(&self) -> f64 {
        if self.verdicts_last_10 == 0 {
            0.0
        } else {
            f64::from(self.overrides_last_10) / f64::from(self.verdicts_last_10)
        }
    }
}

/// Session duration and focus metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Continuous active-interaction time since session start.
    pub active_duration_ms: u64,
    /// Longest uninterrupted focus block.
    pub longest_focus_ms: u64,
    /// Time since the last break or sleep-proxy rest.
    pub since_last_break_ms: u64,
}

/// Loss-reaction profile — time from a losing close to the next
/// entry. Addendum A §2.1, §10.2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LossReaction {
    /// Median time from loss → entry, over last 10 losses, ms.
    pub median_last_10_ms: u64,
    /// Fastest loss-reaction in current session, ms.
    pub fastest_session_ms: u64,
    /// Operator's personal baseline median, ms.
    pub baseline_ms: Option<u64>,
}

/// Same-symbol re-entry counts, by time window. Addendum A §10.2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ReEntry {
    pub within_15m: u32,
    pub within_30m: u32,
    pub within_2h: u32,
}

/// Sleep-proxy: how long since the last >6h input-free window ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SleepProxy {
    pub hours_since_rest_ended: Option<u32>,
}

/// The composite state vector — all the ingredients the classifier
/// folds into a [`crate::Label`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StateVector {
    pub velocity: Velocity,
    pub deviation: Deviation,
    pub session: Session,
    pub loss_reaction: LossReaction,
    pub re_entry: ReEntry,
    pub sleep_proxy: SleepProxy,
    /// True when the most recent `BreakStarted` has not yet been
    /// followed by `BreakEnded`. While on break, velocity counters
    /// freeze.
    pub on_break: bool,
}
