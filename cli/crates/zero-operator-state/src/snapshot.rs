//! A renderer-friendly snapshot of operator state.
//!
//! Widgets do not read the [`crate::StateVector`] directly; they read
//! a `Snapshot`. The snapshot includes the computed label, the
//! derived friction level, and a monotonically increasing `version`
//! so widgets can skip rendering when nothing has changed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::friction::{FrictionLevel, RiskContext};
use crate::label::Label;
use crate::vector::StateVector;

/// Cheap, clone-safe summary consumed by the TUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub label: Label,
    pub friction: FrictionLevel,
    pub vector: StateVector,
    /// When this snapshot was produced.
    pub as_of: DateTime<Utc>,
    /// Monotonic version number; widgets compare to their last seen
    /// and skip render when equal.
    pub version: u64,
}

impl Snapshot {
    /// Construct a snapshot whose `friction` is derived from
    /// `label` alone. Caps at L2 — call
    /// [`Snapshot::new_with_risk`] from a caller with engine
    /// access to reach L3/L4.
    #[must_use]
    pub fn new(label: Label, vector: StateVector, as_of: DateTime<Utc>, version: u64) -> Self {
        let friction = FrictionLevel::from_label(label);
        Self {
            label,
            friction,
            vector,
            as_of,
            version,
        }
    }

    /// Construct a snapshot whose `friction` is derived from both
    /// `label` **and** the engine-reported `risk`.
    ///
    /// This is the M2 entrypoint. It uses
    /// [`FrictionLevel::from_label_and_risk`] so the snapshot's
    /// `.friction` field can reach L3 (TILT + guardrail proximity)
    /// or L4 (TILT + halt). Callers without engine context (tests,
    /// pure replay, the classifier's default `classify` path) stay
    /// on [`Self::new`] and retain the L2 cap.
    #[must_use]
    pub fn new_with_risk(
        label: Label,
        vector: StateVector,
        as_of: DateTime<Utc>,
        version: u64,
        risk: RiskContext,
    ) -> Self {
        let friction = FrictionLevel::from_label_and_risk(label, risk);
        Self {
            label,
            friction,
            vector,
            as_of,
            version,
        }
    }
}
