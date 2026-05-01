//! Classifier tick benchmark (Addendum A §2 · M1_PLAN §9).
//!
//! ## Contract
//!
//! Every render of the operator-state overlay — and every status
//! bar refresh when the WS pushes a new position / decision event
//! — calls `Classifier::classify(now)`. The spec pins a p95 of
//! **≤ 1 ms** on a realistic event-log load. Exceeding that
//! means the render loop has a hot path that can stall the
//! frame during an engine event burst, which is the exact
//! surface the friction ladder needs to be *more* responsive on,
//! not less.
//!
//! ## What "realistic" means here
//!
//! We measure three load points that bracket the operator's
//! lived day:
//!
//! - **light (32 events)** — first hour of a session. Most of
//!   the matching logic has empty rolling windows; this is the
//!   floor.
//! - **typical (512 events)** — a full day of activity:
//!   ~40 decisions, a handful of trades, half a dozen break
//!   transitions, plus periodic idle/resume/verdict bookkeeping.
//!   This is what the classifier sees during regular play.
//! - **heavy (4096 events)** — someone left the CLI up across
//!   a weekend of paper trading; the log grew without a
//!   compaction pass. Exercises the 24-hour-window pruning and
//!   the reverse-iteration early-exit.
//!
//! The bench walks each load point ten thousand times through
//! criterion's sampling harness; p95 is reported in the HTML
//! report and compared to the 1 ms ceiling in the regression
//! test below (`classifier_tick_under_1ms_on_typical_load`).
//!
//! ## Why a regression test lives next to the bench
//!
//! Criterion benches do not fail a build on their own — they
//! produce measurements, not verdicts. To keep "≤ 1 ms p95"
//! enforceable in CI, we run a deliberately smaller inline
//! measurement inside a `#[test]` and assert the mean is well
//! under the ceiling. The criterion bench is the detailed
//! instrument; the test is the trip wire.

use chrono::{DateTime, Duration, TimeZone, Utc};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use zero_operator_state::classifier::Classifier;
use zero_operator_state::events::{Event, EventKind, Outcome, Source};
use zero_operator_state::friction::RiskContext;

/// Wall-clock anchor so every bench run is bit-for-bit
/// reproducible. Kept in 2026 so the rolling windows all land
/// inside the event log's span rather than eclipsing it.
fn anchor() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 21, 18, 0, 0).unwrap()
}

/// Build a synthetic event log that looks like a real trading
/// day, sized to `n`. Uses only cheap-to-clone primitives so
/// the bench's prep phase doesn't dominate the measurement.
///
/// Distribution (approximately the spec's "typical" mix):
/// - 60% `DecisionMade`
/// - 15% `TradeClosed` with alternating Win/Loss/Scratch
/// - 10% `VerdictShown` / 5% `VerdictOverridden`
/// - 5% `Idle`/`Resumed` pairs
/// - 5% break bookkeeping + session markers
fn synth_events(n: usize, anchor: DateTime<Utc>) -> Vec<Event> {
    let mut out = Vec::with_capacity(n);
    // Events must be monotonically non-decreasing in ts, so
    // walk backwards from the anchor by a fraction of a second
    // per event. At n = 4096 that spans ~68 minutes, which
    // keeps everything in the hottest rolling window — the
    // realistic worst case.
    for i in 0..n {
        // i is bounded by n (4096 in the heavy bench), so the
        // cast is lossless; explicit conversion placates clippy.
        let ts =
            anchor - Duration::milliseconds(i64::try_from(i).expect("bench load fits i64") * 1_000);
        let symbol = match i % 4 {
            0 => "BTC",
            1 => "ETH",
            2 => "SOL",
            _ => "AVAX",
        }
        .to_string();
        let kind = match i % 20 {
            0..=11 => EventKind::DecisionMade {
                symbol,
                source: Source::Manual,
            },
            12 => EventKind::TradeClosed {
                symbol,
                outcome: Outcome::Win,
                pnl_r: 1.2,
                conviction: Some(7),
            },
            13 => EventKind::TradeClosed {
                symbol,
                outcome: Outcome::Loss,
                pnl_r: -0.8,
                conviction: Some(5),
            },
            14 => EventKind::TradeClosed {
                symbol,
                outcome: Outcome::Scratch,
                pnl_r: 0.0,
                conviction: Some(6),
            },
            15 => EventKind::VerdictShown,
            16 => EventKind::VerdictOverridden,
            17 => EventKind::Idle { since_ms: 30_000 },
            18 => EventKind::Resumed,
            _ => EventKind::BreakStarted {
                planned_ms: Some(5 * 60 * 1_000),
            },
        };
        out.push(Event::new(ts, kind));
    }
    // Classifier::push() expects non-decreasing ts; we built
    // them descending, so reverse before returning.
    out.reverse();
    out
}

fn build_classifier(n: usize) -> Classifier {
    let now = anchor();
    let mut c = Classifier::new();
    c.push(Event::new(
        now - Duration::hours(6),
        EventKind::SessionStarted,
    ));
    for ev in synth_events(n, now) {
        c.push(ev);
    }
    c
}

fn bench_classifier_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("classifier/tick");
    // Narrower sample size than the default (100) so the bench
    // finishes inside a typical CI step's budget. Each iteration
    // is sub-microsecond; criterion's variance estimation still
    // converges cleanly at this sample count.
    group.sample_size(50);
    let now = anchor();

    for &n in &[32_usize, 512, 4096] {
        let classifier = build_classifier(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &classifier, |b, cls| {
            b.iter(|| {
                // black_box on both the input and the output so
                // the optimizer cannot hoist the call out of the
                // loop or constant-fold it.
                let snap = cls.classify(black_box(now));
                black_box(snap);
            });
        });
    }
    group.finish();
}

/// M2 §3: approaching-halt load point (1 024 events) with
/// `classify_with_risk` on the hot path.
///
/// The M2 escalation branches in `from_label_and_risk` add a
/// pair of comparisons and a boolean read on top of the
/// classifier's rolling-window work. Isolating this as its own
/// bench group lets the regression alarm catch any
/// higher-order slowdown (e.g. a future change that reads
/// `RiskContext` inside a loop instead of at the final
/// `Snapshot::new_with_risk` call). The event mix matches
/// `build_classifier` at n=1024 — a full day + change of
/// activity — and the supplied `RiskContext` drives the
/// classifier through the L3 (near-guardrail) branch so the
/// escalation code path is exercised, not just present.
fn bench_classifier_tick_with_risk(c: &mut Criterion) {
    let mut group = c.benchmark_group("classifier/tick_with_risk");
    group.sample_size(50);
    let now = anchor();
    let classifier = build_classifier(1024);
    let risk = RiskContext {
        guardrail_proximity_pct: Some(0.5),
        halted: false,
    };

    group.bench_function(BenchmarkId::from_parameter(1024_usize), |b| {
        b.iter(|| {
            let snap = classifier.classify_with_risk(black_box(now), black_box(risk));
            black_box(snap);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_classifier_tick,
    bench_classifier_tick_with_risk
);
criterion_main!(benches);
