//! Friction ladder — **Addendum A §3 and §6.3.**
//!
//! # The invariant
//!
//! Every command carries a [`RiskDirection`]. Only `Increases` is
//! gated. `Reduces` is **always instant**. `Neutral` is passthrough.
//! Applying a friction gate to a `Reduces` command is a compile
//! error (see [`FrictionGate::apply`]). See ADR-014.
//!
//! This is the single most important invariant in the crate graph.
//! Violating it means a tired operator at 2 AM can't kill their
//! positions. That outcome is not allowed to be reachable from any
//! code path in this repository.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::label::Label;

/// Classification of a command's effect on operator risk.
///
/// See **Addendum A §6.3**. The enum is the declaration; the
/// invariant is enforced by [`FrictionGate::apply`] being generic
/// over this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskDirection {
    /// The command opens, enlarges, or unsafes a position. Subject
    /// to friction when operator state is not STEADY/FRESH/RECOVERY.
    Increases,
    /// The command closes, shrinks, or risk-offs. **Never gated.**
    /// `/kill`, `/flatten-all`, `/close`, `/pause-entries`, `/break`.
    Reduces,
    /// The command is informational or cosmetic. Passthrough.
    Neutral,
}

/// The friction escalator's current level.
///
/// Level definitions come straight from Addendum A §3.1. Every
/// variant is reachable: L0–L2 from [`FrictionLevel::from_label`]
/// alone, L3/L4 from [`FrictionLevel::from_label_and_risk`] when
/// the engine reports guardrail proximity or a halt flag alongside
/// a TILT label. See M2_PLAN §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrictionLevel {
    /// No friction — FRESH / STEADY / RECOVERY.
    L0,
    /// 3 s visible countdown before execute — ELEVATED.
    L1,
    /// 10 s pause + typed `execute` — TILT.
    L2,
    /// 30 s pause + typed re-read of the guardrail-proximity
    /// disclosure — TILT **and** engine-reported drawdown within
    /// [`RiskContext::PROXIMITY_PCT`] of the last alert threshold.
    /// The command still runs if the operator completes the
    /// re-read; this is a deliberate friction bump, not a refusal.
    L3,
    /// Hard refusal — TILT **and** the engine reports any halt
    /// flag (`risk.halted`, `global_halt`, `stop_failure_halt`).
    /// Risk-increasing commands are dropped; only `Reduces`
    /// (`/kill`, `/flatten-all`, `/close`, `/break`, …) continue
    /// to pass through the un-gated path. This is the dead-man
    /// switch — a tired operator at 2 AM must not be able to
    /// reach for a risk-increasing command while the engine is
    /// already halted.
    L4,
}

/// Engine-reported risk context the classifier/friction layer
/// consults to escalate TILT → L3 or L4.
///
/// Defaults describe "engine is healthy, no proximity alert" so a
/// caller without engine access (test harnesses, headless replay)
/// gets pre-M2 behaviour: L2 cap, no escalation. Every field is
/// optional / boolean; the escalation is strictly a one-way bump
/// (never down-graded below `from_label`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiskContext {
    /// Distance in percentage points between the engine's current
    /// `drawdown_pct` and its `last_drawdown_alert_pct` threshold.
    /// `None` when either field is missing from the engine mirror
    /// (no proximity signal → no L3 escalation).
    pub guardrail_proximity_pct: Option<f64>,
    /// Any of the halt booleans on `Risk` is set — see
    /// `Risk::is_halted`. Setting this alongside `Label::Tilt`
    /// escalates to L4.
    pub halted: bool,
}

impl RiskContext {
    /// Threshold (in percentage points) within which TILT escalates
    /// to L3. Taken from M2_PLAN §3 ("guardrail-proximity within 1
    /// percent of a hard limit"). The overlay-side threshold in §4
    /// is separately tuned (0.5 pp) — do not unify without updating
    /// both plan rows.
    pub const PROXIMITY_PCT: f64 = 1.0;

    /// Construct a context from the engine mirror's
    /// `drawdown_pct` + `last_drawdown_alert_pct` pair, plus a halt
    /// boolean. Returns the default (no-escalation) shape when
    /// either percentage field is missing.
    #[must_use]
    pub fn from_engine(
        drawdown_pct: Option<f64>,
        last_drawdown_alert_pct: Option<f64>,
        halted: bool,
    ) -> Self {
        let proximity = match (drawdown_pct, last_drawdown_alert_pct) {
            (Some(dd), Some(alert)) => Some((alert - dd).abs()),
            _ => None,
        };
        Self {
            guardrail_proximity_pct: proximity,
            halted,
        }
    }

    /// True when drawdown is within
    /// [`Self::PROXIMITY_PCT`] of the last alert threshold. False
    /// when the proximity reading is missing — the honest behaviour
    /// is "no proximity signal → no escalation", not "missing ⇒
    /// conservatively escalate", because the latter would penalise
    /// an engine restart that hasn't populated
    /// `last_drawdown_alert_pct` yet.
    #[must_use]
    pub fn near_guardrail(&self) -> bool {
        self.guardrail_proximity_pct
            .is_some_and(|pp| pp <= Self::PROXIMITY_PCT)
    }
}

impl FrictionLevel {
    /// Map a state [`Label`] to its default friction level.
    ///
    /// This form is capped at L2 and is the right call when the
    /// caller has no engine risk context — tests, the classifier's
    /// pure `classify(now)` entrypoint, replay harnesses. Use
    /// [`FrictionLevel::from_label_and_risk`] from the dispatcher,
    /// which does see the engine mirror, to reach L3/L4.
    #[must_use]
    pub const fn from_label(label: Label) -> Self {
        match label {
            Label::Fresh | Label::Steady | Label::Recovery => Self::L0,
            Label::Elevated | Label::Fatigued => Self::L1,
            Label::Tilt => Self::L2,
        }
    }

    /// Map a `(label, risk)` pair to the full friction level,
    /// including the M2 L3/L4 escalations.
    ///
    /// Escalation is one-way and TILT-gated:
    /// - `Label::Tilt` + [`RiskContext::halted`] → L4 (refusal).
    /// - `Label::Tilt` + [`RiskContext::near_guardrail`] → L3.
    /// - Otherwise, identical to [`Self::from_label`].
    ///
    /// L4 beats L3 when both conditions trip — the dead-man switch
    /// is the stronger signal.
    #[must_use]
    pub fn from_label_and_risk(label: Label, risk: RiskContext) -> Self {
        let base = Self::from_label(label);
        if matches!(label, Label::Tilt) {
            if risk.halted {
                return Self::L4;
            }
            if risk.near_guardrail() {
                return Self::L3;
            }
        }
        base
    }

    /// Required pause duration before execution.
    ///
    /// L3 is **30 s** — the spec's "mandatory pause + re-read the
    /// exact disclosure phrase" window. L4 is 15 min for the
    /// pathological case that the operator clears the halt in that
    /// window; in practice L4 is rendered as a refusal and never
    /// reaches a timer.
    #[must_use]
    pub const fn pause(self) -> Duration {
        match self {
            Self::L0 => Duration::ZERO,
            Self::L1 => Duration::from_secs(3),
            Self::L2 => Duration::from_secs(10),
            Self::L3 => Duration::from_secs(30),
            Self::L4 => Duration::from_secs(15 * 60),
        }
    }

    /// Whether the confirmation step requires typing a word/phrase
    /// rather than a single key. TILT and above switch away from
    /// single-key `e` to typed confirmations — `execute` at L2, the
    /// full proximity disclosure at L3 (§6.2, §3.2).
    #[must_use]
    pub const fn requires_typed_confirm(self) -> bool {
        matches!(self, Self::L2 | Self::L3 | Self::L4)
    }

    /// True when this level is a refusal — the command is dropped,
    /// no pause can redeem it. Only L4 is a refusal.
    ///
    /// This is the load-bearing check for "a tilted operator must
    /// not reach for a risk-increasing command while the engine is
    /// already halted". See `RiskContext` and M2_PLAN §3.
    #[must_use]
    pub const fn is_refusal(self) -> bool {
        matches!(self, Self::L4)
    }
}

/// A gate applied in front of a command. The generic `D` binds to
/// `RiskDirection` at the type level so only risk-increasing
/// commands can be gated.
#[derive(Debug, Clone, Copy)]
pub struct FrictionGate<D: GateableDirection> {
    level: FrictionLevel,
    _direction: std::marker::PhantomData<D>,
}

/// Sealed marker trait that lists the directions a gate may apply
/// to. Only `Increases` implements it. Attempting to parameterize
/// `FrictionGate` with `Reduces` or `Neutral` fails at compile time.
pub trait GateableDirection: sealed::Sealed {}

/// Phantom marker type representing `RiskDirection::Increases` at the
/// type level. Used to parameterize [`FrictionGate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Increases;

impl sealed::Sealed for Increases {}
impl GateableDirection for Increases {}

mod sealed {
    pub trait Sealed {}
}

impl FrictionGate<Increases> {
    /// Construct a gate from a friction level.
    #[must_use]
    pub const fn new(level: FrictionLevel) -> Self {
        Self {
            level,
            _direction: std::marker::PhantomData,
        }
    }

    /// Derive a gate from a state [`Label`].
    #[must_use]
    pub const fn for_label(label: Label) -> Self {
        Self::new(FrictionLevel::from_label(label))
    }

    #[must_use]
    pub const fn level(&self) -> FrictionLevel {
        self.level
    }

    /// Required pause before execution.
    #[must_use]
    pub const fn pause(&self) -> Duration {
        self.level.pause()
    }

    /// Whether the confirmation must be typed-word rather than
    /// single-key.
    #[must_use]
    pub const fn requires_typed_confirm(&self) -> bool {
        self.level.requires_typed_confirm()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_steady_recovery_have_no_pause() {
        for label in [Label::Fresh, Label::Steady, Label::Recovery] {
            let gate = FrictionGate::<Increases>::for_label(label);
            assert_eq!(gate.pause(), Duration::ZERO);
            assert!(!gate.requires_typed_confirm());
        }
    }

    #[test]
    fn elevated_and_fatigued_get_three_seconds() {
        for label in [Label::Elevated, Label::Fatigued] {
            let gate = FrictionGate::<Increases>::for_label(label);
            assert_eq!(gate.pause(), Duration::from_secs(3));
            assert!(!gate.requires_typed_confirm());
        }
    }

    #[test]
    fn tilt_requires_typed_confirm_and_ten_seconds() {
        let gate = FrictionGate::<Increases>::for_label(Label::Tilt);
        assert_eq!(gate.pause(), Duration::from_secs(10));
        assert!(gate.requires_typed_confirm());
    }

    // -------------------------------------------------------------
    // M2 §3: L3/L4 escalation via RiskContext.
    // -------------------------------------------------------------

    #[test]
    fn from_label_caps_at_l2_without_risk_context() {
        for label in [
            Label::Fresh,
            Label::Steady,
            Label::Elevated,
            Label::Fatigued,
            Label::Tilt,
            Label::Recovery,
        ] {
            assert!(
                FrictionLevel::from_label(label) <= FrictionLevel::L2,
                "from_label({label:?}) escaped the L2 cap — L3/L4 must flow through from_label_and_risk only"
            );
        }
    }

    #[test]
    fn non_tilt_labels_never_escalate_regardless_of_risk() {
        let haz = RiskContext {
            guardrail_proximity_pct: Some(0.1),
            halted: true,
        };
        for label in [
            Label::Fresh,
            Label::Steady,
            Label::Elevated,
            Label::Fatigued,
            Label::Recovery,
        ] {
            let level = FrictionLevel::from_label_and_risk(label, haz);
            assert_eq!(
                level,
                FrictionLevel::from_label(label),
                "label={label:?} must not escalate — TILT-gated invariant"
            );
        }
    }

    #[test]
    fn tilt_plus_halt_escalates_to_l4() {
        let ctx = RiskContext {
            guardrail_proximity_pct: None,
            halted: true,
        };
        assert_eq!(
            FrictionLevel::from_label_and_risk(Label::Tilt, ctx),
            FrictionLevel::L4
        );
    }

    #[test]
    fn tilt_plus_proximity_escalates_to_l3() {
        let ctx = RiskContext {
            guardrail_proximity_pct: Some(0.5),
            halted: false,
        };
        assert_eq!(
            FrictionLevel::from_label_and_risk(Label::Tilt, ctx),
            FrictionLevel::L3
        );
    }

    #[test]
    fn tilt_plus_halt_beats_tilt_plus_proximity() {
        let ctx = RiskContext {
            guardrail_proximity_pct: Some(0.1),
            halted: true,
        };
        assert_eq!(
            FrictionLevel::from_label_and_risk(Label::Tilt, ctx),
            FrictionLevel::L4
        );
    }

    #[test]
    fn tilt_with_distant_proximity_stays_at_l2() {
        let ctx = RiskContext {
            guardrail_proximity_pct: Some(RiskContext::PROXIMITY_PCT + 0.01),
            halted: false,
        };
        assert_eq!(
            FrictionLevel::from_label_and_risk(Label::Tilt, ctx),
            FrictionLevel::L2
        );
    }

    #[test]
    fn tilt_without_any_risk_signal_stays_at_l2() {
        assert_eq!(
            FrictionLevel::from_label_and_risk(Label::Tilt, RiskContext::default()),
            FrictionLevel::L2
        );
    }

    #[test]
    fn proximity_is_inclusive_at_the_threshold() {
        let ctx = RiskContext {
            guardrail_proximity_pct: Some(RiskContext::PROXIMITY_PCT),
            halted: false,
        };
        assert_eq!(
            FrictionLevel::from_label_and_risk(Label::Tilt, ctx),
            FrictionLevel::L3,
            "proximity at exactly the threshold must escalate"
        );
    }

    #[test]
    fn risk_context_from_engine_computes_absolute_distance() {
        // Use integer-exact pairs so we don't have to reason about
        // binary-floating-point rounding in the assertion.
        let ctx = RiskContext::from_engine(Some(4.0), Some(5.0), false);
        assert_eq!(ctx.guardrail_proximity_pct, Some(1.0));
        assert!(ctx.near_guardrail());

        let ctx_reversed = RiskContext::from_engine(Some(5.0), Some(4.0), false);
        assert_eq!(
            ctx_reversed.guardrail_proximity_pct,
            Some(1.0),
            "from_engine must be sign-symmetric — absolute distance only"
        );
    }

    #[test]
    fn risk_context_from_engine_drops_proximity_when_either_field_missing() {
        let ctx = RiskContext::from_engine(None, Some(5.0), false);
        assert_eq!(ctx.guardrail_proximity_pct, None);
        assert!(!ctx.near_guardrail());

        let ctx = RiskContext::from_engine(Some(5.0), None, false);
        assert_eq!(ctx.guardrail_proximity_pct, None);
        assert!(!ctx.near_guardrail());
    }

    #[test]
    fn l3_pauses_thirty_seconds_and_l4_refuses() {
        assert_eq!(FrictionLevel::L3.pause(), Duration::from_secs(30));
        assert!(FrictionLevel::L3.requires_typed_confirm());
        assert!(!FrictionLevel::L3.is_refusal());

        assert!(FrictionLevel::L4.is_refusal());
        assert!(FrictionLevel::L4.requires_typed_confirm());
    }

    /// The risk-asymmetry invariant — Addendum A §6.3.
    ///
    /// This doctest must not compile. If someone loosens the trait
    /// bound on `FrictionGate` to accept `Reduces` or `Neutral`, this
    /// doctest starts compiling and the CI gate catches it. That is
    /// the only thing standing between a tired operator and a gated
    /// `/kill`.
    ///
    /// ```compile_fail
    /// use zero_operator_state::friction::{FrictionGate, FrictionLevel};
    /// // `Reduces` is not a GateableDirection — this line should fail.
    /// let _: FrictionGate<zero_operator_state::friction::sealed::Sealed> =
    ///     FrictionGate::new(FrictionLevel::L0);
    /// ```
    fn _compile_fail_doctest_marker() {}
}
