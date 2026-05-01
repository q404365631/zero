//! Single-position row — the reusable primitive behind the
//! Positions pane and any future verdict overlay that embeds an
//! open-position snapshot.
//!
//! # Layout
//!
//! Columns, left to right, with a one-space gutter between each:
//!
//! ```text
//!  BTC     long  size=0.4200  entry=64120.50  mark=64480.00  pnl=+151.13 (+0.82R)  stop=63500  tgt=66000
//! ```
//!
//! Column widths are fixed so the eye can scan down a column
//! without jitter. Missing optional fields (`mark`, `pnl`, `stop`,
//! `target`) render as `—` so the row still occupies its slot
//! and columns don't shift.
//!
//! # Color rules (operator-safety)
//!
//! - Symbol / side → `theme.primary` (bright, first-glance anchor).
//! - Numeric fill (`size`, `entry`, `mark`) → `theme.metadata`
//!   (low-contrast; ops read these after verifying the symbol).
//! - `pnl` is the one column whose color changes with value:
//!   - `>= 0`  → `theme.primary` (positive).
//!   - `<  0`  → `theme.alert` (negative).
//!   - `None`  → `theme.metadata` (not yet observed).
//! - `stop` / `target` → `theme.caution` only if the mark has
//!   crossed the level (stop hit while position open, target hit
//!   while position open); otherwise `theme.metadata`. This is a
//!   cheap early-warning cue; full guardrail-proximity coloring
//!   lands with the risk-overlay pass.
//!
//! The widget does **not** flag stale data — freshness is the
//! job of the parent pane's `Stat<Positions>` badge. Coloring a
//! single row as stale would hide the fact that the whole feed
//! is behind.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use zero_engine_client::Position;

use crate::theme::Theme;

#[derive(Debug)]
pub struct PositionRow<'a> {
    pub position: &'a Position,
    pub theme: Theme,
}

impl Widget for PositionRow<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        // One row max — callers composing into a pane allocate
        // their own row rects; this keeps the widget dumb.
        let row = Rect { height: 1, ..area };
        Line::from(self.spans()).render(row, buf);
    }
}

impl PositionRow<'_> {
    /// The rendered span vector. Exposed so callers that want to
    /// embed the row inside another `Line` (e.g. a verdict card
    /// with a leading chevron) can compose without re-rendering.
    #[must_use]
    pub fn spans(&self) -> Vec<Span<'static>> {
        let p = self.position;
        let t = &self.theme;

        let sym_style = Style::default().fg(t.primary).add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(t.metadata);

        let pnl_style = match p.unrealized_pnl {
            Some(v) if v > 0.0 => Style::default().fg(t.primary),
            Some(v) if v < 0.0 => Style::default().fg(t.alert),
            _ => dim,
        };

        let stop_style = if stop_breached(p) {
            Style::default().fg(t.caution).add_modifier(Modifier::BOLD)
        } else {
            dim
        };
        let target_style = if target_reached(p) {
            Style::default().fg(t.primary).add_modifier(Modifier::BOLD)
        } else {
            dim
        };

        vec![
            Span::styled(format!(" {:<6}", p.symbol), sym_style),
            Span::styled(format!(" {:<5}", p.side), dim),
            Span::styled(format!(" size={:<8.4}", p.size), dim),
            Span::styled(format!(" entry={:<10}", format!("{:.2}", p.entry)), dim),
            Span::styled(format!(" mark={:<10}", fmt_opt_price(p.mark)), dim),
            Span::styled(
                format!(" pnl={:<10}", fmt_opt_signed(p.unrealized_pnl)),
                pnl_style,
            ),
            Span::styled(format!(" {:<7}", fmt_r(p.unrealized_r)), pnl_style),
            Span::styled(format!(" stop={:<8}", fmt_opt_price(p.stop)), stop_style),
            Span::styled(format!(" tgt={:<8}", fmt_opt_price(p.target)), target_style),
        ]
    }
}

fn fmt_opt_price(v: Option<f64>) -> String {
    v.map_or_else(|| "—".to_string(), |x| format!("{x:.2}"))
}

fn fmt_opt_signed(v: Option<f64>) -> String {
    v.map_or_else(|| "—".to_string(), |x| format!("{x:+.2}"))
}

fn fmt_r(v: Option<f64>) -> String {
    v.map_or_else(|| "—".to_string(), |x| format!("{x:+.2}R"))
}

/// True when the position is on the losing side of its stop:
/// long + mark ≤ stop, or short + mark ≥ stop. Conservative —
/// we only flag when all three of (stop, mark, side) are known.
fn stop_breached(p: &Position) -> bool {
    let (Some(mark), Some(stop)) = (p.mark, p.stop) else {
        return false;
    };
    match p.side.as_str() {
        "long" => mark <= stop,
        "short" => mark >= stop,
        _ => false,
    }
}

fn target_reached(p: &Position) -> bool {
    let (Some(mark), Some(tgt)) = (p.mark, p.target) else {
        return false;
    };
    match p.side.as_str() {
        "long" => mark >= tgt,
        "short" => mark <= tgt,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(position: &Position, width: u16) -> String {
        let backend = TestBackend::new(width, 1);
        let mut term = Terminal::new(backend).expect("term");
        term.draw(|f| {
            let w = PositionRow {
                position,
                theme: Theme::default(),
            };
            f.render_widget(w, f.area());
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        let mut s = String::new();
        for x in 0..buf.area.width {
            s.push_str(buf[(x, 0)].symbol());
        }
        s
    }

    fn btc_long() -> Position {
        Position {
            symbol: "BTC".into(),
            side: "long".into(),
            size: 0.42,
            entry: 64_120.50,
            mark: Some(64_480.00),
            unrealized_pnl: Some(151.13),
            unrealized_r: Some(0.82),
            stop: Some(63_500.0),
            target: Some(66_000.0),
            ..Default::default()
        }
    }

    #[test]
    fn renders_key_fields_in_order() {
        let p = btc_long();
        let line = render(&p, 120);
        assert!(line.contains("BTC"), "symbol missing: {line:?}");
        assert!(line.contains("long"), "side missing: {line:?}");
        assert!(line.contains("size=0.4200"), "size missing: {line:?}");
        assert!(line.contains("entry=64120.50"), "entry missing: {line:?}");
        assert!(line.contains("mark=64480.00"), "mark missing: {line:?}");
        assert!(line.contains("pnl=+151.13"), "pnl missing: {line:?}");
        assert!(line.contains("+0.82R"), "R missing: {line:?}");
        assert!(line.contains("stop=63500"), "stop missing: {line:?}");
        assert!(line.contains("tgt=66000"), "target missing: {line:?}");
    }

    #[test]
    fn missing_optional_fields_render_as_em_dash() {
        let mut p = btc_long();
        p.mark = None;
        p.unrealized_pnl = None;
        p.unrealized_r = None;
        p.stop = None;
        p.target = None;
        let line = render(&p, 120);
        for field in ["mark=—", "pnl=—", "stop=—", "tgt=—"] {
            assert!(line.contains(field), "missing {field}: {line:?}");
        }
        // R column renders as a bare em-dash (no `R` suffix) when
        // absent — the dash alone is the honest empty state.
        assert!(line.contains(" — "), "expected R column em-dash: {line:?}");
    }

    #[test]
    fn stop_breach_detected_for_long_when_mark_at_or_below_stop() {
        let mut p = btc_long();
        p.mark = Some(63_500.0);
        assert!(stop_breached(&p), "long with mark==stop must be breached");
        p.mark = Some(63_000.0);
        assert!(stop_breached(&p), "long with mark<stop must be breached");
        p.mark = Some(64_000.0);
        assert!(
            !stop_breached(&p),
            "long with mark>stop must not be breached"
        );
    }

    #[test]
    fn stop_breach_detected_for_short_when_mark_at_or_above_stop() {
        let mut p = btc_long();
        p.side = "short".into();
        p.stop = Some(64_500.0);
        p.mark = Some(64_500.0);
        assert!(stop_breached(&p));
        p.mark = Some(65_000.0);
        assert!(stop_breached(&p));
        p.mark = Some(64_000.0);
        assert!(!stop_breached(&p));
    }

    #[test]
    fn target_reached_mirrors_direction() {
        let mut p = btc_long();
        p.mark = Some(66_000.0);
        assert!(target_reached(&p));
        p.mark = Some(65_999.0);
        assert!(!target_reached(&p));
        p.side = "short".into();
        p.target = Some(63_000.0);
        p.mark = Some(63_000.0);
        assert!(target_reached(&p));
        p.mark = Some(63_500.0);
        assert!(!target_reached(&p));
    }

    #[test]
    fn unknown_side_never_flags_breach() {
        let mut p = btc_long();
        p.side = "flat".into();
        p.mark = Some(0.0);
        p.stop = Some(1.0);
        p.target = Some(2.0);
        assert!(!stop_breached(&p));
        assert!(!target_reached(&p));
    }
}
