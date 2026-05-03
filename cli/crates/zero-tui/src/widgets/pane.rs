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
use serde_json::Value;
use zero_engine_client::{EngineState, LiveCockpit};

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

#[derive(Debug)]
pub struct CockpitPane<'a> {
    pub engine: &'a EngineState,
    pub theme: Theme,
}

impl Widget for CockpitPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        clear(area, buf);
        Line::from(vec![Span::styled(
            " live cockpit ",
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD),
        )])
        .render(subrect(area, 0, 1), buf);

        let Some(stat) = &self.engine.live_cockpit else {
            Line::from(vec![Span::styled(
                " (no cockpit packet yet — waiting for /live/cockpit poll)",
                Style::default().fg(self.theme.metadata),
            )])
            .render(subrect(area, 2, 1), buf);
            return;
        };

        let c = &stat.value;
        render_cockpit_header(area, buf, self.theme, c);
        render_cockpit_summary(area, buf, self.theme, c);
        let y = render_cockpit_findings(area, buf, self.theme, c, 13);
        line(
            area,
            buf,
            y.saturating_add(1),
            self.theme,
            " actions",
            "reduce=/pause-entries /kill /flatten-all  resume=/resume-entries",
        );
    }
}

fn render_cockpit_header(area: Rect, buf: &mut Buffer, theme: Theme, c: &LiveCockpit) {
    let header_style = if c.ready && c.risk_increasing_allowed {
        Style::default().fg(theme.primary)
    } else {
        Style::default()
            .fg(theme.alert)
            .add_modifier(Modifier::BOLD)
    };
    Line::from(vec![
        Span::styled(" live_mode=", Style::default().fg(theme.metadata)),
        Span::styled(c.live_mode.as_str(), header_style),
        Span::styled(
            format!(
                "  ready={}  risk_allowed={}  controls_ready={}",
                c.ready, c.risk_increasing_allowed, c.controls_ready
            ),
            Style::default().fg(theme.metadata),
        ),
    ])
    .render(subrect(area, 2, 1), buf);

    line(area, buf, 3, theme, " next", c.next_action.as_str());
    line(
        area,
        buf,
        4,
        theme,
        " operator",
        &format!(
            "handle={} id={} role={} scope={}",
            c.operator_context.handle,
            c.operator_context.operator_id,
            c.operator_context.role,
            c.operator_context.scope
        ),
    );
}

fn render_cockpit_summary(area: Rect, buf: &mut Buffer, theme: Theme, c: &LiveCockpit) {
    let preflight_total = json_u64(&c.preflight.summary, "total");
    let preflight_passed = json_u64(&c.preflight.summary, "passed");
    let preflight_failed = json_u64(&c.preflight.summary, "failed");
    let immune_open = json_u64(&c.immune.summary, "open");
    let immune_blocking = json_u64(&c.immune.summary, "risk_blocking");
    let cert_total = json_u64(&c.certification.summary, "total");
    let cert_passed = json_u64(&c.certification.summary, "passed");
    let timeout = c
        .heartbeat
        .timeout_s
        .map_or_else(|| "n/a".to_string(), |s| s.to_string());

    line(
        area,
        buf,
        6,
        theme,
        " preflight",
        &format!("passed={preflight_passed}/{preflight_total} failed={preflight_failed}"),
    );
    line(
        area,
        buf,
        7,
        theme,
        " immune",
        &format!("open={immune_open} risk_blocking={immune_blocking}"),
    );
    line(
        area,
        buf,
        8,
        theme,
        " reconcile",
        &format!(
            "status={} risk_allowed={} drifts={} - {}",
            c.reconciliation.status,
            c.reconciliation.risk_increasing_allowed,
            c.reconciliation.drifts,
            c.reconciliation.reason
        ),
    );
    line(
        area,
        buf,
        9,
        theme,
        " certification",
        &format!(
            "passed={} live_start_certified={} drills={cert_passed}/{cert_total}",
            c.certification.passed, c.certification.live_start_certified
        ),
    );
    line(
        area,
        buf,
        10,
        theme,
        " heartbeat",
        &format!(
            "configured={} expired={} timeout_s={timeout}",
            c.heartbeat.configured, c.heartbeat.expired
        ),
    );
    line(
        area,
        buf,
        11,
        theme,
        " receipts",
        &format!(
            "total={} accepted={} refused={} exchange_error={}",
            c.live_records.total,
            c.live_records.accepted,
            c.live_records.refused,
            c.live_records.exchange_error
        ),
    );
}

fn render_cockpit_findings(
    area: Rect,
    buf: &mut Buffer,
    theme: Theme,
    c: &LiveCockpit,
    mut y: u16,
) -> u16 {
    for check in c.preflight.failed_checks.iter().take(4) {
        line(
            area,
            buf,
            y,
            theme,
            " preflight",
            &format!("{} {} - {}", check.name, check.status, check.note),
        );
        y += 1;
    }
    for breaker in c.immune.open_breakers.iter().take(4) {
        line(
            area,
            buf,
            y,
            theme,
            " breaker",
            &format!("{} {} - {}", breaker.name, breaker.status, breaker.reason),
        );
        y += 1;
    }
    y
}

fn line(area: Rect, buf: &mut Buffer, y: u16, theme: Theme, label: &str, value: &str) {
    if y >= area.height {
        return;
    }
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(theme.metadata)),
        Span::styled(": ", Style::default().fg(theme.metadata)),
        Span::styled(value.to_string(), Style::default().fg(theme.primary)),
    ])
    .render(subrect(area, y, 1), buf);
}

fn json_u64(map: &std::collections::BTreeMap<String, Value>, key: &str) -> u64 {
    map.get(key).and_then(Value::as_u64).unwrap_or(0)
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
