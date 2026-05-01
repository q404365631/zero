//! Verdict block — card-style widget that renders a single
//! [`Evaluation`] as a compact, scannable decision surface.
//!
//! # Layout
//!
//! ```text
//!  PASS  BTC  conf 72%
//!  ├─ stage1 : PASS
//!  ├─ stage2 : HOLD
//!  └─ stage3 : PASS
//!  rationale: trend aligned with regime; volume confirming.
//! ```
//!
//! - Row 1 is the **verdict chip** — one of `PASS`, `HOLD`,
//!   `REJECT`, or a pass-through of whatever the engine sent if
//!   it's a string we don't recognize. Color tracks the severity:
//!   green (primary) on PASS, amber (caution) on HOLD, red
//!   (alert + bold) on REJECT. Everything else renders in
//!   metadata color so a typo on the engine side does not look
//!   authoritative.
//! - Confidence is rendered as an integer percentage next to
//!   the chip. Values outside `[0.0, 1.0]` are rounded into
//!   range — we do not display `200%`.
//! - Gates are stacked vertically, sorted lexically for stable
//!   rendering across frames. Gate statuses use the same
//!   severity palette as the verdict chip.
//! - Rationale wraps to one row and is truncated with `…` at
//!   the right edge when it doesn't fit. The full text lives in
//!   the engine's response; the widget's job is at-a-glance
//!   triage, not prose.
//!
//! # Honest "no verdict" state
//!
//! When `evaluation.verdict` is `None` the widget renders a
//! single low-contrast row:
//!
//! ```text
//!  (no verdict — `/evaluate <coin>` to request one)
//! ```
//!
//! No chip, no fake gates, no placeholder confidence bar. The
//! widget never fabricates data — the verdict block is a trust
//! surface, and an empty gate table rendered as `?` or `--`
//! would be visually indistinguishable from a real partial
//! engine response.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use zero_engine_client::Evaluation;

use crate::theme::Theme;

/// Normalized verdict severity. Maps the engine's string verdict
/// onto a finite palette so the widget's color rules are
/// exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictSeverity {
    Pass,
    Hold,
    Reject,
    Unknown,
}

impl VerdictSeverity {
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "PASS" | "APPROVE" | "OK" => Self::Pass,
            "HOLD" | "WAIT" | "PARTIAL" => Self::Hold,
            "REJECT" | "DENY" | "FAIL" => Self::Reject,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    const fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Hold => "HOLD",
            Self::Reject => "REJECT",
            Self::Unknown => "?",
        }
    }

    fn style(self, theme: &Theme) -> Style {
        match self {
            Self::Pass => Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
            Self::Hold => Style::default()
                .fg(theme.caution)
                .add_modifier(Modifier::BOLD),
            Self::Reject => Style::default()
                .fg(theme.alert)
                .add_modifier(Modifier::BOLD),
            Self::Unknown => Style::default()
                .fg(theme.metadata)
                .add_modifier(Modifier::DIM),
        }
    }
}

#[derive(Debug)]
pub struct VerdictBlock<'a> {
    pub evaluation: &'a Evaluation,
    pub theme: Theme,
}

impl Widget for VerdictBlock<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        // Clear first so a shrinking pane does not leave ghost
        // glyphs from the previous verdict.
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_char(' ');
            }
        }

        // The engine's `/evaluate/{coin}` response is always
        // populated, so an empty `layers` list is the reliable
        // "nothing to show" sentinel — not a missing verdict
        // string. A populated evaluation means we can always
        // derive PASS / HOLD / REJECT from the layer results.
        if self.evaluation.layers.is_empty() && self.evaluation.direction.is_none() {
            let line = Line::from(vec![Span::styled(
                " (no verdict — `/evaluate <coin>` to request one)",
                Style::default().fg(self.theme.metadata),
            )]);
            line.render(row(area, 0), buf);
            return;
        }

        let sev = VerdictSeverity::parse(self.evaluation.verdict());

        // Row 0 — chip + coin + confidence.
        let chip = Span::styled(
            format!(" {} ", sev.label()),
            sev.style(&self.theme).add_modifier(Modifier::REVERSED),
        );
        let coin = Span::styled(
            format!(" {}", self.evaluation.coin.as_deref().unwrap_or("?")),
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        );
        let conf = Span::styled(
            format!(" conf {}%", confidence_pct(self.evaluation.conviction)),
            Style::default().fg(self.theme.metadata),
        );
        Line::from(vec![chip, coin, conf]).render(row(area, 0), buf);

        // Rows 1..=N — layer table in engine order. The engine
        // already decides the row sequence; we preserve it so
        // `layer_0 .. layer_N` reads top-to-bottom the way the
        // gate stack is written in the engine source, rather
        // than re-sorting to a lexical order that would scramble
        // `layer_10` above `layer_2`.
        let layer_count = self.evaluation.layers.len();
        for (i, layer) in self.evaluation.layers.iter().enumerate() {
            let y = 1 + u16::try_from(i).unwrap_or(u16::MAX);
            let target_row = row(area, y);
            if target_row.height == 0 {
                break;
            }
            let is_last = i + 1 == layer_count;
            let connector = if is_last { "└─ " } else { "├─ " };
            let status = if layer.passed { "PASS" } else { "REJECT" };
            let gate_sev = VerdictSeverity::parse(status);
            Line::from(vec![
                Span::styled(
                    format!(" {connector}"),
                    Style::default().fg(self.theme.metadata),
                ),
                Span::styled(
                    format!("{:<10}", layer.layer),
                    Style::default().fg(self.theme.metadata),
                ),
                Span::styled(format!(" : {status}"), gate_sev.style(&self.theme)),
            ])
            .render(target_row, buf);
        }

        // Rationale — synthesized from the real fields the engine
        // emits: regime + direction + consensus. When the engine
        // later adds a `rationale` string we can prefer it, but
        // today the wire format does not carry one.
        let rationale = synthesize_rationale(self.evaluation);
        if !rationale.is_empty() {
            let y = 1 + u16::try_from(layer_count).unwrap_or(u16::MAX);
            let target_row = row(area, y);
            if target_row.height > 0 {
                let available = usize::from(target_row.width).saturating_sub(13);
                let clipped = truncate_with_ellipsis(&rationale, available);
                Line::from(vec![
                    Span::styled(" rationale: ", Style::default().fg(self.theme.metadata)),
                    Span::styled(clipped, Style::default().fg(self.theme.primary)),
                ])
                .render(target_row, buf);
            }
        }
    }
}

/// Build a one-line rationale from the fields the engine actually
/// emits. Returns an empty string when nothing useful is available,
/// which the widget treats as "skip the rationale row."
fn synthesize_rationale(e: &Evaluation) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(dir) = e.direction.as_deref().filter(|d| !d.is_empty()) {
        parts.push(format!("direction {dir}"));
    }
    if let Some(reg) = e.regime.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("regime {reg}"));
    }
    if let Some(cons) = e.consensus {
        parts.push(format!("consensus {cons}"));
    }
    parts.join(" · ")
}

fn confidence_pct(v: Option<f64>) -> i32 {
    let Some(x) = v else {
        return 0;
    };
    // Input is clamped to `[0.0, 1.0]` before scaling, so the
    // product is in `[0.0, 100.0]` — well within `i32` range and
    // never negative. The cast is bounded, not truncating.
    #[allow(clippy::cast_possible_truncation)]
    let pct = (x.clamp(0.0, 1.0) * 100.0).round() as i32;
    pct
}

fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    // Reserve one cell for the ellipsis.
    let keep = max_chars.saturating_sub(1);
    let prefix: String = s.chars().take(keep).collect();
    format!("{prefix}…")
}

fn row(area: Rect, y_offset: u16) -> Rect {
    let abs_y = area.y.saturating_add(y_offset);
    if abs_y >= area.bottom() {
        return Rect {
            x: area.x,
            y: area.bottom().saturating_sub(1),
            width: area.width,
            height: 0,
        };
    }
    Rect {
        x: area.x,
        y: abs_y,
        width: area.width,
        height: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use zero_engine_client::models::EvaluationLayer;

    fn render(e: &Evaluation, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).expect("term");
        term.draw(|f| {
            let w = VerdictBlock {
                evaluation: e,
                theme: Theme::default(),
            };
            f.render_widget(w, f.area());
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn layer(name: &str, passed: bool) -> EvaluationLayer {
        EvaluationLayer {
            layer: name.into(),
            passed,
            value: serde_json::Value::Null,
            detail: String::new(),
        }
    }

    fn pass_eval() -> Evaluation {
        Evaluation {
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
        }
    }

    #[test]
    fn renders_verdict_coin_and_confidence() {
        let lines = render(&pass_eval(), 60, 6);
        assert!(lines[0].contains("PASS"), "verdict chip missing: {lines:?}");
        assert!(lines[0].contains("BTC"), "coin missing: {lines:?}");
        assert!(
            lines[0].contains("conf 72%"),
            "confidence missing: {lines:?}"
        );
    }

    #[test]
    fn layers_render_in_engine_order_with_tree_connectors() {
        let lines = render(&pass_eval(), 60, 6);
        assert!(lines[1].contains("├─ layer_0"), "row1 wrong: {lines:?}");
        assert!(lines[2].contains("├─ layer_1"), "row2 wrong: {lines:?}");
        assert!(
            lines[3].contains("└─ layer_2"),
            "row3 wrong (last): {lines:?}"
        );
    }

    #[test]
    fn rejected_layer_marks_overall_verdict_reject() {
        let mut e = pass_eval();
        e.layers[1].passed = false;
        let lines = render(&e, 60, 6);
        assert!(
            lines[0].contains("REJECT"),
            "overall verdict should flip to REJECT when any layer fails: {lines:?}"
        );
    }

    #[test]
    fn rationale_synthesizes_from_direction_regime_consensus() {
        let lines = render(&pass_eval(), 60, 6);
        let rat = lines
            .iter()
            .find(|l| l.contains("rationale:"))
            .expect("rationale row must render");
        assert!(rat.contains("direction LONG"), "direction missing: {rat:?}");
        assert!(rat.contains("regime trending"), "regime missing: {rat:?}");
        assert!(rat.contains("consensus 8"), "consensus missing: {rat:?}");
    }

    #[test]
    fn long_rationale_truncates_to_fit() {
        let mut e = pass_eval();
        e.regime = Some("x".repeat(500));
        let lines = render(&e, 40, 6);
        let rat = lines
            .iter()
            .find(|l| l.contains("rationale:"))
            .expect("rationale row must render");
        assert!(rat.contains('…'), "long rationale must ellipsize: {rat:?}");
        assert!(
            rat.chars().count() <= 40,
            "rationale must fit within width: {rat:?}"
        );
    }

    #[test]
    fn missing_verdict_renders_honest_empty_row() {
        let e = Evaluation::default();
        let lines = render(&e, 60, 3);
        assert!(
            lines[0].contains("no verdict"),
            "expected honest empty state: {lines:?}"
        );
        for needle in ["PASS", "REJECT", "├─", "conf "] {
            for line in &lines {
                assert!(!line.contains(needle), "fake {needle} leaked: {line:?}");
            }
        }
    }

    #[test]
    fn hold_when_all_pass_but_direction_none() {
        let mut e = pass_eval();
        e.direction = Some("NONE".into());
        let lines = render(&e, 60, 6);
        assert!(
            lines[0].contains("HOLD"),
            "direction=NONE with all layers passing should be HOLD: {lines:?}"
        );
    }

    #[test]
    fn confidence_clamps_out_of_range_values() {
        assert_eq!(confidence_pct(Some(-0.2)), 0);
        assert_eq!(confidence_pct(Some(1.4)), 100);
        assert_eq!(confidence_pct(None), 0);
        assert_eq!(confidence_pct(Some(0.5)), 50);
    }

    #[test]
    fn verdict_severity_parses_common_strings() {
        assert_eq!(VerdictSeverity::parse("PASS"), VerdictSeverity::Pass);
        assert_eq!(VerdictSeverity::parse("pass"), VerdictSeverity::Pass);
        assert_eq!(VerdictSeverity::parse("HOLD"), VerdictSeverity::Hold);
        assert_eq!(VerdictSeverity::parse("REJECT"), VerdictSeverity::Reject);
        assert_eq!(VerdictSeverity::parse(""), VerdictSeverity::Unknown);
        assert_eq!(VerdictSeverity::parse("idk"), VerdictSeverity::Unknown);
    }
}
