//! Stub panes for Positions, Decisions, Heat.
//!
//! M1 ships the shell with minimal per-mode content so the operator
//! can switch modes, see live engine state for the ones we have
//! (positions, risk), and know the others are on the roadmap.
//! Real dashboards land in subsequent milestones.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use zero_engine_client::EngineState;

use crate::theme::Theme;
use crate::widgets::position_row::PositionRow;

#[derive(Debug)]
pub struct PositionsPane<'a> {
    pub engine: &'a EngineState,
    pub theme: Theme,
}

impl Widget for PositionsPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        clear(area, buf);
        let header = Line::from(vec![Span::styled(
            " positions ",
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )]);
        header.render(subrect(area, 0, 1), buf);

        let Some(stat) = &self.engine.positions else {
            Line::from(vec![Span::styled(
                " (no positions seen — waiting for engine)",
                Style::default().fg(self.theme.metadata),
            )])
            .render(subrect(area, 2, 1), buf);
            return;
        };

        let pos = &stat.value;
        if pos.items.is_empty() {
            Line::from(vec![Span::styled(
                " (flat — no open positions)",
                Style::default().fg(self.theme.metadata),
            )])
            .render(subrect(area, 2, 1), buf);
            return;
        }

        // No column header — every cell in `PositionRow` is
        // self-labeling (`size=…`, `entry=…`, …), so a header
        // would be visually redundant.
        for (i, p) in pos.items.iter().enumerate() {
            let y = u16::try_from(2 + i).unwrap_or(u16::MAX);
            let r = subrect(area, y, 1);
            if r.y >= area.bottom() {
                break;
            }
            PositionRow {
                position: p,
                theme: self.theme,
            }
            .render(r, buf);
        }
    }
}

#[derive(Debug)]
pub struct DecisionsPane {
    pub theme: Theme,
}

impl Widget for DecisionsPane {
    fn render(self, area: Rect, buf: &mut Buffer) {
        clear(area, buf);
        Line::from(vec![Span::styled(
            " decisions ",
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )])
        .render(subrect(area, 0, 1), buf);

        Line::from(vec![Span::styled(
            " (decisions stream lands with `/decisions` command dispatch)",
            Style::default().fg(self.theme.metadata),
        )])
        .render(subrect(area, 2, 1), buf);
    }
}

#[derive(Debug)]
pub struct HeatPane<'a> {
    pub engine: &'a EngineState,
    pub theme: Theme,
}

impl Widget for HeatPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        clear(area, buf);
        Line::from(vec![Span::styled(
            " heat ",
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )])
        .render(subrect(area, 0, 1), buf);

        let body: Vec<Span<'_>> = if let Some(risk) = &self.engine.risk {
            let r = &risk.value;
            let halted_style = if r.is_halted() {
                Style::default()
                    .fg(self.theme.alert)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.primary)
            };
            let halted = if r.is_halted() { "HALTED" } else { "OK" };
            vec![
                Span::styled(" risk: ", Style::default().fg(self.theme.metadata)),
                Span::styled(halted, halted_style),
                Span::styled(
                    {
                        let dd = r.drawdown_pct.map_or("—".into(), |v| format!("{v:.1}%"));
                        let loss = r
                            .daily_loss_pct()
                            .map(|v| format!("{v:.1}%"))
                            .or_else(|| r.daily_loss_usd.map(|v| format!("${v:.2}")))
                            .unwrap_or_else(|| "—".into());
                        let open = r.open_count.map_or("—".into(), |n| n.to_string());
                        format!("  dd:{dd}  daily-loss:{loss}  open:{open}")
                    },
                    Style::default().fg(self.theme.metadata),
                ),
            ]
        } else {
            vec![Span::styled(
                " (no risk snapshot yet — waiting for engine)",
                Style::default().fg(self.theme.metadata),
            )]
        };

        Line::from(body).render(subrect(area, 2, 1), buf);
    }
}

fn clear(area: Rect, buf: &mut Buffer) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_char(' ');
        }
    }
}

fn subrect(area: Rect, y_offset: u16, height: u16) -> Rect {
    Rect {
        x: area.x,
        y: area
            .y
            .saturating_add(y_offset)
            .min(area.bottom().saturating_sub(1)),
        width: area.width,
        height: height.min(area.height.saturating_sub(y_offset)),
    }
}
