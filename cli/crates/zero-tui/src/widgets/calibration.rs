//! Calibration bar — a single-row gauge showing how closely the
//! engine's stated confidence tracks realized outcome.
//!
//! The model: over the last N graded decisions, average the
//! engine's stated confidence (`predicted`) and the fraction of
//! those decisions whose outcome matched the engine's verdict
//! (`observed`). A well-calibrated engine has
//! `|predicted - observed| → 0`.
//!
//! # Layout (width ≥ 40)
//!
//! ```text
//!  calib  [■■■■■■■■■■□□□□□□□□□□]  pred 72% / obs 68%  n=134
//! ```
//!
//! - The bar is always the fixed-width (`BAR_CELLS`) cell gauge;
//!   shading shows the **observed** rate so the eye anchors on
//!   what actually happened.
//! - Text to the right shows both numbers explicitly. We never
//!   show only one — the whole point of the widget is the gap.
//! - `n=` is the decision count contributing to this figure; the
//!   operator uses it to gauge statistical weight.
//! - Color tracks the **gap**, not either side:
//!   - `|pred - obs| ≤ 5pp` → primary (well-calibrated).
//!   - `|pred - obs| ≤ 15pp` → caution.
//!   - `> 15pp` → alert.
//!
//! # Honest "insufficient data" state
//!
//! Below `MIN_SAMPLES` the widget renders the low-contrast
//! notice:
//!
//! ```text
//!  calib  (insufficient data — need ≥30 graded decisions, have 12)
//! ```
//!
//! No gauge is drawn. Showing a bar with a wobbling two-sample
//! average would be confidently wrong, which is exactly the
//! failure mode the widget is built to prevent.
//!
//! # Width guard
//!
//! When `area.width < 30` only the label and the count render.
//! Drawing a sub-10-cell gauge gives no usable information and
//! invites misreading.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::theme::Theme;

/// Minimum graded decisions before we show a bar. Below this,
/// the honest "insufficient data" state renders instead.
pub const MIN_SAMPLES: usize = 30;

/// Fixed cell-width of the bar portion. Chosen so each cell is
/// exactly 5 percentage points.
pub const BAR_CELLS: usize = 20;

/// Observed/predicted calibration sample, as surfaced by a
/// future `/calibration` or `/evaluate --report` engine call.
/// We keep the shape minimal here so the widget can be unit
/// tested without pulling in the full engine client model.
#[derive(Debug, Clone, Copy, Default)]
pub struct CalibrationSample {
    pub predicted: f64,
    pub observed: f64,
    pub n_samples: usize,
}

#[derive(Debug)]
pub struct CalibrationBar {
    pub sample: Option<CalibrationSample>,
    pub theme: Theme,
}

impl Widget for CalibrationBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let row = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };

        let Some(sample) = self.sample else {
            Line::from(vec![
                Span::styled(" calib  ", Style::default().fg(self.theme.primary)),
                Span::styled(
                    "(no calibration data yet — engine has not reported a sample)",
                    Style::default().fg(self.theme.metadata),
                ),
            ])
            .render(row, buf);
            return;
        };

        if sample.n_samples < MIN_SAMPLES {
            Line::from(vec![
                Span::styled(" calib  ", Style::default().fg(self.theme.primary)),
                Span::styled(
                    format!(
                        "(insufficient data — need ≥{MIN_SAMPLES} graded decisions, have {})",
                        sample.n_samples
                    ),
                    Style::default().fg(self.theme.metadata),
                ),
            ])
            .render(row, buf);
            return;
        }

        let pred = clamp_unit(sample.predicted);
        let obs = clamp_unit(sample.observed);
        let gap = (pred - obs).abs();

        let gap_style = if gap <= 0.05 {
            Style::default().fg(self.theme.primary)
        } else if gap <= 0.15 {
            Style::default().fg(self.theme.caution)
        } else {
            Style::default()
                .fg(self.theme.alert)
                .add_modifier(Modifier::BOLD)
        };

        let mut spans = vec![Span::styled(
            " calib  ",
            Style::default().fg(self.theme.primary),
        )];

        if usize::from(area.width) >= 30 {
            let bar = render_bar(obs, pred);
            spans.push(Span::styled("[", Style::default().fg(self.theme.metadata)));
            spans.push(Span::styled(bar, gap_style));
            spans.push(Span::styled("] ", Style::default().fg(self.theme.metadata)));
        }

        spans.push(Span::styled(
            format!(" pred {}% / obs {}%", pct(pred), pct(obs)),
            gap_style,
        ));
        spans.push(Span::styled(
            format!("  n={}", sample.n_samples),
            Style::default().fg(self.theme.metadata),
        ));

        Line::from(spans).render(row, buf);
    }
}

fn clamp_unit(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn pct(v: f64) -> i32 {
    // Bounded in `[0, 100]` after `clamp_unit` — the cast cannot
    // truncate or wrap.
    #[allow(clippy::cast_possible_truncation)]
    let p = (clamp_unit(v) * 100.0).round() as i32;
    p
}

/// Map a fractional `[0.0, 1.0]` rate to a cell index in the
/// fixed `BAR_CELLS`-wide gauge. The cast is bounded (`BAR_CELLS`
/// fits well within the `f64` mantissa and the rounded product
/// is in `[0, BAR_CELLS]`), but we still clamp with `.min()` as
/// a last-line defense against a denormal-valued input.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn cells_for(rate: f64) -> usize {
    let scaled = (rate * BAR_CELLS as f64).round();
    let as_usize = if scaled.is_finite() && scaled >= 0.0 {
        scaled as usize
    } else {
        0
    };
    as_usize.min(BAR_CELLS)
}

/// Render the bar: fill up to `observed`, then mark `predicted`
/// with a distinguishable glyph if it falls in a different cell.
/// When the two land in the same cell, the observed glyph wins
/// (we don't double-draw).
fn render_bar(observed: f64, predicted: f64) -> String {
    let obs_cells = cells_for(observed);
    let pred_cells = cells_for(predicted);
    let mut s = String::with_capacity(BAR_CELLS);
    for i in 0..BAR_CELLS {
        let filled = i < obs_cells;
        let is_pred_marker = pred_cells > 0 && i + 1 == pred_cells && pred_cells != obs_cells;
        s.push(match (filled, is_pred_marker) {
            // Observed fill covers the predicted cell — drop the
            // marker so the bar reads cleanly.
            (true, _) => '■',
            (false, true) => '│',
            (false, false) => '□',
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(sample: Option<CalibrationSample>, width: u16) -> String {
        let backend = TestBackend::new(width, 1);
        let mut term = Terminal::new(backend).expect("term");
        term.draw(|f| {
            let w = CalibrationBar {
                sample,
                theme: Theme::default(),
            };
            f.render_widget(w, f.area());
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn none_sample_renders_no_data_state() {
        let line = render(None, 80);
        assert!(line.contains("calib"));
        assert!(line.contains("no calibration data"));
        assert!(!line.contains("pred"), "must not show fake numbers");
        assert!(!line.contains('■'), "must not draw fake bar");
    }

    #[test]
    fn below_min_samples_renders_insufficient_data_state() {
        let sample = CalibrationSample {
            predicted: 0.7,
            observed: 0.65,
            n_samples: 12,
        };
        let line = render(Some(sample), 80);
        assert!(line.contains("insufficient data"));
        assert!(line.contains("have 12"));
        assert!(!line.contains('■'), "bar must not render below MIN_SAMPLES");
    }

    #[test]
    fn above_min_samples_renders_bar_and_numbers() {
        let sample = CalibrationSample {
            predicted: 0.72,
            observed: 0.68,
            n_samples: 134,
        };
        let line = render(Some(sample), 80);
        assert!(line.contains('■'), "expected filled bar cells: {line:?}");
        assert!(line.contains("pred 72%"), "pred missing: {line:?}");
        assert!(line.contains("obs 68%"), "obs missing: {line:?}");
        assert!(line.contains("n=134"), "n missing: {line:?}");
    }

    #[test]
    fn narrow_width_drops_bar_but_keeps_numbers() {
        let sample = CalibrationSample {
            predicted: 0.5,
            observed: 0.5,
            n_samples: 100,
        };
        let line = render(Some(sample), 29);
        assert!(
            !line.contains('■'),
            "bar should not render at width<30: {line:?}"
        );
        assert!(line.contains("pred 50%"), "pred still required: {line:?}");
    }

    #[test]
    fn pct_clamps_out_of_range() {
        assert_eq!(pct(-0.1), 0);
        assert_eq!(pct(1.5), 100);
        assert_eq!(pct(0.5), 50);
    }

    #[test]
    fn bar_observed_cells_match_rounded_fraction() {
        let s = render_bar(0.5, 0.5);
        let filled = s.chars().filter(|c| *c == '■').count();
        assert_eq!(filled, 10, "50% observed should fill 10/{BAR_CELLS} cells");
    }

    #[test]
    fn bar_predicted_marker_visible_when_gap_nonzero() {
        let s = render_bar(0.5, 0.8);
        // Observed fills 10 cells, predicted marker lives in cell 16.
        let filled = s.chars().filter(|c| *c == '■').count();
        let markers = s.chars().filter(|c| *c == '│').count();
        assert_eq!(filled, 10);
        assert_eq!(markers, 1, "predicted marker must render when > observed");
    }
}
