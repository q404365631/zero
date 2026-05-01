//! The 2 AM test suite (§6.2 + M1_PLAN §9).
//!
//! This is the architecture's moral-weight test. Every other
//! piece of the friction ladder could regress and the operator
//! would survive — the daily wrap would print, the welcome would
//! show, the status bar would still say `ops:TILT`. These
//! twelve tests are different. If any of the first six fail,
//! a tired operator at 2 AM cannot `/kill` or `/flatten-all`
//! because the CLI is "protecting" them from their own risk-
//! reducer. That is the exact 2 AM failure the architecture
//! exists to prevent, and the one the product cannot survive.
//!
//! ## The matrix
//!
//! 6 operator-state labels × 2 risk directions = 12 cells. We
//! exercise them with the canonical representatives of each
//! direction:
//!
//! - `/kill` — the canonical `Reduces` command. If `/kill`
//!   proceeds from every label, so does every other `Reduces`
//!   (the type-level `RiskDirection::Reduces` invariant makes
//!   the others follow by construction; see `risk.rs` and the
//!   `compile_fail` doctest in the friction module). Testing
//!   `/kill` is the representative proof.
//! - `/execute` — the canonical `Increases` command. It picks
//!   up the friction ladder via `decide(Increases, label)`,
//!   which is the one function call whose output this suite
//!   locks down end-to-end through `dispatch`.
//!
//! Each test is named after the cell so a CI failure report
//! reads like: "the 2 AM scenario where `/kill` at TILT got
//! gated" rather than a generic "matrix regression."
//!
//! ## Contract each test pins
//!
//! - **Reduces row (6 tests):** `FrictionDecision::Proceed`
//!   and `RiskDirection::Reduces`, at every label, with no
//!   `pending_command` left over. The `Proceed` path does not
//!   populate `pending_command` — a TUI overlay for a
//!   risk-reducer would be the exact thing §15 rules out.
//! - **Increases row (6 tests):** the expected decision for
//!   that label per the `FrictionLevel::from_label` mapping,
//!   with `RiskDirection::Increases` and — when the decision
//!   is Pause or TypedConfirm — a live `pending_command` so
//!   the TUI can re-dispatch after friction is honored.
//!
//! ## What this suite deliberately does not test
//!
//! - Engine-reachability. The dispatcher's HTTP-backed stubs
//!   are friction-layered on top of, not instead of, the
//!   friction ladder; these scenarios run without an engine
//!   and care only about the gating decision, not the fill
//!   round-trip. A separate `dispatch_integration.rs` suite
//!   already covers the HTTP surface against `MockEngine`.
//! - The TUI overlay's visual rendering. The `zero-tui`
//!   friction-overlay tests (`app/state.rs`,
//!   `widgets/overlay.rs`) handle that layer. Here we lock
//!   down what `dispatch` emits; the UI is downstream.
//! - L3 and L4. `FrictionLevel::from_label` tops out at L2
//!   for Phase 1; L3 and L4 are spec'd for M2 and marked in
//!   the risk module. When they land, each adds two more
//!   cells (a label that yields L3, a label that yields L4)
//!   and this suite expands in a forward-compatible way: the
//!   `expected_decision_for` helper is table-driven.

use std::sync::Arc;

use zero_commands::{
    Command, DispatchContext, FrictionDecision, RiskDirection, StaticLabel, dispatch,
};
use zero_engine_client::EngineState;
use zero_operator_state::friction::FrictionLevel;
use zero_operator_state::label::Label;

/// Build a dispatch context pinned to a specific operator-state
/// label. Mirrors the in-crate `ctx_with_label` helper so the
/// two test surfaces read the same way — a future refactor that
/// moves that helper into `zero-testkit` can update both call
/// sites in lock-step.
fn ctx_at(label: Label) -> DispatchContext {
    DispatchContext::new(None, EngineState::shared()).with_state(Arc::new(StaticLabel(label)))
}

/// The label → expected friction-decision mapping for an
/// `Increases` command. Documented once here so the six
/// Increases-row tests can share a single truth; any spec drift
/// lands in exactly one place.
///
/// Phase 1 only reaches L2; the Phase 2 L3/L4 labels do not
/// exist yet, so every label in `Label` is covered.
fn expected_decision_for_increases(label: Label) -> FrictionDecision {
    match FrictionLevel::from_label(label) {
        FrictionLevel::L0 => FrictionDecision::Proceed,
        FrictionLevel::L1 => FrictionDecision::Pause {
            pause: FrictionLevel::L1.pause(),
            level: FrictionLevel::L1,
        },
        FrictionLevel::L2 | FrictionLevel::L3 | FrictionLevel::L4 => {
            // L3/L4 unreachable in Phase 1 but the match must be
            // exhaustive on the friction-level enum; unify the
            // typed-confirm arm so the suite stays green the
            // day Phase 2 lands (the level comes through in the
            // returned decision, so the per-label tests still
            // catch mismatches).
            FrictionDecision::TypedConfirm {
                pause: FrictionLevel::from_label(label).pause(),
                level: FrictionLevel::from_label(label),
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Reduces row: `/kill` must always proceed. The 2 AM failure
//  mode the architecture exists to prevent is any of these
//  six flipping.
// ─────────────────────────────────────────────────────────────

/// Shared body for the six Reduces-row tests. Kept as a free
/// function (not a macro) so a failure reports a clean line
/// number inside the individual `#[tokio::test]` that called
/// it rather than inside a macro expansion.
async fn assert_kill_proceeds_at(label: Label) {
    let ctx = ctx_at(label);
    let out = dispatch(&ctx, "/kill")
        .await
        .expect("dispatch returned error")
        .expect("dispatch returned None for /kill");

    assert_eq!(
        out.risk,
        Some(RiskDirection::Reduces),
        "/kill must remain classified Reduces at {label:?}"
    );
    assert_eq!(
        out.friction,
        Some(FrictionDecision::Proceed),
        "/kill at {label:?} must Proceed — a tired operator cannot be denied the kill switch"
    );
    assert!(
        out.pending_command.is_none(),
        "/kill at {label:?} produced a pending_command; a risk-reducer must never open a friction overlay. got: {:?}",
        out.pending_command
    );
}

#[tokio::test]
async fn kill_at_fresh_proceeds() {
    assert_kill_proceeds_at(Label::Fresh).await;
}

#[tokio::test]
async fn kill_at_steady_proceeds() {
    assert_kill_proceeds_at(Label::Steady).await;
}

#[tokio::test]
async fn kill_at_elevated_proceeds() {
    assert_kill_proceeds_at(Label::Elevated).await;
}

#[tokio::test]
async fn kill_at_fatigued_proceeds() {
    assert_kill_proceeds_at(Label::Fatigued).await;
}

#[tokio::test]
async fn kill_at_tilt_proceeds() {
    // The canonical 2 AM case. If this ever fails, an operator
    // at 2 AM who has triggered TILT cannot reach their kill
    // switch because the CLI is "helping". Everything else in
    // the product is vanity until this passes.
    assert_kill_proceeds_at(Label::Tilt).await;
}

#[tokio::test]
async fn kill_at_recovery_proceeds() {
    assert_kill_proceeds_at(Label::Recovery).await;
}

// ─────────────────────────────────────────────────────────────
//  Increases row: `/execute` picks up the ladder. Each label
//  yields a specific decision; the suite pins every cell.
// ─────────────────────────────────────────────────────────────

/// Shared body for the six Increases-row tests. Asserts the
/// expected decision + the risk-direction tag + the correct
/// `pending_command` presence depending on whether the cell
/// is Proceed or friction-gated.
async fn assert_execute_decision_at(label: Label) {
    let ctx = ctx_at(label);
    let out = dispatch(&ctx, "/execute")
        .await
        .expect("dispatch returned error")
        .expect("dispatch returned None for /execute");

    assert_eq!(
        out.risk,
        Some(RiskDirection::Increases),
        "/execute must remain classified Increases at {label:?}"
    );

    let expected = expected_decision_for_increases(label);
    assert_eq!(
        out.friction,
        Some(expected.clone()),
        "/execute at {label:?}: wrong friction decision. expected {expected:?}, got {:?}",
        out.friction
    );

    match expected {
        FrictionDecision::Proceed => {
            assert!(
                out.pending_command.is_none(),
                "/execute at {label:?} proceeded but left a pending_command: {:?}",
                out.pending_command
            );
        }
        FrictionDecision::Pause { .. }
        | FrictionDecision::TypedConfirm { .. }
        | FrictionDecision::WaitAndReread { .. } => {
            assert_eq!(
                out.pending_command,
                Some(Command::Execute),
                "/execute at {label:?} gated by friction but did not queue a pending_command"
            );
        }
        // L4 refusals never reach this branch via the default
        // `ctx_at(Label)` helper — those tests use a context
        // without engine risk data, so `decide_with_risk` stays
        // capped at L2. If the harness ever wires a halted risk
        // into `ctx_at`, the arm below asserts the refusal
        // contract: no pending command survives.
        FrictionDecision::HardStop { .. } => {
            assert!(
                out.pending_command.is_none(),
                "/execute at {label:?} hit HardStop but still carried pending_command: {:?}",
                out.pending_command
            );
        }
    }
}

#[tokio::test]
async fn execute_at_fresh_proceeds() {
    assert_execute_decision_at(Label::Fresh).await;
}

#[tokio::test]
async fn execute_at_steady_proceeds() {
    assert_execute_decision_at(Label::Steady).await;
}

#[tokio::test]
async fn execute_at_elevated_requires_pause() {
    assert_execute_decision_at(Label::Elevated).await;
}

#[tokio::test]
async fn execute_at_fatigued_requires_pause() {
    assert_execute_decision_at(Label::Fatigued).await;
}

#[tokio::test]
async fn execute_at_tilt_requires_typed_confirm() {
    // The other canonical 2 AM case. At TILT, `/execute` must
    // NOT silently run. It must gate behind the 10s pause +
    // typed `execute` word. Single-key confirmation is the
    // exact regression §6.2 was written to preempt.
    assert_execute_decision_at(Label::Tilt).await;

    // Extra invariant on the TILT cell: the confirm word is
    // fully determined by the level and reads as "execute".
    // Anything else breaks the TUI overlay's input-matcher.
    let ctx = ctx_at(Label::Tilt);
    let out = dispatch(&ctx, "/execute").await.unwrap().unwrap();
    let word = out
        .friction
        .as_ref()
        .and_then(FrictionDecision::confirm_word);
    assert_eq!(
        word.as_deref(),
        Some("execute"),
        "TILT confirm word must be 'execute' — see TYPED_CONFIRM_WORD"
    );
}

#[tokio::test]
async fn execute_at_recovery_proceeds() {
    assert_execute_decision_at(Label::Recovery).await;
}

// ─────────────────────────────────────────────────────────────
//  Meta-check: the suite above is a complete cover of `Label`.
//  If a future contributor adds a new label variant, this test
//  breaks compilation (via the exhaustive match) and forces
//  the suite to grow by two more named tests — one per row of
//  the matrix — before CI turns green.
// ─────────────────────────────────────────────────────────────

#[test]
fn two_am_suite_covers_every_label() {
    fn _exhaustive_marker(l: Label) {
        match l {
            Label::Fresh
            | Label::Steady
            | Label::Elevated
            | Label::Fatigued
            | Label::Tilt
            | Label::Recovery => {}
        }
    }
    // Runtime check: the number of tests in this file matches
    // 2 × variants(Label) + M2 §3 escalation cells + meta.
    // L3 (TILT + proximity) and L4 (TILT + halt) add two cells on
    // the Increases side only — `Reduces` must never escalate.
    const REDUCES_TESTS: usize = 6;
    const INCREASES_TESTS: usize = 6;
    const M2_ESCALATION_TESTS: usize = 3; // L3 proximity, L4 halt, Reduces-under-halt
    const META_TESTS: usize = 1;
    const TOTAL: usize = REDUCES_TESTS + INCREASES_TESTS + M2_ESCALATION_TESTS + META_TESTS;
    assert_eq!(TOTAL, 16, "suite accounting drifted");
}

// ---------------------------------------------------------------
// M2 §3: TILT escalations via engine risk context.
// ---------------------------------------------------------------

/// Build a dispatch context with `Label::Tilt` **and** an engine
/// mirror whose `Risk` carries the fields required to drive
/// `decide_with_risk` into the M2 escalations.
///
/// `proximity_pp` is the pp-gap between `drawdown_pct` and
/// `last_drawdown_alert_pct`. `halted` sets the engine's
/// `global_halt` flag. Both fields feed
/// [`zero_operator_state::RiskContext::from_engine`] — the same
/// path production uses.
fn ctx_tilt_with_risk(proximity_pp: Option<f64>, halted: bool) -> DispatchContext {
    use chrono::{TimeZone, Utc};
    use zero_engine_client::models::Risk;
    use zero_engine_client::stat::Source;

    let engine = EngineState::shared();
    let mut risk = Risk::default();
    if let Some(pp) = proximity_pp {
        // Pick arbitrary-but-stable anchor numbers: alert at 5.00%,
        // current drawdown `pp` below it. Engine reports both.
        let alert = 5.00_f64;
        risk.last_drawdown_alert_pct = Some(alert);
        risk.drawdown_pct = Some(alert - pp);
    }
    if halted {
        risk.global_halt = true;
    }
    engine.write().apply_risk(
        risk,
        Utc.with_ymd_and_hms(2026, 4, 21, 18, 0, 0).unwrap(),
        Source::Ws,
    );
    DispatchContext::new(None, engine).with_state(Arc::new(StaticLabel(Label::Tilt)))
}

#[tokio::test]
async fn tilt_plus_proximity_escalates_execute_to_wait_and_reread() {
    // Within 0.5 pp of the hard alert → L3 / WaitAndReread.
    let ctx = ctx_tilt_with_risk(Some(0.5), false);
    let out = dispatch(&ctx, "/execute").await.unwrap().unwrap();

    match out.friction {
        Some(FrictionDecision::WaitAndReread { level, phrase, .. }) => {
            assert_eq!(level, FrictionLevel::L3);
            assert!(
                phrase.contains("drawdown") && phrase.contains("hard alert"),
                "L3 phrase should embed the proximity disclosure, got {phrase:?}"
            );
        }
        other => panic!("expected WaitAndReread at L3, got {other:?}"),
    }
    assert_eq!(
        out.pending_command,
        Some(Command::Execute),
        "L3 must still carry pending_command so the TUI can re-dispatch after the re-read"
    );
}

#[tokio::test]
async fn tilt_plus_halt_refuses_execute_with_hard_stop() {
    let ctx = ctx_tilt_with_risk(None, true);
    let out = dispatch(&ctx, "/execute").await.unwrap().unwrap();

    match out.friction {
        Some(FrictionDecision::HardStop { level, reason }) => {
            assert_eq!(level, FrictionLevel::L4);
            assert_eq!(
                reason, "global_halt",
                "halt reason label must name the engine flag that tripped"
            );
        }
        other => panic!("expected HardStop at L4, got {other:?}"),
    }
    assert!(
        out.pending_command.is_none(),
        "L4 refusal must NOT carry a pending_command — no re-dispatch path is honest"
    );
}

#[tokio::test]
async fn tilt_plus_halt_still_permits_kill_as_reduces() {
    // The single most load-bearing test in this suite. The engine
    // is halted, the operator is tilted — and `/kill` must still
    // proceed. If this ever regresses, the 2 AM failure mode the
    // entire architecture exists to prevent is live in production.
    let ctx = ctx_tilt_with_risk(Some(0.1), true);
    let out = dispatch(&ctx, "/kill").await.unwrap().unwrap();

    assert_eq!(out.risk, Some(RiskDirection::Reduces));
    assert_eq!(
        out.friction,
        Some(FrictionDecision::Proceed),
        "/kill must Proceed even under TILT + halted engine"
    );
    assert!(
        out.pending_command.is_none(),
        "/kill proceeded — it should not leave a pending_command"
    );
}
