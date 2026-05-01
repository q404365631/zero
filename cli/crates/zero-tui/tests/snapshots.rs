//! Snapshot tests for the TUI render at a fixed 80x24 grid.
//!
//! Uses ratatui's `TestBackend` so rendering is fully deterministic
//! in CI. `Buffer::to_string` flattens the grid to a plain-text
//! snapshot that `insta` diffs cleanly; color/style changes are not
//! captured here — those belong in a future pixel-level suite.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use parking_lot::RwLock;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use zero_engine_client::{EngineState, Positions, Risk, Source, Stat};
use zero_operator_state::{Label, Snapshot as OperatorSnapshot, StateVector};
use zero_tui::app::log::{EntryKind, LogEntry};
use zero_tui::app::render::render_at;
use zero_tui::{AppState, Mode};

fn frozen() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 21, 18, 30, 0).unwrap()
}

fn base_state() -> AppState {
    let engine = EngineState::shared();
    let mut s = AppState::new(engine);
    s.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    s.log
        .push(LogEntry::new(EntryKind::System, "zero — deterministic test harness").at(frozen()));
    s
}

fn grid_to_text(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn conversation_mode_empty_engine() {
    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    let state = base_state();

    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("conversation_empty_engine", snap);
}

#[test]
fn conversation_mode_with_prompt_text() {
    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    let mut state = base_state();
    for c in "/help".chars() {
        state.prompt.insert(c);
    }

    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("conversation_with_prompt", snap);
}

#[test]
fn positions_mode_with_one_position() {
    let engine: Arc<RwLock<EngineState>> = EngineState::shared();
    {
        let mut e = engine.write();
        let mut pos = Positions::default();
        pos.items.push(zero_engine_client::Position {
            symbol: "BTC".into(),
            side: "long".into(),
            size: 0.42,
            entry: 64_120.5,
            mark: Some(64_480.0),
            unrealized_pnl: Some(151.13),
            unrealized_r: Some(0.82),
            ..Default::default()
        });
        e.apply_positions(pos, frozen(), Source::Ws);
    }
    let mut state = AppState::new(engine);
    state.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    state.mode = Mode::Positions;

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("positions_one_position", snap);
}

#[test]
fn heat_mode_with_risk_ok() {
    let engine: Arc<RwLock<EngineState>> = EngineState::shared();
    {
        let mut e = engine.write();
        let risk = Risk {
            account_value: Some(10_034.12),
            drawdown_pct: Some(0.8),
            daily_loss_usd: Some(20.0),
            peak_equity: Some(10_000.0),
            open_count: Some(1),
            ..Default::default()
        };
        e.apply_risk(risk, frozen(), Source::Ws);
    }
    let mut state = AppState::new(engine);
    state.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    state.mode = Mode::Heat;

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("heat_with_risk_ok", snap);
}

#[test]
fn conversation_mode_with_tilt_label() {
    // Operator-state mirror populated directly — the status bar
    // must render `ops:TILT` so the segment is proven end-to-end
    // through `render_at`, not just the widget's unit tests.
    let engine: Arc<RwLock<EngineState>> = EngineState::shared();
    {
        let mut e = engine.write();
        let snap = OperatorSnapshot::new(Label::Tilt, StateVector::default(), frozen(), 1);
        e.operator_state = Some(Stat::new(snap, Source::Http).with_as_of(frozen()));
    }
    let mut state = AppState::new(engine);
    state.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    state
        .log
        .push(LogEntry::new(EntryKind::System, "deterministic test").at(frozen()));

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("conversation_with_tilt_label", snap);
}

#[test]
fn state_overlay_over_conversation() {
    let engine: Arc<RwLock<EngineState>> = EngineState::shared();
    {
        let mut e = engine.write();
        let snap = OperatorSnapshot::new(Label::Elevated, StateVector::default(), frozen(), 1);
        e.operator_state = Some(Stat::new(snap, Source::Http).with_as_of(frozen()));
    }
    let mut state = AppState::new(engine);
    state.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    state
        .log
        .push(LogEntry::new(EntryKind::System, "deterministic test").at(frozen()));
    state.overlay = Some(zero_tui::ActiveOverlay::State);

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("state_overlay_elevated", snap);
}

#[test]
fn decisions_mode_is_placeholder() {
    let mut state = base_state();
    state.mode = Mode::Decisions;

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("decisions_placeholder", snap);
}

// ─── Width-responsive snapshots ─────────────────────────────────
//
// Per spec §6 we must render at four canonical widths: 60 / 80 /
// 120 / 200. The 80-col case is implicitly covered by every test
// above; the rest live here. We seed a "rich" engine state (open
// position, populated risk, populated operator label, connected
// WS) so each width's snapshot is genuinely distinguishable —
// otherwise narrow widths just look like padding.

fn rich_state(mode: Mode) -> AppState {
    let engine: Arc<RwLock<EngineState>> = EngineState::shared();
    {
        let mut e = engine.write();
        let mut pos = Positions::default();
        pos.items.push(zero_engine_client::Position {
            symbol: "BTC".into(),
            side: "long".into(),
            size: 0.42,
            entry: 64_120.5,
            mark: Some(64_480.0),
            unrealized_pnl: Some(151.13),
            unrealized_r: Some(0.82),
            ..Default::default()
        });
        e.apply_positions(pos, frozen(), Source::Ws);
        e.apply_risk(
            Risk {
                account_value: Some(10_034.12),
                drawdown_pct: Some(2.5),
                daily_loss_usd: Some(20.0),
                peak_equity: Some(10_000.0),
                open_count: Some(1),
                ..Default::default()
            },
            frozen(),
            Source::Ws,
        );
        let snap = OperatorSnapshot::new(Label::Elevated, StateVector::default(), frozen(), 1);
        e.operator_state = Some(Stat::new(snap, Source::Http).with_as_of(frozen()));
        e.on_ws_connected();
    }
    let mut state = AppState::new(engine);
    state.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    state
        .log
        .push(LogEntry::new(EntryKind::System, "deterministic test").at(frozen()));
    state.mode = mode;
    state
}

#[test]
fn responsive_60_cols_drops_diagnostics_keeps_risk() {
    // 60 columns is the spec's narrow tier. We expect the status
    // bar to fall back to Compact (drop retry, single-space seps)
    // or Minimal depending on label widths. Either way `ops:` and
    // `dd:` must survive.
    let state = rich_state(Mode::Conversation);
    let backend = TestBackend::new(60, 18);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    assert!(snap.contains("ops:ELEVATED"), "{snap}");
    assert!(snap.contains("dd:2.5%"), "{snap}");
    insta::assert_snapshot!("responsive_60_cols", snap);
}

#[test]
fn responsive_120_cols_renders_full_status_bar() {
    let state = rich_state(Mode::Conversation);
    let backend = TestBackend::new(120, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    for needle in [
        " [CONV]",
        "engine:OK",
        "feed:0s",
        // M2 §2 added `rate:` and `hl:` between `feed:` and the
        // anchored risk+ops cluster. Rich state has no bucket
        // attached and no engine-reported hl_rate, so both
        // render as `?` per the honest-rendering contract.
        "rate:?",
        "hl:?",
        "dd:2.5%",
        "ops:ELEVATED",
    ] {
        assert!(snap.contains(needle), "missing {needle}: {snap}");
    }
    insta::assert_snapshot!("responsive_120_cols", snap);
}

#[test]
fn responsive_200_cols_renders_full_status_bar_with_padding() {
    // 200 cols is the wide-monitor tier — same content as 120,
    // just more whitespace. The snapshot pins the layout against
    // accidental wrap or padding regressions.
    let state = rich_state(Mode::Conversation);
    let backend = TestBackend::new(200, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    for needle in [" [CONV]", "engine:OK", "dd:2.5%", "ops:ELEVATED"] {
        assert!(snap.contains(needle), "missing {needle}: {snap}");
    }
    insta::assert_snapshot!("responsive_200_cols", snap);
}

#[test]
fn slash_picker_appears_above_prompt_when_filter_matches() {
    // Typing `/st` should surface the picker with /status + /state
    // highlighted, stacked directly above the prompt row.
    let mut state = base_state();
    for c in "/st".chars() {
        state.prompt.insert(c);
    }
    state.refresh_picker();
    assert!(state.picker.is_some(), "picker must open for /st");

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    // The chevron marks the selected row; /status is first-ranked.
    assert!(snap.contains("› /status"), "picker missing: {snap}");
    // Prompt row below the picker still renders.
    assert!(snap.contains("> /st"), "prompt row missing: {snap}");
    insta::assert_snapshot!("slash_picker_st_filter", snap);
}

#[test]
fn multiline_prompt_renders_continuation_cue() {
    // Shift+Enter on the prompt buffer should give us a visible
    // second row with the continuation cue (`. `), not a submit.
    let mut state = base_state();
    for c in "line one".chars() {
        state.prompt.insert(c);
    }
    state.prompt.insert_newline();
    for c in "line two".chars() {
        state.prompt.insert(c);
    }

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    assert!(snap.contains("> line one"), "first row missing: {snap}");
    assert!(snap.contains(". line two"), "continuation missing: {snap}");
    insta::assert_snapshot!("multiline_prompt_two_rows", snap);
}

#[test]
fn scrolled_conversation_shows_up_arrow_cue() {
    // Fill the log past the visible rows and scroll up; the pane
    // should mark the scroll with the ↑ glyph in the top-right.
    let mut state = base_state();
    state.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    for i in 0..40 {
        state
            .log
            .push(LogEntry::new(EntryKind::System, format!("entry {i:02}")).at(frozen()));
    }
    state.scroll_log_up(15);

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    assert!(snap.contains('↑'), "scrolled-up indicator missing: {snap}");
    insta::assert_snapshot!("conversation_scrolled_up", snap);
}

// ─── Widget-level snapshots ─────────────────────────────────────
//
// Each of the three new widgets (position row, verdict block,
// calibration bar) is rendered into its own minimal TestBackend
// so a change in layout algebra pings a crisp, small diff. These
// complement `positions_one_position` above, which exercises the
// full pane pipeline; the widget-level snapshots are where we
// will notice a column-width regression first.

fn widget_grid(width: u16, height: u16, draw: impl FnOnce(&mut ratatui::Frame<'_>)) -> String {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(draw).unwrap();
    grid_to_text(&term)
}

#[test]
fn widget_position_row_long_with_all_fields() {
    use zero_tui::widgets::position_row::PositionRow;
    let p = zero_engine_client::Position {
        symbol: "BTC".into(),
        side: "long".into(),
        size: 0.42,
        entry: 64_120.50,
        mark: Some(64_480.0),
        unrealized_pnl: Some(151.13),
        unrealized_r: Some(0.82),
        stop: Some(63_500.0),
        target: Some(66_000.0),
        ..Default::default()
    };
    let snap = widget_grid(120, 1, |f| {
        f.render_widget(
            PositionRow {
                position: &p,
                theme: zero_tui::theme::Theme::default(),
            },
            f.area(),
        );
    });
    insta::assert_snapshot!("widget_position_row_long", snap);
}

#[test]
fn widget_verdict_block_pass_with_gates() {
    use zero_engine_client::models::EvaluationLayer;
    use zero_tui::widgets::verdict::VerdictBlock;
    let layer = |name: &str, passed: bool| EvaluationLayer {
        layer: name.into(),
        passed,
        value: serde_json::Value::Null,
        detail: String::new(),
    };
    let e = zero_engine_client::Evaluation {
        coin: Some("BTC".into()),
        direction: Some("LONG".into()),
        conviction: Some(0.72),
        regime: Some("trending".into()),
        consensus: Some(8),
        layers: vec![
            layer("layer_0", true),
            layer("layer_1", true),
            layer("layer_2", true),
        ],
        ..Default::default()
    };
    let snap = widget_grid(60, 6, |f| {
        f.render_widget(
            VerdictBlock {
                evaluation: &e,
                theme: zero_tui::theme::Theme::default(),
            },
            f.area(),
        );
    });
    insta::assert_snapshot!("widget_verdict_pass", snap);
}

#[test]
fn widget_verdict_block_empty_is_honest() {
    use zero_tui::widgets::verdict::VerdictBlock;
    let e = zero_engine_client::Evaluation::default();
    let snap = widget_grid(60, 3, |f| {
        f.render_widget(
            VerdictBlock {
                evaluation: &e,
                theme: zero_tui::theme::Theme::default(),
            },
            f.area(),
        );
    });
    insta::assert_snapshot!("widget_verdict_empty", snap);
}

#[test]
fn widget_calibration_above_threshold() {
    use zero_tui::widgets::calibration::{CalibrationBar, CalibrationSample};
    let sample = CalibrationSample {
        predicted: 0.72,
        observed: 0.68,
        n_samples: 134,
    };
    let snap = widget_grid(60, 1, |f| {
        f.render_widget(
            CalibrationBar {
                sample: Some(sample),
                theme: zero_tui::theme::Theme::default(),
            },
            f.area(),
        );
    });
    insta::assert_snapshot!("widget_calibration_ok", snap);
}

#[test]
fn verdict_overlay_over_conversation() {
    use zero_engine_client::models::EvaluationLayer;
    let layer = |name: &str, passed: bool| EvaluationLayer {
        layer: name.into(),
        passed,
        value: serde_json::Value::Null,
        detail: String::new(),
    };
    let eval = zero_engine_client::Evaluation {
        coin: Some("BTC".into()),
        direction: Some("LONG".into()),
        conviction: Some(0.72),
        regime: Some("trending".into()),
        consensus: Some(8),
        layers: vec![
            layer("layer_0", true),
            layer("layer_1", true),
            layer("layer_2", true),
        ],
        ..Default::default()
    };
    let mut state = base_state();
    state.overlay = Some(zero_tui::ActiveOverlay::Verdict(Box::new(eval)));

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("verdict_overlay_pass_btc", snap);
}

#[test]
fn widget_calibration_below_threshold_is_honest() {
    use zero_tui::widgets::calibration::{CalibrationBar, CalibrationSample};
    let sample = CalibrationSample {
        predicted: 0.70,
        observed: 0.65,
        n_samples: 12,
    };
    let snap = widget_grid(72, 1, |f| {
        f.render_widget(
            CalibrationBar {
                sample: Some(sample),
                theme: zero_tui::theme::Theme::default(),
            },
            f.area(),
        );
    });
    insta::assert_snapshot!("widget_calibration_insufficient", snap);
}

// ─── Live-stream pane — full-app snapshot ──────────────────────
//
// Rendered at 80x24 with the pane toggled on, a handful of
// synthetic events in the ring, and one synthetic broadcast-lag
// marker. Guards against regressions in:
// - layout (conversation area collapses by exactly 8 rows)
// - header ("live stream · N buffered")
// - row order (newest at the bottom)
// - honesty (lag row visible in the tail)

#[test]
fn live_stream_pane_with_mixed_events() {
    use chrono::TimeZone;
    use zero_engine_client::{EngineEvent, models::Risk};

    let mut state = base_state();
    state.live_stream_visible = true;

    let ts = |h, m, s| Utc.with_ymd_and_hms(2026, 4, 21, h, m, s).unwrap();
    state.record_engine_event(EngineEvent::Heartbeat(ts(18, 29, 55)));
    let risk = Risk {
        drawdown_pct: Some(1.25),
        daily_loss_usd: Some(5.0),
        peak_equity: Some(1000.0),
        ..Default::default()
    };
    state.record_engine_event_at(EngineEvent::Risk(Box::new(risk)), ts(18, 29, 57));
    state.record_events_lagged_at(3, ts(18, 29, 58));
    state.record_engine_event(EngineEvent::Heartbeat(ts(18, 30, 0)));

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("live_stream_pane_mixed", snap);
}

// ─── M2 §4: Risk overlay snapshots ─────────────────────────────
//
// Two 80×24 snapshots locking the Risk overlay's visual contract:
// the L3 trigger (TILT + guardrail proximity) and the L4 trigger
// (engine halted). The card shows the banner, the /risk summary
// line (drawdown % and distance Δ to the alert), and the state-
// vector proximity components so the operator sees *why* the
// overlay fired. Rendered directly over a seeded engine + state,
// constructed via the poll hook so the auto-open contract is
// exercised too (not a raw `state.overlay = …` assignment).

fn l3_overlay_state() -> AppState {
    use zero_operator_state::friction::FrictionLevel;
    let engine: Arc<RwLock<EngineState>> = EngineState::shared();
    {
        let mut e = engine.write();
        // TILT + L3 means "engine reports near-guardrail
        // proximity." Seed the matching Risk block so the /risk
        // summary line has real numbers to show.
        let snap = OperatorSnapshot {
            label: Label::Tilt,
            friction: FrictionLevel::L3,
            vector: StateVector::default(),
            as_of: frozen(),
            version: 7,
        };
        e.operator_state = Some(Stat::new(snap, Source::Ws).with_as_of(frozen()));
        let risk = Risk {
            drawdown_pct: Some(4.6),
            last_drawdown_alert_pct: Some(5.0),
            account_value: Some(9_700.0),
            peak_equity: Some(10_000.0),
            ..Default::default()
        };
        e.apply_risk(risk, frozen(), Source::Ws);
    }
    let mut s = AppState::new(engine);
    s.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    s.log
        .push(LogEntry::new(EntryKind::System, "deterministic test").at(frozen()));
    // The constructor's poll_risk_overlay already opened the
    // overlay; no manual `state.overlay = …` needed. Belt-and-
    // suspenders: assert the auto-open fired.
    assert!(
        matches!(s.overlay, Some(zero_tui::ActiveOverlay::Risk { .. })),
        "L3 snapshot must auto-open the risk overlay"
    );
    s
}

fn l4_overlay_state() -> AppState {
    use zero_operator_state::friction::FrictionLevel;
    let engine: Arc<RwLock<EngineState>> = EngineState::shared();
    {
        let mut e = engine.write();
        let snap = OperatorSnapshot {
            label: Label::Tilt,
            friction: FrictionLevel::L4,
            vector: StateVector::default(),
            as_of: frozen(),
            version: 9,
        };
        e.operator_state = Some(Stat::new(snap, Source::Ws).with_as_of(frozen()));
        let risk = Risk {
            drawdown_pct: Some(5.4),
            last_drawdown_alert_pct: Some(5.0),
            account_value: Some(9_460.0),
            peak_equity: Some(10_000.0),
            halted: true,
            halt_reason: Some("drawdown limit hit".into()),
            ..Default::default()
        };
        e.apply_risk(risk, frozen(), Source::Ws);
    }
    let mut s = AppState::new(engine);
    s.log = zero_tui::app::log::ConversationLog::with_capacity(2048);
    s.log
        .push(LogEntry::new(EntryKind::System, "deterministic test").at(frozen()));
    assert!(
        matches!(s.overlay, Some(zero_tui::ActiveOverlay::Risk { .. })),
        "L4 snapshot must auto-open the risk overlay"
    );
    s
}

#[test]
fn risk_overlay_l3_proximity() {
    let state = l3_overlay_state();
    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("risk_overlay_l3_proximity", snap);
}

#[test]
fn risk_overlay_l4_halted_banner() {
    let state = l4_overlay_state();
    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_at(f, &state, frozen())).unwrap();
    let snap = grid_to_text(&term);
    insta::assert_snapshot!("risk_overlay_l4_halted", snap);
}
