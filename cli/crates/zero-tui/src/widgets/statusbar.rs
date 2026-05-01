//! Status bar widget — always visible, single row at the bottom of
//! the screen (above the prompt).
//!
//! Layout, left-to-right (full tier, ≥ 80 cols):
//!
//!   `[MODE]  engine:<health>  feed:<age>  dd:<pct>  ops:<LABEL>`
//!
//! Plus a `retry:N` addendum when the WS is disconnected and has
//! already attempted a reconnect.
//!
//! Design rules:
//!
//! - Mode label left-aligned and colored; never truncated.
//! - Engine health is one word: OK / RECONNECTING / DOWN.
//! - Feed age renders as seconds; caution > 3 s, alert > 10 s.
//! - Drawdown reads `risk.drawdown_pct`. Thresholds:
//!   - missing → `dd:--` (muted).
//!   - halted → `dd:HALT` in alert+bold (kill_all or circuit-breaker
//!     tripped). Hides the number on purpose — "the engine is
//!     refusing new risk" is the headline; the exact dd figure is a
//!     detail the `/state` overlay has more room for.
//!   - 0 .. 2% → primary.
//!   - 2 .. 5% → caution.
//!   - > 5% → alert.
//! - **Operator-state segment is always visible, never hidden**
//!   (Addendum A §2.3 — the indicator is present even at STEADY so
//!   the operator never has to guess whether the system is
//!   watching). Appearance:
//!   - `ops:?` — classifier has not reported yet (muted).
//!   - `ops:<LABEL>` — label colored by `ColorHint`.
//!   - `ops:<LABEL>*` — label is stale (muted asterisk).
//!
//! The label is sourced from the engine's `GET /operator/state`
//! endpoint (ADR-016); the CLI never computes it locally.
//!
//! ## Width-responsive tiers
//!
//! The status bar picks the widest tier that fits in `area.width`.
//! Tiers are defined so that each narrower tier is a strict subset
//! of the tier above, and the ordering is driven by operator-safety
//! priority — we drop diagnostic segments (retry count, feed age)
//! before we drop risk segments (drawdown, halt), and we never drop
//! `ops:` at any width.
//!
//! - `Tier::Full` — all segments, double-space separators.
//! - `Tier::Compact` — drops the `retry:N` addendum and uses
//!   single-space separators between segments.
//! - `Tier::Minimal` — renders `[MODE] ops:<LABEL>  dd:<pct>` only.
//!   The operator's minimum useful floor: which mode you are in,
//!   what state the engine thinks you are in, and whether capital
//!   is bleeding.
//!
//! ## `rate:` (CLI-side) and `hl:` (Hyperliquid-side)
//!
//! The two are parallel by design so an operator reads both with
//! the same eye movement. Both use a shared "tri-color" policy:
//!
//! - headroom ≥ 25 % → primary
//! - 10 % ≤ headroom < 25 % → caution
//! - headroom < 10 %, tokens > 0 → alert
//! - tokens == 0 → `<name>:EXH` in alert+bold
//!
//! `rate:N/M` reads from the CLI-side token bucket
//! ([`zero_engine_client::RateBudget`]) that the caller hands in
//! each render. `None` → `rate:?` in metadata color (the same
//! honest-rendering rule `ops:?` uses before the classifier
//! reports).
//!
//! `hl:N/M` reads from `/v2/status.hl_rate` which the engine
//! optionally reports. Unset → `hl:?`. Once the engine-side cut
//! surfaces the field (tracked separately from the M2_PLAN row),
//! this segment starts showing live Hyperliquid pressure without
//! any CLI code change.
//!
//! ## Rendering
//!
//! The widget is pure: given a `Mode`, an `EngineState` snapshot, a
//! `Theme`, and a `now: DateTime<Utc>`, it builds a [`Line`] and
//! paints it into the buffer. All freshness math flows through the
//! caller-supplied `now` so snapshot tests can freeze time.

use chrono::{DateTime, Duration, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use zero_engine_client::{BudgetSnapshot, EngineState, HlRate, Risk};
use zero_operator_state::Snapshot as OperatorSnapshot;

use crate::app::mode::Mode;
use crate::theme::Theme;

/// How old an operator-state snapshot can get before we mark it
/// stale with a trailing `*` and de-saturate its color. The
/// classifier polls at ~5 s cadence; 30 s is 6× that, well beyond
/// "we lost the engine" noise but short enough that the operator
/// sees the change before a full minute passes.
const OPERATOR_STATE_STALE_AFTER: Duration = Duration::seconds(30);

/// Width-responsive tier picked by [`StatusBar::render`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Every segment, double-space separators.
    Full,
    /// Drop `retry:N`; single-space separators.
    Compact,
    /// `[MODE] ops:<LABEL>  dd:<pct>` only.
    Minimal,
}

#[derive(Debug)]
pub struct StatusBar<'a> {
    pub mode: Mode,
    pub engine: &'a EngineState,
    pub theme: Theme,
    /// Wall-clock "now" for freshness math. Callers from live
    /// render pass `Utc::now()`; tests pass a frozen instant so
    /// snapshots stay stable.
    pub now: DateTime<Utc>,
    /// CLI-side `RateBudget` snapshot, re-read each frame by the
    /// render pass. `None` means "no bucket attached" (test
    /// harnesses, early bootstrap) — the widget renders `rate:?`
    /// in metadata color. The live binary always attaches one
    /// (`zero::build_client`), so operators see a number from the
    /// first frame.
    pub rate_budget: Option<BudgetSnapshot>,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the area first so leftover characters from a wider
        // previous frame don't bleed through a narrower render.
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_char(' ');
            }
        }

        let line = self.build_line_for_width(area.width);
        line.render(area, buf);
    }
}

impl StatusBar<'_> {
    /// Pick the widest tier whose rendered width fits inside
    /// `available_width`. `Minimal` is the absolute floor and is
    /// never rejected by the width check — if even Minimal overflows,
    /// it still renders (and crops naturally at `area.right()` via
    /// ratatui's `Line::render`), because "mode + ops + dd" at any
    /// truncation is still more useful than a blank bar.
    #[must_use]
    pub fn pick_tier(&self, available_width: u16) -> Tier {
        for tier in [Tier::Full, Tier::Compact] {
            let line = self.build_line(tier);
            if line.width() <= usize::from(available_width) {
                return tier;
            }
        }
        Tier::Minimal
    }

    fn build_line_for_width(&self, available_width: u16) -> Line<'static> {
        let tier = self.pick_tier(available_width);
        self.build_line(tier)
    }

    fn build_line(&self, tier: Tier) -> Line<'static> {
        let mode_span = self.mode_span();
        let (ops_prefix, ops_label, ops_marker) = self.ops_spans();
        let dd_span = self.drawdown_span();
        let sep_wide = Span::styled("  ", Style::default().fg(self.theme.metadata));
        let sep_narrow = Span::styled(" ", Style::default().fg(self.theme.metadata));

        match tier {
            Tier::Full => {
                // Full tier: [MODE] engine:<> [retry:N?]  feed:…  rate:…  hl:…  dd:…  ops:…
                //
                // The `rate:` / `hl:` segments sit between `feed:`
                // and the anchored risk+ops cluster. They are
                // droppable (next tier elides them) before we
                // touch `dd:`, `ops:`, or `[MODE]` — those three
                // are the operator-safety anchors per §2.3.
                let sep = || sep_wide.clone();
                let mut spans: Vec<Span<'static>> = vec![mode_span, self.engine_span()];
                if let Some(retry) = self.retry_span() {
                    spans.push(retry);
                }
                spans.extend([sep(), self.feed_span(), sep(), self.rate_span(), sep()]);
                // `hl:` is a single Span so we push it directly
                // (Line::from expects a flat Vec<Span>).
                spans.push(self.hl_span());
                spans.extend([sep(), dd_span, sep(), ops_prefix, ops_label, ops_marker]);
                Line::from(spans)
            }
            Tier::Compact => {
                // Compact drops the `retry:` addendum and the
                // `rate:` / `hl:` diagnostic pair to make room
                // for the anchored segments. Single-space seps.
                let sep = || sep_narrow.clone();
                Line::from(vec![
                    mode_span,
                    self.engine_span(),
                    sep(),
                    self.feed_span(),
                    sep(),
                    dd_span,
                    sep(),
                    ops_prefix,
                    ops_label,
                    ops_marker,
                ])
            }
            Tier::Minimal => Line::from(vec![
                mode_span, ops_prefix, ops_label, ops_marker, sep_narrow, dd_span,
            ]),
        }
    }

    fn mode_span(&self) -> Span<'static> {
        // Leading space is intentional — the bar abuts the prompt
        // and a 1-col left margin lets the bracketed mode breathe
        // without looking glued to the screen edge.
        Span::styled(
            format!(" [{}] ", self.mode.short()),
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn engine_span(&self) -> Span<'static> {
        let (label, color) = if self.engine.connection.ws_connected {
            ("OK", self.theme.primary)
        } else if self.engine.connection.total_attempts > 0 {
            ("RECONNECTING", self.theme.caution)
        } else {
            ("DOWN", self.theme.alert)
        };
        Span::styled(format!("engine:{label}"), Style::default().fg(color))
    }

    /// Retry addendum, present only while disconnected with at
    /// least one prior attempt. Intentionally *not* separated from
    /// `engine:` by the outer separator — the two belong together
    /// as a single "connection story" in the reader's eye.
    fn retry_span(&self) -> Option<Span<'static>> {
        if !self.engine.connection.ws_connected && self.engine.connection.reconnect_count > 0 {
            Some(Span::styled(
                format!(" retry:{}", self.engine.connection.reconnect_count),
                Style::default().fg(self.theme.caution),
            ))
        } else {
            None
        }
    }

    fn feed_span(&self) -> Span<'static> {
        match self.engine.feed_age_seconds(self.now) {
            None => Span::styled("feed:--", Style::default().fg(self.theme.metadata)),
            Some(age) => {
                let color = if age < 0 {
                    // Clock skew between CLI and engine host: show
                    // the number but don't alarm on it.
                    self.theme.metadata
                } else if age <= 3 {
                    self.theme.primary
                } else if age <= 10 {
                    self.theme.caution
                } else {
                    self.theme.alert
                };
                Span::styled(format!("feed:{age}s"), Style::default().fg(color))
            }
        }
    }

    /// Build the drawdown segment. See module-level doc for the
    /// threshold table. `kill_all` / `circuit_breaker_active` take
    /// precedence over the number because "no new risk" matters
    /// more than "how deep we are".
    fn drawdown_span(&self) -> Span<'static> {
        match self.engine.risk.as_ref() {
            None => Span::styled("dd:--", Style::default().fg(self.theme.metadata)),
            Some(stat) => render_drawdown(&stat.value, &self.theme),
        }
    }

    /// CLI-side rate bucket segment. Rendering contract, in
    /// priority order:
    ///
    /// 1. `None` → `rate:?` in metadata color. No bucket was
    ///    handed in; don't lie about the state.
    /// 2. `capacity == 0` → `rate:?`. Defensive: a zero-capacity
    ///    snapshot would divide-by-zero on `headroom()`; treat
    ///    it as "unknown" and show the honest placeholder.
    /// 3. `tokens == 0` → `rate:EXH` in alert+bold. The next
    ///    request *will* be refused locally; the operator needs
    ///    to see that plainly.
    /// 4. Otherwise `rate:N/M` colored by `headroom()` thresholds.
    fn rate_span(&self) -> Span<'static> {
        let prefix = "rate:";
        let Some(snap) = self.rate_budget else {
            return Span::styled(
                format!("{prefix}?"),
                Style::default().fg(self.theme.metadata),
            );
        };
        if snap.capacity == 0 {
            return Span::styled(
                format!("{prefix}?"),
                Style::default().fg(self.theme.metadata),
            );
        }
        if snap.tokens == 0 {
            return Span::styled(
                format!("{prefix}EXH"),
                Style::default()
                    .fg(self.theme.alert)
                    .add_modifier(Modifier::BOLD),
            );
        }
        Span::styled(
            format!("{prefix}{}/{}", snap.tokens, snap.capacity),
            Style::default().fg(self.pressure_color(snap.headroom())),
        )
    }

    /// Hyperliquid rate segment sourced from `/v2/status.hl_rate`.
    /// Mirrors `rate_span`'s tri-color policy so both segments
    /// share the same visual language.
    fn hl_span(&self) -> Span<'static> {
        let prefix = "hl:";
        let Some(HlRate { used, cap }) = self.engine.hl_rate_snapshot() else {
            return Span::styled(
                format!("{prefix}?"),
                Style::default().fg(self.theme.metadata),
            );
        };
        if cap == 0 {
            return Span::styled(
                format!("{prefix}?"),
                Style::default().fg(self.theme.metadata),
            );
        }
        // `used` can legitimately exceed `cap` during immune
        // bypass overshoot (engine/shared/http.py:_hl_check_global_rate
        // keeps counting immune calls past the cap). Treat any
        // `used >= cap` as EXH — the operator needs to see "we
        // are at or past the HL cap", not the exact overshoot.
        if used >= cap {
            return Span::styled(
                format!("{prefix}EXH"),
                Style::default()
                    .fg(self.theme.alert)
                    .add_modifier(Modifier::BOLD),
            );
        }
        // Headroom math mirrors BudgetSnapshot::headroom.
        let headroom = f64::from(cap.saturating_sub(used)) / f64::from(cap);
        Span::styled(
            format!("{prefix}{used}/{cap}"),
            Style::default().fg(self.pressure_color(headroom)),
        )
    }

    /// Shared tri-color threshold for `rate:` and `hl:`:
    /// primary ≥ 25 %, caution 10..25 %, alert < 10 %.
    fn pressure_color(&self, headroom: f64) -> ratatui::style::Color {
        if headroom >= 0.25 {
            self.theme.primary
        } else if headroom >= 0.10 {
            self.theme.caution
        } else {
            self.theme.alert
        }
    }

    fn ops_spans(&self) -> (Span<'static>, Span<'static>, Span<'static>) {
        let metadata = self.theme.metadata;
        let prefix = Span::styled("ops:", Style::default().fg(metadata));
        match &self.engine.operator_state {
            None => (
                prefix,
                Span::styled("?", Style::default().fg(metadata)),
                Span::raw(""),
            ),
            Some(stat) => {
                let snap: &OperatorSnapshot = &stat.value;
                let color = self.theme.resolve_hint(snap.label.color_hint());
                let stale = stat.is_stale(self.now, OPERATOR_STATE_STALE_AFTER);
                let label_color = if stale { metadata } else { color };
                let label_span = Span::styled(
                    snap.label.short().to_string(),
                    Style::default()
                        .fg(label_color)
                        .add_modifier(Modifier::BOLD),
                );
                let marker_span = if stale {
                    Span::styled("*", Style::default().fg(metadata))
                } else {
                    Span::raw("")
                };
                (prefix, label_span, marker_span)
            }
        }
    }
}

fn render_drawdown(risk: &Risk, theme: &Theme) -> Span<'static> {
    if risk.is_halted() {
        return Span::styled(
            "dd:HALT",
            Style::default()
                .fg(theme.alert)
                .add_modifier(Modifier::BOLD),
        );
    }
    match risk.drawdown_pct {
        None => Span::styled("dd:--", Style::default().fg(theme.metadata)),
        Some(pct) => {
            // `pct` is engine-side magnitude, always ≥ 0 in normal
            // operation. Guard against stray negative values by
            // treating them as 0 for color selection — honesty
            // matters more than color correctness on malformed data.
            let magnitude = pct.max(0.0);
            let color = if magnitude <= 2.0 {
                theme.primary
            } else if magnitude <= 5.0 {
                theme.caution
            } else {
                theme.alert
            };
            Span::styled(format!("dd:{pct:.1}%"), Style::default().fg(color))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use zero_engine_client::{Source, Stat, V2Status};
    use zero_operator_state::{Label, StateVector};

    fn snapshot_at(label: Label, as_of: DateTime<Utc>) -> Stat<OperatorSnapshot> {
        let snap = OperatorSnapshot::new(label, StateVector::default(), as_of, 1);
        Stat::new(snap, Source::Http).with_as_of(as_of)
    }

    fn risk_stat(risk: Risk, as_of: DateTime<Utc>) -> Stat<Risk> {
        Stat::new(risk, Source::Ws).with_as_of(as_of)
    }

    fn render_bar_at(engine: &EngineState, now: DateTime<Utc>, width: u16) -> Vec<String> {
        let backend = TestBackend::new(width, 1);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| {
            let bar = StatusBar {
                mode: Mode::Conversation,
                engine,
                theme: Theme::default(),
                now,
                rate_budget: None,
            };
            f.render_widget(bar, f.area());
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn render_bar(engine: &EngineState, now: DateTime<Utc>) -> Vec<String> {
        render_bar_at(engine, now, 80)
    }

    #[test]
    fn unseen_snapshot_renders_question_mark() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let engine = EngineState::new();
        let lines = render_bar(&engine, now);
        assert!(
            lines[0].contains("ops:?"),
            "expected ops:? placeholder, got {lines:?}"
        );
    }

    #[test]
    fn fresh_snapshot_renders_label() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Steady, now));
        let lines = render_bar(&engine, now);
        assert!(
            lines[0].contains("ops:STEADY"),
            "expected ops:STEADY, got {lines:?}"
        );
        assert!(
            !lines[0].contains("STEADY*"),
            "fresh snapshot should not carry staleness marker: {lines:?}"
        );
    }

    #[test]
    fn stale_snapshot_gets_asterisk() {
        let as_of = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let now = as_of + Duration::seconds(60);
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Tilt, as_of));
        let lines = render_bar(&engine, now);
        assert!(
            lines[0].contains("ops:TILT*"),
            "stale TILT should render with asterisk: {lines:?}"
        );
    }

    #[test]
    fn every_label_has_a_rendered_form() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        for (label, expected) in [
            (Label::Fresh, "ops:FRESH"),
            (Label::Steady, "ops:STEADY"),
            (Label::Elevated, "ops:ELEVATED"),
            (Label::Tilt, "ops:TILT"),
            (Label::Fatigued, "ops:FATIGUED"),
            (Label::Recovery, "ops:RECOVERY"),
        ] {
            let mut engine = EngineState::new();
            engine.operator_state = Some(snapshot_at(label, now));
            let lines = render_bar(&engine, now);
            assert!(
                lines[0].contains(expected),
                "label {label:?} should render as {expected}, got {lines:?}"
            );
        }
    }

    #[test]
    fn drawdown_missing_shows_placeholder() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let engine = EngineState::new();
        let lines = render_bar(&engine, now);
        assert!(lines[0].contains("dd:--"), "got {lines:?}");
    }

    #[test]
    fn drawdown_renders_one_decimal_percent() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        let risk = Risk {
            drawdown_pct: Some(1.23),
            ..Default::default()
        };
        engine.risk = Some(risk_stat(risk, now));
        let lines = render_bar(&engine, now);
        assert!(lines[0].contains("dd:1.2%"), "got {lines:?}");
    }

    #[test]
    fn drawdown_halted_reads_halt() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        let risk = Risk {
            drawdown_pct: Some(0.5),
            halted: true,
            ..Default::default()
        };
        engine.risk = Some(risk_stat(risk, now));
        let lines = render_bar(&engine, now);
        assert!(
            lines[0].contains("dd:HALT"),
            "halt must override the number: {lines:?}"
        );
        assert!(
            !lines[0].contains("dd:0.5%"),
            "number must not leak when halted: {lines:?}"
        );
    }

    #[test]
    fn drawdown_circuit_breaker_reads_halt() {
        // Any halt-family flag flips to HALT — not just `halted`.
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        let risk = Risk {
            drawdown_pct: Some(3.0),
            global_halt: true,
            ..Default::default()
        };
        engine.risk = Some(risk_stat(risk, now));
        let lines = render_bar(&engine, now);
        assert!(lines[0].contains("dd:HALT"), "got {lines:?}");
    }

    #[test]
    fn minimal_tier_drops_engine_and_feed() {
        // 40 cols forces Minimal. `[CONV] ops:STEADY  dd:1.0%` fits
        // inside that. The important contract is: `engine:` and
        // `feed:` are gone, but `ops:` and `dd:` survive.
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Steady, now));
        engine.risk = Some(risk_stat(
            Risk {
                drawdown_pct: Some(1.0),
                ..Default::default()
            },
            now,
        ));

        let lines = render_bar_at(&engine, now, 40);
        assert!(lines[0].contains("ops:STEADY"), "got {lines:?}");
        assert!(lines[0].contains("dd:1.0%"), "got {lines:?}");
        assert!(
            !lines[0].contains("engine:"),
            "minimal drops engine: {lines:?}"
        );
        assert!(!lines[0].contains("feed:"), "minimal drops feed: {lines:?}");
    }

    #[test]
    fn full_tier_includes_all_segments_at_120_cols() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Elevated, now));
        engine.risk = Some(risk_stat(
            Risk {
                drawdown_pct: Some(2.5),
                ..Default::default()
            },
            now,
        ));
        engine.apply_status(V2Status::default(), now, Source::Ws);
        engine.on_ws_connected();

        let lines = render_bar_at(&engine, now, 120);
        for needle in [" [CONV]", "engine:OK", "feed:0s", "dd:2.5%", "ops:ELEVATED"] {
            assert!(
                lines[0].contains(needle),
                "full tier missing {needle}: {lines:?}"
            );
        }
    }

    #[test]
    fn pick_tier_prefers_widest_fit() {
        // Freeze a scenario, ask pick_tier at three widths, expect
        // strictly monotonic narrowing as the budget shrinks.
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Elevated, now));
        engine.risk = Some(risk_stat(
            Risk {
                drawdown_pct: Some(3.3),
                ..Default::default()
            },
            now,
        ));
        engine.on_ws_connected();

        let make_bar = |w: u16| {
            let bar = StatusBar {
                mode: Mode::Conversation,
                engine: &engine,
                theme: Theme::default(),
                now,
                rate_budget: None,
            };
            bar.pick_tier(w)
        };

        assert_eq!(make_bar(200), Tier::Full);
        assert_eq!(make_bar(80), Tier::Full);
        // 30 cols is well below both Full and Compact widths.
        assert_eq!(make_bar(30), Tier::Minimal);
    }

    // ── rate: / hl: unit coverage (M2 §2) ─────────────────────
    //
    // The fault-matrix integration suite pins the full-line
    // rendering at every canonical width; these tests pin the
    // *per-segment* contract in isolation so a color-threshold
    // regression surfaces here first (smaller diff, faster
    // iteration).

    fn bar_with_budget(snap: BudgetSnapshot) -> StatusBar<'static> {
        // The widget is `'static` here because the engine is a
        // leaked default — tests only call `build_line(...)`
        // which reads no `'a` lifetime-tied data beyond
        // `theme`+`rate_budget`, both owned.
        let engine: &'static EngineState = Box::leak(Box::new(EngineState::new()));
        StatusBar {
            mode: Mode::Conversation,
            engine,
            theme: Theme::default(),
            now: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            rate_budget: Some(snap),
        }
    }

    #[test]
    fn rate_segment_is_question_mark_without_bucket() {
        let engine = EngineState::new();
        let bar = StatusBar {
            mode: Mode::Conversation,
            engine: &engine,
            theme: Theme::default(),
            now: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            rate_budget: None,
        };
        let span = bar.rate_span();
        assert_eq!(span.content, "rate:?");
    }

    #[test]
    fn rate_segment_renders_n_over_m() {
        let bar = bar_with_budget(BudgetSnapshot {
            capacity: 60,
            refill_per_second: 1.0,
            tokens: 42,
        });
        assert_eq!(bar.rate_span().content, "rate:42/60");
    }

    #[test]
    fn rate_segment_renders_exh_at_zero_tokens() {
        let bar = bar_with_budget(BudgetSnapshot {
            capacity: 60,
            refill_per_second: 1.0,
            tokens: 0,
        });
        let span = bar.rate_span();
        assert_eq!(span.content, "rate:EXH");
        assert!(
            span.style.add_modifier.contains(Modifier::BOLD),
            "rate:EXH must render bold for at-a-glance operator visibility",
        );
    }

    #[test]
    fn rate_segment_zero_capacity_renders_question_mark() {
        // Defensive: a bucket with capacity 0 would divide-by-
        // zero on headroom; treat it as unknown rather than EXH
        // because a zero-capacity bucket is a config bug and
        // `?` is the honest render for "we don't know".
        let bar = bar_with_budget(BudgetSnapshot {
            capacity: 0,
            refill_per_second: 0.0,
            tokens: 0,
        });
        assert_eq!(bar.rate_span().content, "rate:?");
    }

    #[test]
    fn rate_segment_color_bands_cover_all_headroom_regions() {
        let theme = Theme::default();
        let mk = |tokens: u32| {
            bar_with_budget(BudgetSnapshot {
                capacity: 60,
                refill_per_second: 1.0,
                tokens,
            })
            .rate_span()
            .style
            .fg
            .unwrap()
        };
        // 60 → 100 % headroom → primary
        assert_eq!(mk(60), theme.primary);
        // 15 / 60 = 25 % → primary (≥25 % band's lower edge)
        assert_eq!(mk(15), theme.primary);
        // 14 / 60 ≈ 23 % → caution
        assert_eq!(mk(14), theme.caution);
        // 7 / 60 ≈ 11 % → caution (still ≥ 10 %)
        assert_eq!(mk(7), theme.caution);
        // 5 / 60 ≈ 8 % → alert
        assert_eq!(mk(5), theme.alert);
        // 1 / 60 ≈ 1.7 % → alert
        assert_eq!(mk(1), theme.alert);
    }

    #[test]
    fn hl_segment_is_question_mark_when_engine_silent() {
        let engine = EngineState::new();
        let bar = StatusBar {
            mode: Mode::Conversation,
            engine: &engine,
            theme: Theme::default(),
            now: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            rate_budget: None,
        };
        assert_eq!(bar.hl_span().content, "hl:?");
    }

    #[test]
    fn hl_segment_renders_used_over_cap_from_v2status() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.apply_status(
            V2Status {
                hl_rate: Some(zero_engine_client::HlRate {
                    used: 120,
                    cap: 240,
                }),
                ..V2Status::default()
            },
            now,
            Source::Ws,
        );
        let bar = StatusBar {
            mode: Mode::Conversation,
            engine: &engine,
            theme: Theme::default(),
            now,
            rate_budget: None,
        };
        assert_eq!(bar.hl_span().content, "hl:120/240");
    }

    #[test]
    fn hl_segment_overshoot_renders_exh() {
        // `used >= cap` is the immune-bypass overshoot signal;
        // the widget renders `hl:EXH` rather than, say,
        // `hl:245/240` because operators need a single-glance
        // "we are at or past the wall", not a bookkeeping line.
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.apply_status(
            V2Status {
                hl_rate: Some(zero_engine_client::HlRate {
                    used: 245,
                    cap: 240,
                }),
                ..V2Status::default()
            },
            now,
            Source::Ws,
        );
        let bar = StatusBar {
            mode: Mode::Conversation,
            engine: &engine,
            theme: Theme::default(),
            now,
            rate_budget: None,
        };
        let span = bar.hl_span();
        assert_eq!(span.content, "hl:EXH");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn narrow_render_never_wraps_or_panics() {
        // Regression: rendering a 20-column bar used to panic when
        // the minimal line was wider than the area. Now we just
        // crop at area.right() via ratatui and keep going.
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut engine = EngineState::new();
        engine.operator_state = Some(snapshot_at(Label::Tilt, now));
        let lines = render_bar_at(&engine, now, 20);
        assert_eq!(lines.len(), 1, "status bar is single-row: {lines:?}");
        assert_eq!(lines[0].chars().count(), 20);
    }
}
