//! **M2 §4 contract — LIVE.** When the engine mirror carries an
//! L3+ operator-state snapshot, an `AppState` constructed over
//! that mirror auto-opens the `Risk` overlay. Pre-M2 §4 this was
//! two tests — one locking the "no overlay yet" M1 behavior, one
//! `#[ignore]`-marked stub documenting the forward contract. The
//! commit landing M2 §4 deletes the M1 lock and un-ignores the
//! M2 assertion, which is this file's current shape.
//!
//! ## What the overlay does
//!
//! - Opens when `FrictionLevel::L3` or `L4` lands on the mirror
//!   (TILT + guardrail proximity, or TILT + halt; see
//!   `zero-operator-state::friction::FrictionLevel` docs).
//! - Also opens, independent of the classifier's verdict, when
//!   the engine reports `Risk.drawdown_pct` within
//!   [`AppState::GUARDRAIL_PROXIMITY_PP`] (0.5 pp) of the
//!   `Risk.last_drawdown_alert_pct` threshold.
//! - Dismisses on any key. The auto-open hook enforces a 60 s
//!   cooldown after dismissal unless (a) the trigger strictly
//!   escalates (L3 → L4) or (b) the engine trips a fresh
//!   guardrail threshold mid-dismiss.
//!
//! This test pins the **open path** specifically. The rate-limit
//! and escalation paths are covered by the `AppState` unit tests
//! in `state.rs`.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use parking_lot::RwLock;
use zero_engine_client::EngineState;
use zero_operator_state::friction::FrictionLevel;
use zero_operator_state::label::Label;
use zero_operator_state::snapshot::Snapshot;
use zero_operator_state::vector::StateVector;
use zero_tui::AppState;
use zero_tui::app::state::{ActiveOverlay, RiskOverlayTrigger};

/// Construct a `Snapshot` carrying an L3 friction level.
///
/// Equivalent to
/// `Snapshot::new_with_risk(Label::Tilt, …, RiskContext {
/// guardrail_proximity_pct: Some(0.5), halted: false })` — both
/// produce the same L3 shape. The hand-assembled form stays
/// because it insulates the overlay assertion from the
/// classifier's `StateVector` default, which is orthogonal to
/// "overlay opens on L3".
fn l3_snapshot() -> Snapshot {
    Snapshot {
        label: Label::Tilt,
        friction: FrictionLevel::L3,
        vector: StateVector::default(),
        as_of: Utc.with_ymd_and_hms(2026, 4, 21, 18, 0, 0).unwrap(),
        version: 42,
    }
}

fn make_engine_with_l3_snapshot() -> Arc<RwLock<EngineState>> {
    let engine = EngineState::shared();
    {
        let mut guard = engine.write();
        guard.apply_operator_state(
            l3_snapshot(),
            Utc.with_ymd_and_hms(2026, 4, 21, 18, 0, 0).unwrap(),
        );
    }
    engine
}

/// An L3 operator snapshot on the mirror at `AppState`
/// construction time opens the Risk overlay with the
/// corresponding `Friction(L3)` trigger. This is the
/// session-attach contract: the operator never sees an empty
/// prompt when the engine already reports they are at L3.
#[test]
fn l3_snapshot_auto_opens_risk_overlay_with_friction_trigger() {
    let engine = make_engine_with_l3_snapshot();
    let state = AppState::new_with_sink(engine, None);

    match state.overlay {
        Some(ActiveOverlay::Risk { trigger, .. }) => {
            assert_eq!(
                trigger,
                RiskOverlayTrigger::Friction(FrictionLevel::L3),
                "L3 snapshot should open with Friction(L3) trigger",
            );
        }
        other => panic!(
            "expected ActiveOverlay::Risk, got {other:?}. M2 §4 \
             contract: L3+ operator-state snapshots must auto-open \
             the risk overlay on AppState construction.",
        ),
    }
}
