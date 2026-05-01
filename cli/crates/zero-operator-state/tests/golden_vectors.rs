//! Cross-language determinism contract — Rust side.
//!
//! The engine-side Python port
//! (`engine/zero/operator_state/`) must produce byte-for-byte
//! identical snapshots for the canonical event log. If either side
//! drifts, this test fails on the Rust side and
//! `engine/zero/tests/test_operator_state_golden.py` fails on the
//! Python side — both languages pin the same JSON fixture.
//!
//! ## Fixture location
//!
//! `tests/fixtures/golden_snapshot.json` — the committed
//! expected-output file. Regenerating after a deliberate classifier
//! behaviour change:
//!
//! ```ignore
//! cargo test -p zero-operator-state --test golden_vectors \
//!     golden_snapshot_matches_fixture -- --ignored \
//!     ZERO_REGENERATE_GOLDEN=1
//! ```
//!
//! The ignore-guarded regen path exists so the happy path stays
//! read-only in CI; tripping the env var is the explicit decision
//! that says "yes, the behaviour change was intended, and I am
//! updating both sides of the contract."
//!
//! ## What the fixture covers
//!
//! A 9-event log exercising every branch the classifier reads:
//! session start, decisions, verdicts, a losing trade close, a
//! conviction rating (log-only event, deterministic-replay canary),
//! an override, a break window. Classified at a deterministic `now`
//! two hours after session start. If you add a new non-flag
//! `EventKind` variant, extend the log before regenerating.

use chrono::{TimeZone, Utc};
use zero_operator_state::{Classifier, Event, EventKind, Outcome, Source as EventSource};

fn canonical_log() -> (chrono::DateTime<Utc>, Vec<Event>) {
    // All timestamps sit on a fixed anchor so the fixture stays
    // reproducible. This is the 2026-04-21 18:00:00 UTC used by the
    // classifier perf tripwire too — one anchor, one truth.
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 18, 0, 0).unwrap();
    let mk = |min: i64, kind: EventKind| -> Event {
        Event::new(base + chrono::Duration::minutes(min), kind)
    };

    let log = vec![
        mk(0, EventKind::SessionStarted),
        mk(
            60,
            EventKind::DecisionMade {
                symbol: "BTC".into(),
                source: EventSource::Plan,
            },
        ),
        mk(90, EventKind::VerdictShown),
        mk(
            120,
            EventKind::TradeClosed {
                symbol: "BTC".into(),
                outcome: Outcome::Loss,
                pnl_r: -0.75,
                conviction: None,
            },
        ),
        mk(
            121,
            EventKind::Conviction {
                trade_id: "t-001".into(),
                rating: 7,
            },
        ),
        mk(
            130,
            EventKind::DecisionMade {
                symbol: "ETH".into(),
                source: EventSource::Override,
            },
        ),
        mk(131, EventKind::VerdictOverridden),
        mk(
            200,
            EventKind::BreakStarted {
                planned_ms: Some(600_000),
            },
        ),
        mk(210, EventKind::BreakEnded),
    ];

    // `now` is classifier-evaluation time: two hours past session
    // start, so session-duration + all 1h/4h/24h windows are exercised.
    let now = base + chrono::Duration::hours(2);
    (now, log)
}

#[test]
fn golden_snapshot_matches_fixture() {
    let (now, log) = canonical_log();
    let mut classifier = Classifier::new();
    for ev in &log {
        classifier.push(ev.clone());
    }
    let snap = classifier.classify(now);

    let emitted = serde_json::to_string_pretty(&snap).expect("serde encode");
    // Normalize trailing newline so a one-byte editor mismatch does
    // not cause spurious diffs; we still assert byte equality on the
    // content itself.
    let emitted = format!("{}\n", emitted.trim_end());

    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/golden_snapshot.json"
    );

    // Regeneration escape hatch. Documented in the module header.
    // Off-by-default so CI cannot silently rewrite the contract;
    // authors with a deliberate change set the env var and commit
    // both files (this fixture + the Python test expectation) in
    // the same diff.
    if std::env::var("ZERO_REGENERATE_GOLDEN").is_ok() {
        std::fs::write(fixture_path, &emitted).expect("write fixture");
        panic!(
            "golden fixture regenerated at {fixture_path}. \
             Commit the change + update the Python side's expected \
             JSON in `engine/zero/tests/test_operator_state_golden.py`."
        );
    }

    let expected = std::fs::read_to_string(fixture_path)
        .unwrap_or_else(|e| panic!("cannot read golden fixture {fixture_path}: {e}"));

    assert_eq!(
        emitted.trim_end(),
        expected.trim_end(),
        "golden snapshot drifted. Diff the two; if the change is \
         intended, regenerate with \
         `ZERO_REGENERATE_GOLDEN=1 cargo test -p zero-operator-state \
         --test golden_vectors golden_snapshot_matches_fixture` and \
         mirror the change in the Python side."
    );
}
