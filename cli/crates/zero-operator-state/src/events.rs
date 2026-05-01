//! The event stream fed into the classifier.
//!
//! Events are recorded in `~/.zero/state/events.log` and replayed on
//! start. The enum is exhaustive on purpose: adding a new event type
//! fails every match arm in the classifier until the author decides
//! how it maps to the state vector.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where a trading decision originated.
///
/// Used to compute the override rate in §2.1 and to separate operator
/// initiative from automated flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Operator accepted a Plan-mode verdict without modification.
    Plan,
    /// Engine executed under Auto mode with no operator touch.
    Auto,
    /// Operator executed under Headless policy.
    Headless,
    /// Operator override — rejected Plan's recommendation and acted
    /// anyway. The strongest signal for deviation-rate.
    Override,
    /// Operator-initiated trade not tied to any engine proposal.
    Manual,
}

/// Outcome of a completed trade. Used for loss-reaction timing and
/// the conviction-calibration report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Win,
    Loss,
    Scratch,
}

/// The event kinds the classifier understands.
///
/// New variants added here force every match arm in `classifier.rs`
/// to decide how the event affects the vector — the compiler becomes
/// the reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    /// Operator took an action that could open or modify a position.
    DecisionMade { symbol: String, source: Source },
    /// A position closed. `loss_reaction_ms` is filled when the prior
    /// event was also a close on the same symbol.
    TradeClosed {
        symbol: String,
        outcome: Outcome,
        pnl_r: f64,
        conviction: Option<u8>,
    },
    /// Operator hit `/break` (or a similar risk-reducing rest).
    BreakStarted { planned_ms: Option<u64> },
    /// Break ended (timer, keypress, or session resume).
    BreakEnded,
    /// Operator has been idle for more than the sleep-proxy threshold.
    Idle { since_ms: u64 },
    /// Operator returned from idle.
    Resumed,
    /// Plan-mode verdict shown to the operator (count of how many
    /// they've seen drives the override-rate denominator).
    VerdictShown,
    /// Plan-mode verdict was explicitly rejected by the operator.
    VerdictOverridden,
    /// Session began (launch or resume).
    SessionStarted,
    /// Session ended.
    SessionEnded,
    /// Operator-supplied conviction rating for a past trade
    /// (`/rate <trade_id> <1..=10>`). The classifier does not
    /// attribute the rating back onto the original `TradeClosed`
    /// variant because the two events are separated by human
    /// latency — merging them would force the classifier to
    /// carry a mutable trade index. Keeping `Conviction` as its
    /// own event lets the downstream consumer (operator-state
    /// engine POST, future calibration overlay) join on
    /// `trade_id` without the classifier needing to know how.
    ///
    /// `rating` is a `u8` in `1..=10` — the parser enforces the
    /// range before pushing. `trade_id` is the engine's opaque
    /// trade identifier, never parsed CLI-side.
    Conviction { trade_id: String, rating: u8 },
}

/// Wall-clock-timestamped event. Instances are the only thing that
/// [`crate::Classifier`] consumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    #[must_use]
    pub fn new(ts: DateTime<Utc>, kind: EventKind) -> Self {
        Self { ts, kind }
    }
}
