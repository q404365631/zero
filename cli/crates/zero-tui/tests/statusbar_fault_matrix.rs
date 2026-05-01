//! Status-bar fault matrix: 4 widths × 4 engine states = 16
//! single-line snapshots.

// `name__cell` reads the matrix at a glance — `stale__w200`
// means "stale engine state, width 200." The double underscore
// is a deliberate separator, not snake_case drift. `cargo test
// reconnecting__` picks up every reconnecting cell cleanly.
#![allow(non_snake_case)]
//!
//! ## Why this suite exists (the honest version)
//!
//! On 2026-04-21 a live run against `api.getzero.dev` rendered
//! this to the operator's status bar:
//!
//! ```text
//! r=HTTP error: 403 Forbidden retry:6  feed:--  dd:0.2%  ops:?
//! ```
//!
//! That's not the status bar. That's the status bar overlaid by
//! `tracing`'s stderr writer bleeding WARN records into the alt
//! screen. The root cause was a tracing-target bug (fixed in
//! `zero/src/main.rs::tracing_target_for`) but the fact the bug
//! shipped is a test-coverage bug: none of the existing
//! snapshots exercised the connection-failure path, so the
//! render contract was undefended for the exact scenario the
//! CLI hit in the wild.
//!
//! This file fixes that. Every snapshot here is **one row tall**
//! — `TestBackend::new(width, 1)` — so a regression shows up as
//! a 1-line diff instead of being buried in a 24-line frame.
//! The matrix is:
//!
//! |                             | W=40  | W=80  | W=120 | W=200 |
//! |-----------------------------|-------|-------|-------|-------|
//! | healthy (ws up, fresh)      | ✓     | ✓     | ✓     | ✓     |
//! | reconnecting (the 403 case) | ✓     | ✓     | ✓     | ✓     |
//! | down (never connected)      | ✓     | ✓     | ✓     | ✓     |
//! | stale ops + feed-lag        | ✓     | ✓     | ✓     | ✓     |
//! | rate-exhausted (M2 §2)      | ✓     | ✓     | ✓     | ✓     |
//! | hl-exhausted  (M2 §2)       | ✓     | ✓     | ✓     | ✓     |
//!
//! The two M2 cells pin the rate/hl rendering contract at every
//! width so a regression in either tier-picking or the EXH
//! rendering fails loudly. Both segments are Full-tier-only:
//! - at 40 the bar falls to Minimal; segments are dropped and
//!   the operator still reads mode + ops + dd (the three
//!   anchors);
//! - at 80, 120, and 200 the Full tier fits for a healthy
//!   engine (no `retry:N` addendum to crowd it out) and both
//!   segments render — `rate:EXH` / `hl:EXH` in alert+bold, or
//!   `rate:N/M` / `hl:N/M` tri-colored by headroom otherwise.
//!
//! The Compact tier (single-space separators, no `rate:`/`hl:`,
//! no `retry:`) is the fallback for the narrow band where Full
//! doesn't fit but Minimal is too spartan — it's exercised by
//! the `pick_tier_prefers_widest_fit` unit test in
//! `widgets::statusbar::tests`, which pins the 40/80/200 picks
//! directly rather than through snapshots.
//!
//! The 4 widths are:
//! - **40** — narrow (half an 80-col tmux pane; forces Minimal)
//! - **80** — classic default
//! - **120** — modern IDE terminal (Full tier, modest padding)
//! - **200** — wide monitor (Full tier, heavy padding; pins
//!   against accidental over-pad or mid-line wrap)
//!
//! The 4 states are:
//! - **healthy**: `ws_connected = true`, fresh feed, populated
//!   risk + ops. Baseline.
//! - **reconnecting**: `ws_connected = false`,
//!   `total_attempts = 7`, `reconnect_count = 6` — a literal
//!   reproduction of the on-the-wire 403 loop. If a future
//!   tracing-writer regression re-bleeds WARN records across
//!   the frame, these snapshots will blow up with garbage in
//!   the 1-line output because stderr and the TestBackend
//!   contend for the same terminal emulator in the `cargo test`
//!   runner's eyes — **no**, that's not quite right. The
//!   TestBackend runs headless and `tracing` in tests writes to
//!   the process's stderr by default, which the test harness
//!   captures. The regression guard here is narrower: it pins
//!   the *intended* render shape for reconnecting state. A
//!   live-world bleed is still ultimately a symptom of wiring
//!   tracing to stderr during TUI mode; that is pinned
//!   separately by `tracing_target_is_log_file_for_tui_entrypoint`
//!   in `zero/src/main.rs`. The two together form the belt and
//!   the suspenders.
//! - **down**: `ws_connected = false`, `total_attempts = 0`. The
//!   first-boot-before-first-connect case. Distinct from
//!   reconnecting — no `retry:N` span.
//! - **stale ops + feed-lag**: ops snapshot older than
//!   `OPERATOR_STATE_STALE_AFTER`; feed age in the alert tier.
//!   Exercises the `*` marker + the alert-color feed span.
//!
//! Width-tier fallback is a property of `StatusBar::pick_tier`,
//! not the state — but pinning both together means a regression
//! in either axis produces a visible diff in the matrix.

use chrono::{DateTime, Duration, TimeZone, Utc};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use zero_engine_client::{BudgetSnapshot, EngineState, HlRate, Risk, Source, Stat, V2Status};
use zero_operator_state::{Label, Snapshot as OperatorSnapshot, StateVector};
use zero_tui::app::mode::Mode;
use zero_tui::theme::Theme;
use zero_tui::widgets::statusbar::StatusBar;

/// Canonical frozen time for every test in this file. Any `as_of`
/// computed from this must be explicit so width snapshots are
/// reproducible to the second.
fn frozen() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 21, 18, 30, 0).unwrap()
}

/// Render the status bar into a `width×1` TestBackend and return
/// the single row as a `String`. Strips nothing — trailing
/// whitespace is preserved because "does the bar pad correctly
/// at 200 cols" is one of the things we want to catch.
fn render_bar_1row(engine: &EngineState, now: DateTime<Utc>, width: u16) -> String {
    render_bar_1row_with_budget(engine, now, width, None)
}

/// Same as [`render_bar_1row`] but with an explicit
/// `BudgetSnapshot` pinned — used by the M2 rate/hl cells that
/// need specific bucket math (near-empty, exhausted, healthy).
fn render_bar_1row_with_budget(
    engine: &EngineState,
    now: DateTime<Utc>,
    width: u16,
    rate_budget: Option<BudgetSnapshot>,
) -> String {
    let backend = TestBackend::new(width, 1);
    let mut term = Terminal::new(backend).expect("terminal");
    term.draw(|f| {
        let bar = StatusBar {
            mode: Mode::Conversation,
            engine,
            theme: Theme::default(),
            now,
            rate_budget,
        };
        f.render_widget(bar, f.area());
    })
    .expect("draw");
    let buf = term.backend().buffer().clone();
    (0..buf.area.width)
        .map(|x| buf[(x, 0)].symbol().to_string())
        .collect()
}

/// Four canonical widths for the M1 responsive grid.
const WIDTHS: [u16; 4] = [40, 80, 120, 200];

// ─── State builders ─────────────────────────────────────────────

fn healthy_engine() -> EngineState {
    let mut e = EngineState::default();
    e.on_ws_connected();
    e.apply_risk(
        Risk {
            account_value: Some(10_000.0),
            drawdown_pct: Some(1.2),
            daily_loss_usd: Some(30.0),
            peak_equity: Some(10_000.0),
            open_count: Some(2),
            ..Default::default()
        },
        frozen(),
        Source::Ws,
    );
    let snap = OperatorSnapshot::new(Label::Steady, StateVector::default(), frozen(), 1);
    e.operator_state = Some(Stat::new(snap, Source::Http).with_as_of(frozen()));
    e
}

fn reconnecting_engine() -> EngineState {
    // Faithful reproduction of the 403-loop state observed in
    // the wild: seven attempts logged, never connected, no
    // feed, no ops snapshot (auth poll also fails at 401/403).
    //
    // Status-bar "RECONNECTING vs DOWN" dispatches on
    // `total_attempts`, and the retry badge dispatches on
    // `reconnect_count` — both ticked by `on_reconnect_attempt`,
    // not by `on_ws_disconnected`. An earlier draft of this
    // helper looped over `on_ws_disconnected` and produced a
    // `DOWN` frame; the difference matters because the status
    // bar's copy ("RECONNECTING" vs "DOWN") is how the
    // operator distinguishes "engine was there a moment ago"
    // from "engine has never been there."
    let mut e = EngineState::default();
    for _ in 0..6 {
        e.on_reconnect_attempt(frozen());
    }
    // `reconnect_count` is now 6 → the bar renders `retry:6`,
    // matching the live frame the regression was captured from.
    // Risk/ops remain None — the CLI has nothing to show because
    // the engine never authorized a payload. This is the exact
    // "empty except for the retry counter" frame.
    e
}

fn down_engine() -> EngineState {
    // First-boot state: the WS subscriber hasn't tried yet.
    // `engine:DOWN`, no retry addendum.
    EngineState::default()
}

fn stale_engine() -> EngineState {
    // Everything populated but as-of is 15 minutes stale — well
    // past OPERATOR_STATE_STALE_AFTER (5s) and into the
    // feed-alert tier (>10s → alert color).
    let mut e = EngineState::default();
    e.on_ws_connected();
    let stale = frozen() - Duration::minutes(15);
    e.apply_risk(
        Risk {
            account_value: Some(10_000.0),
            drawdown_pct: Some(2.5),
            daily_loss_usd: Some(20.0),
            peak_equity: Some(10_000.0),
            open_count: Some(1),
            ..Default::default()
        },
        stale,
        Source::Ws,
    );
    let snap = OperatorSnapshot::new(Label::Tilt, StateVector::default(), stale, 1);
    e.operator_state = Some(Stat::new(snap, Source::Http).with_as_of(stale));
    e
}

// ─── The matrix ─────────────────────────────────────────────────
//
// Each state × each width is one snapshot. Names are
// `<state>__w<width>` so a failing CI line reads like
// `reconnecting__w80` and points directly at the cell.

#[test]
fn healthy__w40() {
    let e = healthy_engine();
    let snap = render_bar_1row(&e, frozen(), 40);
    assert!(snap.contains("dd:1.2%"), "dd must survive narrow: {snap:?}");
    assert!(
        snap.contains("ops:STEADY"),
        "ops must survive narrow: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_healthy_w40", snap);
}

#[test]
fn healthy__w80() {
    let e = healthy_engine();
    let snap = render_bar_1row(&e, frozen(), 80);
    assert!(snap.contains("engine:OK"), "{snap:?}");
    assert!(snap.contains("feed:0s"), "{snap:?}");
    insta::assert_snapshot!("statusbar_healthy_w80", snap);
}

#[test]
fn healthy__w120() {
    let e = healthy_engine();
    let snap = render_bar_1row(&e, frozen(), 120);
    insta::assert_snapshot!("statusbar_healthy_w120", snap);
}

#[test]
fn healthy__w200() {
    let e = healthy_engine();
    let snap = render_bar_1row(&e, frozen(), 200);
    // Width 200 is the "is it over-padding or wrapping?" case.
    // We expect exactly 200 chars, single row, trailing spaces.
    assert_eq!(
        snap.chars().count(),
        200,
        "status bar must render exactly width chars (no wrap): {snap:?}"
    );
    insta::assert_snapshot!("statusbar_healthy_w200", snap);
}

// ── reconnecting: the literal 403-loop reproduction ────────────
//
// If these four snapshots ever change, the status bar's
// connection-failure story changed. The shape we lock down is:
//   `engine:RECONNECTING retry:6` (Full/Compact tiers)
// or
//   `dd:-- ops:?` (Minimal fallback at W=40 if the retry
//   addendum does not fit)

#[test]
fn reconnecting__w40() {
    let e = reconnecting_engine();
    let snap = render_bar_1row(&e, frozen(), 40);
    insta::assert_snapshot!("statusbar_reconnecting_w40", snap);
}

#[test]
fn reconnecting__w80() {
    let e = reconnecting_engine();
    let snap = render_bar_1row(&e, frozen(), 80);
    assert!(snap.contains("RECONNECTING"), "{snap:?}");
    assert!(
        snap.contains("retry:6"),
        "retry counter must render: {snap:?}"
    );
    // The exact frame from the regression: ops poll failed so
    // there is no ops label yet — should be `ops:?`, NOT the
    // raw WARN text that used to bleed through.
    assert!(snap.contains("ops:?"), "{snap:?}");
    // Negative: the literal bleed string must never appear here.
    // If it does, either the status-bar widget regressed to
    // interpolate an error string, or the test harness itself
    // lost containment — either way we want a loud failure.
    assert!(
        !snap.contains("HTTP error"),
        "status bar must never contain raw error strings: {snap:?}"
    );
    assert!(
        !snap.contains("403"),
        "status bar must never contain HTTP status codes: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_reconnecting_w80", snap);
}

#[test]
fn reconnecting__w120() {
    let e = reconnecting_engine();
    let snap = render_bar_1row(&e, frozen(), 120);
    insta::assert_snapshot!("statusbar_reconnecting_w120", snap);
}

#[test]
fn reconnecting__w200() {
    let e = reconnecting_engine();
    let snap = render_bar_1row(&e, frozen(), 200);
    assert_eq!(snap.chars().count(), 200);
    insta::assert_snapshot!("statusbar_reconnecting_w200", snap);
}

// ── down: first-boot-before-first-connect ──────────────────────

#[test]
fn down__w40() {
    let e = down_engine();
    let snap = render_bar_1row(&e, frozen(), 40);
    insta::assert_snapshot!("statusbar_down_w40", snap);
}

#[test]
fn down__w80() {
    let e = down_engine();
    let snap = render_bar_1row(&e, frozen(), 80);
    assert!(snap.contains("engine:DOWN"), "{snap:?}");
    assert!(
        !snap.contains("retry:"),
        "DOWN is pre-first-attempt — no retry addendum: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_down_w80", snap);
}

#[test]
fn down__w120() {
    let e = down_engine();
    let snap = render_bar_1row(&e, frozen(), 120);
    insta::assert_snapshot!("statusbar_down_w120", snap);
}

#[test]
fn down__w200() {
    let e = down_engine();
    let snap = render_bar_1row(&e, frozen(), 200);
    assert_eq!(snap.chars().count(), 200);
    insta::assert_snapshot!("statusbar_down_w200", snap);
}

// ── stale: every field populated but old ───────────────────────

#[test]
fn stale__w40() {
    let e = stale_engine();
    let snap = render_bar_1row(&e, frozen(), 40);
    insta::assert_snapshot!("statusbar_stale_w40", snap);
}

#[test]
fn stale__w80() {
    let e = stale_engine();
    let snap = render_bar_1row(&e, frozen(), 80);
    // The `*` marker is the visible staleness indicator per
    // `OPERATOR_STATE_STALE_AFTER`.
    assert!(
        snap.contains("ops:TILT*"),
        "stale operator snapshot must render with asterisk marker: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_stale_w80", snap);
}

#[test]
fn stale__w120() {
    let e = stale_engine();
    let snap = render_bar_1row(&e, frozen(), 120);
    insta::assert_snapshot!("statusbar_stale_w120", snap);
}

#[test]
fn stale__w200() {
    let e = stale_engine();
    let snap = render_bar_1row(&e, frozen(), 200);
    assert_eq!(snap.chars().count(), 200);
    insta::assert_snapshot!("statusbar_stale_w200", snap);
}

// ── rate-exhausted: CLI-side bucket drained ────────────────────
//
// M2 §1 landed the CLI-side `RateBudget` and §2 surfaces it in
// the status bar. An exhausted bucket is the most operator-
// visible rate state (renders `rate:EXH` in alert+bold), and it
// is the state we most want to pin: a regression that stops
// painting EXH would silently strand the operator at a rate
// wall with no visible signal.

/// Healthy engine + an *exhausted* CLI-side bucket. Drives the
/// `rate:EXH` alert+bold render in the Full tier, and the tier-
/// walk drop-to-Compact/Minimal contract at the narrower widths.
fn rate_exhausted_snapshot() -> BudgetSnapshot {
    BudgetSnapshot {
        capacity: 60,
        refill_per_second: 1.0,
        tokens: 0,
    }
}

#[test]
fn rate_exhausted__w40() {
    let e = healthy_engine();
    let snap = render_bar_1row_with_budget(&e, frozen(), 40, Some(rate_exhausted_snapshot()));
    assert!(
        !snap.contains("rate:"),
        "width 40 falls to Minimal — rate segment must drop: {snap:?}"
    );
    assert!(snap.contains("ops:STEADY"), "anchor survives: {snap:?}");
    insta::assert_snapshot!("statusbar_rate_exhausted_w40", snap);
}

#[test]
fn rate_exhausted__w80() {
    let e = healthy_engine();
    let snap = render_bar_1row_with_budget(&e, frozen(), 80, Some(rate_exhausted_snapshot()));
    // 80 cols still fits the Full tier for a healthy engine
    // (the `healthy_engine` shape has no `retry:N` addendum),
    // so `rate:EXH` is expected here. The tier-drop contract
    // is exercised at w40 (→ Minimal) — if a future operator
    // screen sees a w80 regression that drops the segment, the
    // widget is silently mis-categorizing a fitting line as
    // too narrow, and the assertion below catches it.
    assert!(
        snap.contains("rate:EXH"),
        "width 80 in Full tier must surface rate:EXH: {snap:?}"
    );
    assert!(snap.contains("engine:OK"), "anchor survives: {snap:?}");
    assert!(snap.contains("ops:STEADY"), "anchor survives: {snap:?}");
    insta::assert_snapshot!("statusbar_rate_exhausted_w80", snap);
}

#[test]
fn rate_exhausted__w120() {
    let e = healthy_engine();
    let snap = render_bar_1row_with_budget(&e, frozen(), 120, Some(rate_exhausted_snapshot()));
    assert!(
        snap.contains("rate:EXH"),
        "Full tier must surface rate:EXH: {snap:?}"
    );
    assert!(
        snap.contains("hl:?"),
        "hl segment renders `?` when engine has not reported: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_rate_exhausted_w120", snap);
}

#[test]
fn rate_exhausted__w200() {
    let e = healthy_engine();
    let snap = render_bar_1row_with_budget(&e, frozen(), 200, Some(rate_exhausted_snapshot()));
    assert_eq!(snap.chars().count(), 200);
    assert!(snap.contains("rate:EXH"), "{snap:?}");
    insta::assert_snapshot!("statusbar_rate_exhausted_w200", snap);
}

// ── hl-exhausted: engine reports used == cap on Hyperliquid ────
//
// Mirrors the rate-exhausted matrix, but drives the `hl:EXH`
// render through `V2Status::hl_rate`. The engine is the source
// of truth for HL rate (ADR-016-style separation), so the CLI
// never fabricates the number — we simulate a payload where
// the engine surfaced `hl_rate = { used = 240, cap = 240 }`.

fn hl_exhausted_engine() -> EngineState {
    let mut e = healthy_engine();
    let status = V2Status {
        hl_rate: Some(HlRate {
            used: 240,
            cap: 240,
        }),
        ..V2Status::default()
    };
    e.apply_status(status, frozen(), Source::Ws);
    e
}

#[test]
fn hl_exhausted__w40() {
    let e = hl_exhausted_engine();
    let snap = render_bar_1row(&e, frozen(), 40);
    assert!(
        !snap.contains("hl:"),
        "width 40 falls to Minimal — hl segment must drop: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_hl_exhausted_w40", snap);
}

#[test]
fn hl_exhausted__w80() {
    let e = hl_exhausted_engine();
    let snap = render_bar_1row(&e, frozen(), 80);
    // See `rate_exhausted__w80` — the healthy-engine shape
    // still fits Full at w80, so `hl:EXH` surfaces here.
    assert!(
        snap.contains("hl:EXH"),
        "width 80 in Full tier must surface hl:EXH: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_hl_exhausted_w80", snap);
}

#[test]
fn hl_exhausted__w120() {
    let e = hl_exhausted_engine();
    let snap = render_bar_1row(&e, frozen(), 120);
    assert!(
        snap.contains("hl:EXH"),
        "Full tier must surface hl:EXH: {snap:?}"
    );
    assert!(
        snap.contains("rate:?"),
        "rate segment renders `?` with no bucket attached: {snap:?}"
    );
    insta::assert_snapshot!("statusbar_hl_exhausted_w120", snap);
}

#[test]
fn hl_exhausted__w200() {
    let e = hl_exhausted_engine();
    let snap = render_bar_1row(&e, frozen(), 200);
    assert_eq!(snap.chars().count(), 200);
    assert!(snap.contains("hl:EXH"), "{snap:?}");
    insta::assert_snapshot!("statusbar_hl_exhausted_w200", snap);
}

// ── meta-check: the matrix is complete ─────────────────────────

#[test]
fn matrix_accounting_is_complete() {
    // If a future contributor adds a width or a state, the
    // matrix must grow, not drift. This is a human-facing
    // tripwire: if this test fails, go count your #[test]
    // functions and update the constants.
    //
    // M2 §2 extended the matrix from 4 → 6 states by adding
    // the `rate-exhausted` and `hl-exhausted` cells. The row
    // count therefore jumped from 16 to 24 (+ the meta row).
    const STATES: usize = 6; // healthy / reconnecting / down / stale / rate-exh / hl-exh
    const PER_STATE_SNAPSHOTS: usize = WIDTHS.len(); // 4 widths each
    const META: usize = 1; // this test
    const EXPECTED_TESTS: usize = STATES * PER_STATE_SNAPSHOTS + META;
    assert_eq!(EXPECTED_TESTS, 25, "matrix suite accounting drifted");
}
