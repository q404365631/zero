//! Pure event → state classifier.
//!
//! The classifier is deterministic: given the same event log and the
//! same `now`, it produces identical output. That is the property
//! that makes snapshot tests cheap, replay meaningful, and bugs
//! reproducible.
//!
//! ## What the classifier does
//!
//! 1. Ingest [`Event`]s in timestamp order.
//! 2. Compute the [`StateVector`] fields from sliding-window reductions.
//! 3. Apply the label rules from **Addendum A §2.3 and §10.2** to
//!    produce a [`Label`].
//! 4. Hand both to a [`Snapshot`].
//!
//! ## What the classifier deliberately does not do
//!
//! - It does not talk to the engine.
//! - It does not read or write files.
//! - It does not emit friction side effects. `zero-commands` applies
//!   the friction gate using [`crate::FrictionGate`], not this type.
//! - It does not decide what to render. `zero-tui` reads the
//!   [`Snapshot`].
//!
//! Separation keeps the classifier exhaustively testable and
//! snapshot-safe.

use chrono::{DateTime, Duration, Utc};

use crate::events::{Event, EventKind, Outcome};
use crate::friction::RiskContext;
use crate::label::Label;
use crate::snapshot::Snapshot;
use crate::vector::{Deviation, LossReaction, ReEntry, Session, SleepProxy, StateVector, Velocity};

/// Incremental classifier. Feed events in, snapshot out.
#[derive(Debug, Default, Clone)]
pub struct Classifier {
    events: Vec<Event>,
    version: u64,
    on_break_since: Option<DateTime<Utc>>,
    last_break_ended_at: Option<DateTime<Utc>>,
    session_started_at: Option<DateTime<Utc>>,
    last_loss_at: Option<DateTime<Utc>>,
    last_loss_symbol: Option<String>,
}

impl Classifier {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event. Events must be monotonically non-decreasing
    /// in `ts`; the classifier does not re-sort.
    pub fn push(&mut self, event: Event) {
        match &event.kind {
            EventKind::SessionStarted => self.session_started_at = Some(event.ts),
            EventKind::BreakStarted { .. } => self.on_break_since = Some(event.ts),
            EventKind::BreakEnded => {
                self.on_break_since = None;
                self.last_break_ended_at = Some(event.ts);
            }
            EventKind::TradeClosed {
                outcome: Outcome::Loss,
                symbol,
                ..
            } => {
                self.last_loss_at = Some(event.ts);
                self.last_loss_symbol = Some(symbol.clone());
            }
            // `Conviction` (and every other non-vector-affecting
            // kind) falls through here. It is a calibration
            // ingredient, not a state-vector ingredient: it
            // describes how the operator *felt* about a past
            // trade, not their current tempo, deviation, or
            // session state. The classifier keeps the event in
            // the log (so deterministic replay still
            // reconstructs the same history — see
            // `classify_is_deterministic_over_the_same_log`)
            // but does not reach into any rolling window for
            // it. Downstream calibration consumers join on
            // `trade_id`; the classifier itself never needs to.
            _ => {}
        }
        self.events.push(event);
        self.version = self.version.wrapping_add(1);
    }

    /// Compute a fresh snapshot as of `now`.
    ///
    /// Uses [`Snapshot::new`], which caps friction at L2. Callers
    /// with engine context (the dispatcher, the TUI's operator-
    /// state refresh path) should use
    /// [`Self::classify_with_risk`] to reach L3/L4.
    #[must_use]
    pub fn classify(&self, now: DateTime<Utc>) -> Snapshot {
        let vector = self.compute_vector(now);
        let label = label_for(&vector);
        Snapshot::new(label, vector, now, self.version)
    }

    /// Compute a fresh snapshot as of `now`, folding
    /// engine-reported risk context into the friction level.
    ///
    /// The event-pure part of the classifier is unchanged — `risk`
    /// participates only at the final `Snapshot` construction via
    /// [`Snapshot::new_with_risk`]. Passing
    /// [`RiskContext::default`] is equivalent to calling
    /// [`Self::classify`].
    ///
    /// This entrypoint is what the TUI calls on every tick once
    /// the engine mirror has a live `Risk` payload; the
    /// additional O(1) work is a pair of integer comparisons and
    /// a branch, so the p95 budget from
    /// `classifier_tick_under_budget_on_typical_load` is
    /// unaffected.
    #[must_use]
    pub fn classify_with_risk(&self, now: DateTime<Utc>, risk: RiskContext) -> Snapshot {
        let vector = self.compute_vector(now);
        let label = label_for(&vector);
        Snapshot::new_with_risk(label, vector, now, self.version, risk)
    }

    #[allow(clippy::too_many_lines)] // windowed reductions deliberately inlined for clarity
    fn compute_vector(&self, now: DateTime<Utc>) -> StateVector {
        let h1 = now - Duration::hours(1);
        let h4 = now - Duration::hours(4);
        let day = now - Duration::hours(24);

        // Decisions are any DecisionMade event. Frozen while on break.
        let mut decisions = (0u32, 0u32, 0u32);
        let mut verdicts_shown_10 = 0u32;
        let mut verdicts_shown_50 = 0u32;
        let mut overrides_10 = 0u32;
        let mut overrides_50 = 0u32;
        let mut fastest_loss_reaction_ms: u64 = u64::MAX;
        let mut loss_reactions: Vec<u64> = Vec::new();
        let mut re_entry = ReEntry::default();

        // Walk the event log newest-first so windowed counts are
        // cheap. We stop once all windows are past h24.
        for ev in self.events.iter().rev() {
            let in_1h = ev.ts >= h1;
            let in_4h = ev.ts >= h4;
            let in_day = ev.ts >= day;
            if !in_day {
                // Older than 24h — cannot affect any rolling window
                // except lifetime baseline which we compute elsewhere.
                continue;
            }

            match &ev.kind {
                EventKind::DecisionMade { source: _, symbol } => {
                    if in_1h {
                        decisions.0 += 1;
                    }
                    if in_4h {
                        decisions.1 += 1;
                    }
                    if in_day {
                        decisions.2 += 1;
                    }
                    // Re-entry: if a prior close on this symbol
                    // occurred within the window.
                    if let Some(last_loss_sym) = &self.last_loss_symbol
                        && let Some(last_loss_at) = self.last_loss_at
                        && last_loss_sym == symbol
                    {
                        let gap = ev.ts.signed_duration_since(last_loss_at);
                        if gap <= Duration::minutes(15) && gap >= Duration::zero() {
                            re_entry.within_15m += 1;
                        }
                        if gap <= Duration::minutes(30) && gap >= Duration::zero() {
                            re_entry.within_30m += 1;
                        }
                        if gap <= Duration::hours(2) && gap >= Duration::zero() {
                            re_entry.within_2h += 1;
                        }
                    }
                }
                EventKind::VerdictShown => {
                    if verdicts_shown_50 < 50 {
                        verdicts_shown_50 += 1;
                        if verdicts_shown_10 < 10 {
                            verdicts_shown_10 += 1;
                        }
                    }
                }
                EventKind::VerdictOverridden => {
                    if verdicts_shown_50 < 50 {
                        overrides_50 += 1;
                        if verdicts_shown_10 < 10 {
                            overrides_10 += 1;
                        }
                    }
                }
                EventKind::TradeClosed {
                    outcome: Outcome::Loss,
                    ..
                } => {
                    // Loss-reaction = time to next DecisionMade on
                    // the same symbol. Computed forward-in-time; we
                    // rely on the newest-first walk to capture the
                    // nearest follow-on.
                    let forward_decision =
                        self.events
                            .iter()
                            .filter(|e| e.ts > ev.ts)
                            .find_map(|e| match &e.kind {
                                EventKind::DecisionMade { .. } => Some(e.ts),
                                _ => None,
                            });
                    if let Some(next) = forward_decision {
                        let raw = next.signed_duration_since(ev.ts).num_milliseconds().max(0);
                        let ms = u64::try_from(raw).unwrap_or(u64::MAX);
                        loss_reactions.push(ms);
                        if ms < fastest_loss_reaction_ms {
                            fastest_loss_reaction_ms = ms;
                        }
                    }
                }
                _ => {}
            }
        }

        loss_reactions.sort_unstable();
        let median_ms = if loss_reactions.is_empty() {
            0
        } else {
            loss_reactions[loss_reactions.len() / 2]
        };

        let session_ms = self.session_started_at.map_or(0, |start| {
            let d = now.signed_duration_since(start).num_milliseconds().max(0);
            u64::try_from(d).unwrap_or(u64::MAX)
        });

        let since_last_break_ms = self.last_break_ended_at.map_or(session_ms, |end| {
            let d = now.signed_duration_since(end).num_milliseconds().max(0);
            u64::try_from(d).unwrap_or(u64::MAX)
        });

        StateVector {
            velocity: Velocity {
                last_1h: decisions.0,
                last_4h: decisions.1,
                last_24h: decisions.2,
                baseline_1h: None,
            },
            deviation: Deviation {
                overrides_last_10: overrides_10,
                verdicts_last_10: verdicts_shown_10,
                overrides_last_50: overrides_50,
                verdicts_last_50: verdicts_shown_50,
            },
            session: Session {
                active_duration_ms: session_ms,
                longest_focus_ms: session_ms,
                since_last_break_ms,
            },
            loss_reaction: LossReaction {
                median_last_10_ms: median_ms,
                fastest_session_ms: if fastest_loss_reaction_ms == u64::MAX {
                    0
                } else {
                    fastest_loss_reaction_ms
                },
                baseline_ms: None,
            },
            re_entry,
            sleep_proxy: SleepProxy {
                hours_since_rest_ended: None,
            },
            on_break: self.on_break_since.is_some(),
        }
    }

    /// Total events consumed.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

/// Apply the label rules from Addendum A §2.3 and §10.2.
#[allow(clippy::cast_precision_loss)] // session durations fit comfortably in f64 mantissa
fn label_for(v: &StateVector) -> Label {
    let session_hours = v.session.active_duration_ms as f64 / 3_600_000.0;
    let velocity_ratio = v.velocity.ratio_to_baseline();
    let deviation = v.deviation.rate_last_10();

    // TILT (§10.2): any 2 of 4 composite triggers.
    let tilt_triggers = [
        velocity_ratio.is_some_and(|r| r > 2.0),
        v.loss_reaction.fastest_session_ms > 0
            && v.loss_reaction.fastest_session_ms < 5 * 60 * 1000,
        deviation > 0.4,
        v.re_entry.within_15m > 0,
    ];
    if tilt_triggers.iter().filter(|t| **t).count() >= 2 {
        return Label::Tilt;
    }

    // FATIGUED: session >6h continuous OR sleep proxy >18h.
    if session_hours >= 6.0 || v.sleep_proxy.hours_since_rest_ended.is_some_and(|h| h > 18) {
        return Label::Fatigued;
    }

    // ELEVATED: velocity 1.5x baseline OR deviation 20-40% OR session 4h+.
    if velocity_ratio.is_some_and(|r| r >= 1.5) || deviation >= 0.2 || session_hours >= 4.0 {
        return Label::Elevated;
    }

    // FRESH: <5 decisions in last hour AND session <2h.
    if v.velocity.last_1h < 5 && session_hours < 2.0 {
        return Label::Fresh;
    }

    Label::Steady
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Source;
    use chrono::TimeZone;

    fn ts(min: i64) -> DateTime<Utc> {
        chrono::TimeZone::timestamp_opt(&Utc, 1_700_000_000 + min * 60, 0).unwrap()
    }

    #[test]
    fn empty_classifier_is_fresh() {
        let c = Classifier::new();
        let snap = c.classify(ts(0));
        assert_eq!(snap.label, Label::Fresh);
        assert_eq!(snap.vector.velocity.last_1h, 0);
    }

    #[test]
    fn session_start_advances_duration() {
        let mut c = Classifier::new();
        c.push(Event::new(ts(0), EventKind::SessionStarted));
        let snap = c.classify(ts(150)); // 2h30m
        assert!(snap.vector.session.active_duration_ms >= 2 * 3_600_000);
    }

    #[test]
    fn elevated_on_long_session() {
        let mut c = Classifier::new();
        c.push(Event::new(ts(0), EventKind::SessionStarted));
        // 4h1m later — should be elevated solely by duration.
        let snap = c.classify(ts(4 * 60 + 1));
        assert_eq!(snap.label, Label::Elevated);
    }

    #[test]
    fn fatigued_on_six_hour_session() {
        let mut c = Classifier::new();
        c.push(Event::new(ts(0), EventKind::SessionStarted));
        let snap = c.classify(ts(6 * 60 + 5));
        assert_eq!(snap.label, Label::Fatigued);
    }

    #[test]
    fn tilt_on_reentry_plus_high_deviation() {
        let mut c = Classifier::new();
        c.push(Event::new(ts(0), EventKind::SessionStarted));
        // A loss, then rapid re-entry within 15 min — trigger 1.
        c.push(Event::new(
            ts(10),
            EventKind::TradeClosed {
                symbol: "BTC".into(),
                outcome: Outcome::Loss,
                pnl_r: -1.0,
                conviction: None,
            },
        ));
        c.push(Event::new(
            ts(15),
            EventKind::DecisionMade {
                symbol: "BTC".into(),
                source: Source::Override,
            },
        ));
        // Plan-verdict overrides to push deviation > 40% — trigger 3.
        for m in 16..26 {
            c.push(Event::new(ts(m), EventKind::VerdictShown));
        }
        for m in 16..22 {
            c.push(Event::new(ts(m), EventKind::VerdictOverridden));
        }
        let snap = c.classify(ts(30));
        assert_eq!(snap.label, Label::Tilt, "vector: {:?}", snap.vector);
        assert_eq!(snap.friction, crate::FrictionLevel::L2);
    }

    #[test]
    fn classify_with_risk_escalates_tilt_on_halt() {
        use crate::events::Source;
        use crate::friction::{FrictionLevel, RiskContext};

        // Build a TILT log the same way `tilt_on_reentry_plus_high_deviation`
        // does, then reclassify with a halt flag set.
        let mut c = Classifier::new();
        c.push(Event::new(ts(0), EventKind::SessionStarted));
        c.push(Event::new(
            ts(10),
            EventKind::TradeClosed {
                symbol: "BTC".into(),
                outcome: Outcome::Loss,
                pnl_r: -1.0,
                conviction: None,
            },
        ));
        c.push(Event::new(
            ts(15),
            EventKind::DecisionMade {
                symbol: "BTC".into(),
                source: Source::Override,
            },
        ));
        for m in 16..26 {
            c.push(Event::new(ts(m), EventKind::VerdictShown));
        }
        for m in 16..22 {
            c.push(Event::new(ts(m), EventKind::VerdictOverridden));
        }

        let snap_plain = c.classify(ts(30));
        assert_eq!(snap_plain.friction, FrictionLevel::L2);

        let snap_halt = c.classify_with_risk(
            ts(30),
            RiskContext {
                guardrail_proximity_pct: None,
                halted: true,
            },
        );
        assert_eq!(snap_halt.label, Label::Tilt);
        assert_eq!(snap_halt.friction, FrictionLevel::L4);

        let snap_proximity = c.classify_with_risk(
            ts(30),
            RiskContext {
                guardrail_proximity_pct: Some(0.5),
                halted: false,
            },
        );
        assert_eq!(snap_proximity.friction, FrictionLevel::L3);
    }

    #[test]
    fn version_monotonic() {
        let mut c = Classifier::new();
        let v0 = c.classify(ts(0)).version;
        c.push(Event::new(ts(1), EventKind::VerdictShown));
        let v1 = c.classify(ts(1)).version;
        assert!(v1 > v0);
    }

    /// Determinism contract (M1_PLAN §7a line 117): feeding the
    /// same event log to a fresh classifier at the same `now`
    /// must produce a byte-for-byte identical `Snapshot`. This
    /// is the property that makes replay meaningful and the
    /// per-keypress status-bar re-render safe — a classifier
    /// that drifted between ticks would make `ops:<LABEL>`
    /// flicker while the operator typed. We verify by running
    /// two disjoint classifiers over a shuffled-then-sorted
    /// event mix and comparing the whole snapshot, not just the
    /// label. `vector` + `version` + `friction` + `label` all
    /// have to match — differing on any one would indicate a
    /// hidden piece of mutable state.
    #[test]
    fn classify_is_deterministic_over_the_same_log() {
        use crate::events::Source;

        let now = ts(500);
        let mix: Vec<Event> = vec![
            Event::new(ts(0), EventKind::SessionStarted),
            Event::new(
                ts(60),
                EventKind::DecisionMade {
                    symbol: "BTC".into(),
                    source: Source::Plan,
                },
            ),
            Event::new(ts(90), EventKind::VerdictShown),
            Event::new(
                ts(120),
                EventKind::TradeClosed {
                    symbol: "BTC".into(),
                    outcome: Outcome::Loss,
                    pnl_r: -0.75,
                    conviction: None,
                },
            ),
            Event::new(
                ts(121),
                EventKind::Conviction {
                    trade_id: "t-001".into(),
                    rating: 7,
                },
            ),
            Event::new(
                ts(130),
                EventKind::DecisionMade {
                    symbol: "ETH".into(),
                    source: Source::Override,
                },
            ),
            Event::new(ts(131), EventKind::VerdictOverridden),
            Event::new(
                ts(200),
                EventKind::BreakStarted {
                    planned_ms: Some(600_000),
                },
            ),
            Event::new(ts(210), EventKind::BreakEnded),
        ];

        let mut a = Classifier::new();
        for ev in &mix {
            a.push(ev.clone());
        }

        let mut b = Classifier::new();
        for ev in &mix {
            b.push(ev.clone());
        }

        let snap_a = a.classify(now);
        let snap_b = b.classify(now);

        assert_eq!(snap_a.label, snap_b.label);
        assert_eq!(snap_a.friction, snap_b.friction);
        assert_eq!(snap_a.version, snap_b.version);
        assert_eq!(snap_a.vector, snap_b.vector);
        // Reclassifying at the same `now` on the same classifier
        // must also be idempotent — tests would otherwise green
        // on "same input ⇒ same output" while the second call
        // on the same classifier drifted (e.g. via interior
        // mutability in a future refactor).
        let snap_a2 = a.classify(now);
        assert_eq!(snap_a.vector, snap_a2.vector);
        assert_eq!(snap_a.version, snap_a2.version);
    }

    /// CI tripwire for Addendum A §2 / M1_PLAN §9:
    /// "operator-state classifier tick ≤ 1 ms p95."
    ///
    /// Criterion (see `benches/classifier_tick.rs`) is the
    /// detailed instrument; this test is the regression alarm
    /// that actually fails the build if the classifier gets
    /// slow. We measure only the typical-load case (512 events,
    /// a full day of activity) with an inline wall-clock loop
    /// — criterion benches do not fail CI on their own.
    ///
    /// The ceiling is 500 µs (half the spec budget), not 1 ms:
    /// the spec says p95, we measure mean. Mean should be well
    /// under p95 in a stable distribution, and cutting the
    /// budget in half here gives us headroom for debug vs
    /// release variance, loaded runners, and the fact that a
    /// cold-branch first iteration tends to be an outlier.
    /// If this trips, a real regression is very likely —
    /// tune the heuristic, not the budget.
    ///
    /// Runs under `--release` only. In debug, the classifier's
    /// rolling-window arithmetic is 10–50× slower; asserting
    /// a 500 µs budget there would be spurious failure bait,
    /// so we skip. Anyone wanting to confirm debug behavior
    /// can run `cargo bench` instead, which always compiles
    /// the bench target in release mode by construction.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "release-only perf tripwire")]
    #[allow(clippy::too_many_lines)]
    fn classifier_tick_under_budget_on_typical_load() {
        use std::time::Instant;

        use crate::friction::RiskContext;

        // 500 µs: half the 1 ms spec budget (see fn docs).
        const BUDGET_US: u128 = 500;
        const ITERATIONS: u32 = 1_000;

        let now = Utc.with_ymd_and_hms(2026, 4, 21, 18, 0, 0).unwrap();
        // Mirror the bench's "typical" load point. Kept inline
        // rather than shared because dev-dep imports from a
        // `benches/` target don't flow back into `src/`.
        let mut c = Classifier::new();
        c.push(Event::new(
            now - chrono::Duration::hours(6),
            EventKind::SessionStarted,
        ));
        for i in 0..512u32 {
            let ts_i = now - chrono::Duration::milliseconds(i64::from(i) * 1_000);
            let symbol = ["BTC", "ETH", "SOL", "AVAX"][(i as usize) % 4].to_string();
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
                    planned_ms: Some(300_000),
                },
            };
            c.push(Event::new(ts_i, kind));
        }

        // Warm up once to let the allocator steady. Discarding
        // the first call is standard perf-test hygiene and
        // prevents a cold-cache first sample from dominating
        // the mean.
        let _ = c.classify(now);

        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let snap = c.classify(now);
            // Use the result so the optimizer cannot elide the
            // call. `snap.version` is the cheapest non-trivial
            // read on the snapshot.
            std::hint::black_box(snap.version);
        }
        let elapsed = start.elapsed();
        let per_call = elapsed / ITERATIONS;

        assert!(
            per_call.as_micros() < BUDGET_US,
            "classifier tick mean {per_call:?} exceeded {BUDGET_US}µs budget \
             (spec p95 ≤ 1 ms). Run `cargo bench -p zero-operator-state` \
             for the full distribution."
        );

        // M2 §3 approaching-halt load point (1 024 events)
        // exercises `classify_with_risk` so the L3 escalation
        // branch is covered by the same budget. The
        // `RiskContext` supplied puts drawdown within 0.5 pp of
        // the alert, which routes through the L3 branch — the
        // hottest new code path added by M2 §3.
        let mut c_halt = Classifier::new();
        c_halt.push(Event::new(
            now - chrono::Duration::hours(6),
            EventKind::SessionStarted,
        ));
        for i in 0..1_024u32 {
            let ts_i = now - chrono::Duration::milliseconds(i64::from(i) * 500);
            let symbol = ["BTC", "ETH", "SOL", "AVAX"][(i as usize) % 4].to_string();
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
                    planned_ms: Some(300_000),
                },
            };
            c_halt.push(Event::new(ts_i, kind));
        }
        let risk = RiskContext {
            guardrail_proximity_pct: Some(0.5),
            halted: false,
        };
        let _ = c_halt.classify_with_risk(now, risk);

        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let snap = c_halt.classify_with_risk(now, risk);
            std::hint::black_box(snap.version);
        }
        let per_call = start.elapsed() / ITERATIONS;
        assert!(
            per_call.as_micros() < BUDGET_US,
            "classify_with_risk (1024-event approaching-halt mix) mean {per_call:?} \
             exceeded {BUDGET_US}µs budget. The M2 §3 escalation branches must not \
             degrade the tick budget — see M2_PLAN §3."
        );
    }
}
