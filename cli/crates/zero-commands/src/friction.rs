//! Friction decisions — the runtime half of the risk-asymmetry
//! invariant (ADR-013 / ADR-014, Addendum A §3 and §6.3).
//!
//! The compile-time half lives in `risk.rs`: a `FrictionGate` can
//! only ever be parameterised over `Increases`. A risk-reducing
//! or neutral command is *structurally unable* to be friction-
//! wrapped. That's the guarantee.
//!
//! This module adds the runtime half: given the operator's
//! current behavioural label and a command's [`RiskDirection`],
//! produce a [`FrictionDecision`] — Proceed, Pause, or
//! TypedConfirm — that a caller (the TUI, the non-interactive
//! entrypoint, a headless scheduler) can honor.
//!
//! The decision is purposely stateless. The caller is responsible
//! for the timer (Pause) and the input check (TypedConfirm); we
//! only tell it *what* the friction shape is.
//!
//! # Invariants
//!
//! - `RiskDirection::Reduces` always resolves to [`FrictionDecision::Proceed`].
//!   This is tested. A regression here is the "operator can't
//!   `/kill` at 2 AM" failure mode the architecture exists to
//!   prevent.
//! - `RiskDirection::Neutral` always resolves to Proceed. Reads,
//!   mode switches, log clears never pause.
//! - `RiskDirection::Increases` picks Pause or TypedConfirm
//!   according to [`FrictionLevel::from_label`] (Phase 1:
//!   L0/L1/L2 only; L3/L4 are Phase 2).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use zero_operator_state::friction::{FrictionLevel, RiskContext};
use zero_operator_state::label::Label;

use crate::risk::RiskDirection;

/// The confirmation word the operator must type at TILT (L2) to
/// execute a risk-increasing command. Constant so tests, TUI, and
/// automation key on the same value.
///
/// Per Addendum A §6.2: "At TILT the single-key `e` is replaced
/// by the typed string `execute`."
pub const TYPED_CONFIRM_WORD: &str = "execute";

/// The typed re-read phrase the operator must enter verbatim at
/// L3 (TILT + guardrail proximity) when no engine-reported
/// drawdown number is available to tailor a richer sentence.
///
/// The intention of §3.2 is "re-read the exact disclosure phrase
/// about current guardrail proximity". When drawdown/alert
/// numbers are known, `decide_with_risk` formats a longer phrase
/// that interpolates them; when the engine has not yet reported
/// a pair (fresh connect, older engine), we fall back to this
/// fixed string so the operator still has something concrete to
/// type and the 30 s pause still applies. The phrase is
/// deliberately longer than `execute` — the re-read is the
/// friction; a short word would be an easier bypass than the
/// typed proximity sentence.
pub const FALLBACK_REREAD_PHRASE: &str = "i acknowledge i am approaching a hard guardrail";

/// How the caller must honor friction for a single risk-increasing
/// command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum FrictionDecision {
    /// No friction — run the command immediately.
    Proceed,
    /// The operator must observe a visible countdown, then the
    /// command runs. `pause` is the required duration (3s at L1).
    ///
    /// The caller owns the timer and is expected to render the
    /// countdown so the operator sees the pause happening — it is
    /// *not* a hidden delay.
    Pause {
        #[serde(with = "duration_seconds")]
        pause: Duration,
        level: FrictionLevel,
    },
    /// The operator must type [`TYPED_CONFIRM_WORD`] verbatim and
    /// the `pause` must elapse before the command runs. This is
    /// the TILT friction — ten-second pause + typed-word (§6.2).
    ///
    /// The confirm word itself is not serialised — it is fully
    /// determined by the level and always reads as
    /// [`TYPED_CONFIRM_WORD`]. Callers read it via
    /// [`FrictionDecision::confirm_word`].
    TypedConfirm {
        #[serde(with = "duration_seconds")]
        pause: Duration,
        level: FrictionLevel,
    },
    /// M2 §3: L3 friction. The operator must wait out a longer
    /// pause (30 s by default — see
    /// [`FrictionLevel::pause`]) **and** type back the
    /// proximity-disclosure `phrase` verbatim before the command
    /// re-dispatches.
    ///
    /// Unlike [`Self::TypedConfirm`], the phrase here is dynamic
    /// — it embeds the current drawdown / alert numbers so the
    /// operator *reads what is happening right now* rather than
    /// rote-typing `execute`. Serialised as-is so JSON consumers
    /// can log the exact phrase shown to the operator.
    WaitAndReread {
        #[serde(with = "duration_seconds")]
        pause: Duration,
        level: FrictionLevel,
        phrase: String,
    },
    /// M2 §3: L4 friction — refusal. The command is dropped and
    /// no amount of waiting or typing can run it: the engine is
    /// halted and the dead-man switch is load-bearing.
    ///
    /// Only `Reduces` commands continue to flow (they take the
    /// un-gated path entirely — see [`decide_with_risk`]). The
    /// `reason` carries the halt-flag label the engine reported
    /// so the TUI can surface "engine halted: global_halt" rather
    /// than a bare refusal.
    HardStop {
        level: FrictionLevel,
        reason: String,
    },
}

impl FrictionDecision {
    /// The friction level this decision corresponds to. `Proceed`
    /// maps to L0 — it is a useful value to surface on logs and
    /// JSON so tooling can filter.
    #[must_use]
    pub const fn level(&self) -> FrictionLevel {
        match self {
            Self::Proceed => FrictionLevel::L0,
            Self::Pause { level, .. }
            | Self::TypedConfirm { level, .. }
            | Self::WaitAndReread { level, .. }
            | Self::HardStop { level, .. } => *level,
        }
    }

    /// The required pause. `Proceed` and `HardStop` are zero —
    /// `HardStop` because no pause redeems a refusal.
    #[must_use]
    pub const fn pause(&self) -> Duration {
        match self {
            Self::Proceed | Self::HardStop { .. } => Duration::ZERO,
            Self::Pause { pause, .. }
            | Self::TypedConfirm { pause, .. }
            | Self::WaitAndReread { pause, .. } => *pause,
        }
    }

    /// Whether this decision requires a typed confirmation.
    /// True for L2 (`TypedConfirm`) and L3 (`WaitAndReread`); false
    /// for L4 (`HardStop` — refusal cannot be typed past).
    #[must_use]
    pub const fn requires_typed_confirm(&self) -> bool {
        matches!(self, Self::TypedConfirm { .. } | Self::WaitAndReread { .. })
    }

    /// The string the operator must type verbatim to clear this
    /// decision's friction. `None` when no typing is required
    /// (`Proceed`, `Pause`, `HardStop`).
    ///
    /// Returns a `Cow` because L2's word is a static
    /// (`TYPED_CONFIRM_WORD`) while L3's phrase is owned by the
    /// decision itself and varies with engine state.
    #[must_use]
    pub fn confirm_word(&self) -> Option<std::borrow::Cow<'_, str>> {
        match self {
            Self::TypedConfirm { .. } => Some(std::borrow::Cow::Borrowed(TYPED_CONFIRM_WORD)),
            Self::WaitAndReread { phrase, .. } => Some(std::borrow::Cow::Borrowed(phrase.as_str())),
            _ => None,
        }
    }

    /// True for L4 refusals only. The dispatcher consults this to
    /// decide whether a command is allowed to be carried as a
    /// `pending_command` (it is not — L4 drops the command
    /// entirely, leaving only `Reduces` commands to flow).
    #[must_use]
    pub const fn is_refusal(&self) -> bool {
        matches!(self, Self::HardStop { .. })
    }

    /// The halt reason the engine reported, for `HardStop` only.
    /// Lets callers render "engine halted: global_halt" rather
    /// than a bare refusal.
    #[must_use]
    pub fn refusal_reason(&self) -> Option<&str> {
        match self {
            Self::HardStop { reason, .. } => Some(reason.as_str()),
            _ => None,
        }
    }
}

/// Compute the friction decision for a command's risk direction
/// given the operator's current behavioural label.
///
/// Honoring the invariant is this function's entire job:
/// `Reduces` and `Neutral` always return `Proceed`. Only
/// `Increases` reads the label.
///
/// This form caps at L2 — it does not see engine risk context.
/// Use [`decide_with_risk`] from the dispatcher (which does) to
/// reach L3 (`WaitAndReread`) / L4 (`HardStop`).
///
/// The function is `const` so unit tests and compile-time asserts
/// can call it freely.
#[must_use]
pub const fn decide(direction: RiskDirection, label: Label) -> FrictionDecision {
    match direction {
        // Risk-reducing actions are never gated. Ever. This is the
        // line you don't cross. See `reduces_never_gated` below.
        RiskDirection::Reduces | RiskDirection::Neutral => FrictionDecision::Proceed,
        RiskDirection::Increases => {
            let level = FrictionLevel::from_label(label);
            decision_for_level_const(level)
        }
    }
}

/// Compute the friction decision including the M2 §3 L3/L4
/// escalations, given engine risk context.
///
/// - `Reduces` / `Neutral` → `Proceed` unconditionally (the
///   load-bearing invariant, checked in `reduces_never_gated`).
/// - `Increases` + non-TILT → same as [`decide`].
/// - `Increases` + TILT + `risk.halted` → [`FrictionDecision::HardStop`].
/// - `Increases` + TILT + near guardrail → [`FrictionDecision::WaitAndReread`].
/// - `Increases` + TILT with no risk signal → L2 typed-confirm,
///   same as [`decide`]; no surprise escalation.
///
/// The L4 `reason` is computed from `halt_label`, which the
/// dispatcher derives by walking the engine-side halt flags in
/// priority order (`stop_failure_halt` > `global_halt` > `halted`).
/// Keeping it a `&str` parameter (rather than re-walking the
/// `Risk` struct here) lets this crate stay off the
/// `zero-engine-client` dependency.
#[must_use]
pub fn decide_with_risk(
    direction: RiskDirection,
    label: Label,
    risk: RiskContext,
    halt_reason: Option<&str>,
    reread_phrase: Option<String>,
) -> FrictionDecision {
    match direction {
        RiskDirection::Reduces | RiskDirection::Neutral => FrictionDecision::Proceed,
        RiskDirection::Increases => {
            let level = FrictionLevel::from_label_and_risk(label, risk);
            decision_for_level(level, halt_reason, reread_phrase)
        }
    }
}

const fn decision_for_level_const(level: FrictionLevel) -> FrictionDecision {
    match level {
        FrictionLevel::L0 => FrictionDecision::Proceed,
        FrictionLevel::L1 => FrictionDecision::Pause {
            pause: level.pause(),
            level,
        },
        // The const form is only reached by `decide`, which caps
        // at L2; L3/L4 arms are defensive and would map to the
        // same TypedConfirm shape the pre-M2 code emitted. They
        // are not reachable via `decide` — only via
        // `decide_with_risk`, which takes the owned-string path.
        FrictionLevel::L2 | FrictionLevel::L3 | FrictionLevel::L4 => {
            FrictionDecision::TypedConfirm {
                pause: level.pause(),
                level,
            }
        }
    }
}

fn decision_for_level(
    level: FrictionLevel,
    halt_reason: Option<&str>,
    reread_phrase: Option<String>,
) -> FrictionDecision {
    match level {
        FrictionLevel::L0 => FrictionDecision::Proceed,
        FrictionLevel::L1 => FrictionDecision::Pause {
            pause: level.pause(),
            level,
        },
        FrictionLevel::L2 => FrictionDecision::TypedConfirm {
            pause: level.pause(),
            level,
        },
        FrictionLevel::L3 => FrictionDecision::WaitAndReread {
            pause: level.pause(),
            level,
            // If the dispatcher could not synthesise a dynamic
            // disclosure phrase (missing drawdown numbers, older
            // engine), fall back to the fixed phrase. Either way
            // the operator types something concrete; we never
            // reach this branch with an empty string.
            phrase: reread_phrase.unwrap_or_else(|| FALLBACK_REREAD_PHRASE.to_string()),
        },
        FrictionLevel::L4 => FrictionDecision::HardStop {
            level,
            reason: halt_reason.map_or_else(|| "engine halted".to_string(), ToOwned::to_owned),
        },
    }
}

mod duration_seconds {
    //! Human-facing JSON surface: serialise `Duration` as whole
    //! seconds. The CLI never emits sub-second friction pauses
    //! and operators read the JSON; fractional seconds are noise.

    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_u64(d.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(de)?;
        Ok(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------
    // The load-bearing invariant: Reduces and Neutral never gate.
    // -------------------------------------------------------------

    #[test]
    fn reduces_never_gated_regardless_of_label() {
        for label in [
            Label::Fresh,
            Label::Steady,
            Label::Elevated,
            Label::Fatigued,
            Label::Tilt,
            Label::Recovery,
        ] {
            assert_eq!(
                decide(RiskDirection::Reduces, label),
                FrictionDecision::Proceed,
                "Reduces must never gate — label={label:?}"
            );
        }
    }

    #[test]
    fn neutral_never_gated_regardless_of_label() {
        for label in [
            Label::Fresh,
            Label::Steady,
            Label::Elevated,
            Label::Fatigued,
            Label::Tilt,
            Label::Recovery,
        ] {
            assert_eq!(
                decide(RiskDirection::Neutral, label),
                FrictionDecision::Proceed,
                "Neutral must never gate — label={label:?}"
            );
        }
    }

    // -------------------------------------------------------------
    // Increases picks the right level per label.
    // -------------------------------------------------------------

    #[test]
    fn increases_fresh_steady_recovery_proceed() {
        for label in [Label::Fresh, Label::Steady, Label::Recovery] {
            assert_eq!(
                decide(RiskDirection::Increases, label),
                FrictionDecision::Proceed
            );
        }
    }

    #[test]
    fn increases_elevated_requires_three_second_pause() {
        let d = decide(RiskDirection::Increases, Label::Elevated);
        assert_eq!(d.level(), FrictionLevel::L1);
        assert_eq!(d.pause(), Duration::from_secs(3));
        assert!(!d.requires_typed_confirm());
    }

    #[test]
    fn increases_fatigued_requires_three_second_pause() {
        let d = decide(RiskDirection::Increases, Label::Fatigued);
        assert_eq!(d.level(), FrictionLevel::L1);
        assert_eq!(d.pause(), Duration::from_secs(3));
    }

    #[test]
    fn increases_tilt_requires_typed_confirm() {
        let d = decide(RiskDirection::Increases, Label::Tilt);
        assert_eq!(d.level(), FrictionLevel::L2);
        assert_eq!(d.pause(), Duration::from_secs(10));
        assert!(d.requires_typed_confirm());
        assert_eq!(d.confirm_word().as_deref(), Some("execute"));
    }

    // -------------------------------------------------------------
    // M2 §3: decide_with_risk escalates to L3 / L4.
    // -------------------------------------------------------------

    #[test]
    fn decide_with_risk_reduces_always_proceeds_even_when_halted() {
        // The load-bearing invariant: even if the engine is
        // halted and the operator is tilted, a `Reduces` command
        // (e.g. `/kill`) must pass through unchanged. This is the
        // 2 AM failure mode we refuse to enable.
        let ctx = RiskContext {
            guardrail_proximity_pct: Some(0.1),
            halted: true,
        };
        let d = decide_with_risk(
            RiskDirection::Reduces,
            Label::Tilt,
            ctx,
            Some("global_halt"),
            None,
        );
        assert_eq!(d, FrictionDecision::Proceed);
    }

    #[test]
    fn decide_with_risk_neutral_always_proceeds_even_when_halted() {
        let ctx = RiskContext {
            guardrail_proximity_pct: None,
            halted: true,
        };
        let d = decide_with_risk(
            RiskDirection::Neutral,
            Label::Tilt,
            ctx,
            Some("global_halt"),
            None,
        );
        assert_eq!(d, FrictionDecision::Proceed);
    }

    #[test]
    fn decide_with_risk_tilt_plus_proximity_emits_wait_and_reread() {
        let ctx = RiskContext {
            guardrail_proximity_pct: Some(0.5),
            halted: false,
        };
        let d = decide_with_risk(
            RiskDirection::Increases,
            Label::Tilt,
            ctx,
            None,
            Some("drawdown 4.2% — within 0.5pp of 4.7% hard alert".into()),
        );
        assert_eq!(d.level(), FrictionLevel::L3);
        assert_eq!(d.pause(), Duration::from_secs(30));
        assert!(d.requires_typed_confirm());
        assert_eq!(
            d.confirm_word().as_deref(),
            Some("drawdown 4.2% — within 0.5pp of 4.7% hard alert"),
            "L3 phrase must be the dynamic disclosure, not `execute`"
        );
        assert!(!d.is_refusal());
    }

    #[test]
    fn decide_with_risk_l3_falls_back_to_fixed_phrase_when_none_supplied() {
        let ctx = RiskContext {
            guardrail_proximity_pct: Some(0.5),
            halted: false,
        };
        let d = decide_with_risk(RiskDirection::Increases, Label::Tilt, ctx, None, None);
        assert_eq!(d.level(), FrictionLevel::L3);
        assert_eq!(d.confirm_word().as_deref(), Some(FALLBACK_REREAD_PHRASE));
    }

    #[test]
    fn decide_with_risk_tilt_plus_halt_emits_hard_stop() {
        let ctx = RiskContext {
            guardrail_proximity_pct: None,
            halted: true,
        };
        let d = decide_with_risk(
            RiskDirection::Increases,
            Label::Tilt,
            ctx,
            Some("global_halt"),
            None,
        );
        assert_eq!(d.level(), FrictionLevel::L4);
        assert_eq!(d.pause(), Duration::ZERO);
        assert!(!d.requires_typed_confirm());
        assert_eq!(d.confirm_word(), None);
        assert!(d.is_refusal());
        assert_eq!(d.refusal_reason(), Some("global_halt"));
    }

    #[test]
    fn decide_with_risk_hard_stop_without_reason_renders_fallback() {
        let ctx = RiskContext {
            guardrail_proximity_pct: None,
            halted: true,
        };
        let d = decide_with_risk(RiskDirection::Increases, Label::Tilt, ctx, None, None);
        assert_eq!(d.refusal_reason(), Some("engine halted"));
    }

    #[test]
    fn decide_with_risk_no_escalation_signal_matches_decide() {
        // Without guardrail proximity or halt, `decide_with_risk`
        // must return the same shape as `decide`. This is the
        // "no surprise escalation" guarantee that lets
        // non-engine callers keep using `decide`.
        for label in [
            Label::Fresh,
            Label::Steady,
            Label::Elevated,
            Label::Fatigued,
            Label::Tilt,
            Label::Recovery,
        ] {
            for dir in [
                RiskDirection::Reduces,
                RiskDirection::Neutral,
                RiskDirection::Increases,
            ] {
                let plain = decide(dir, label);
                let enriched = decide_with_risk(dir, label, RiskContext::default(), None, None);
                assert_eq!(
                    plain, enriched,
                    "decide/decide_with_risk must agree when risk context is default \
                     (dir={dir:?}, label={label:?})"
                );
            }
        }
    }

    // -------------------------------------------------------------
    // Serialisation round-trip — JSON consumers depend on it.
    // -------------------------------------------------------------

    #[test]
    fn decision_roundtrips_through_json() {
        let d = decide(RiskDirection::Increases, Label::Tilt);
        let s = serde_json::to_string(&d).expect("to-json");
        assert!(s.contains("\"typed_confirm\""));
        let back: FrictionDecision = serde_json::from_str(&s).expect("from-json");
        assert_eq!(d, back);
        assert_eq!(back.confirm_word().as_deref(), Some("execute"));
    }

    #[test]
    fn l3_l4_decisions_roundtrip_through_json() {
        let ctx_l3 = RiskContext {
            guardrail_proximity_pct: Some(0.5),
            halted: false,
        };
        let l3 = decide_with_risk(
            RiskDirection::Increases,
            Label::Tilt,
            ctx_l3,
            None,
            Some("dd 4.2% within 0.5pp of 4.7%".into()),
        );
        let s = serde_json::to_string(&l3).unwrap();
        assert!(s.contains("\"wait_and_reread\""));
        assert!(s.contains("dd 4.2%"));
        let back: FrictionDecision = serde_json::from_str(&s).unwrap();
        assert_eq!(l3, back);

        let ctx_l4 = RiskContext {
            guardrail_proximity_pct: None,
            halted: true,
        };
        let l4 = decide_with_risk(
            RiskDirection::Increases,
            Label::Tilt,
            ctx_l4,
            Some("stop_failure_halt"),
            None,
        );
        let s = serde_json::to_string(&l4).unwrap();
        assert!(s.contains("\"hard_stop\""));
        assert!(s.contains("stop_failure_halt"));
        let back: FrictionDecision = serde_json::from_str(&s).unwrap();
        assert_eq!(l4, back);
    }
}
