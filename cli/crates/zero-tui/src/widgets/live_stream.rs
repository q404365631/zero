//! Live-stream pane — a compact tail of the engine's WS push
//! surface. Sourced from [`crate::app::event_ring::EventRing`];
//! rendered as one row per event in chronological order (newest
//! at the bottom so the pane reads like a log).
//!
//! Design choices:
//! - Self-labeling kind column so operators can scan by color
//!   and prefix without waiting for a full decode.
//! - `HH:MM:SS` timestamps only. Dates are visible in the status
//!   bar; repeating them per-row eats horizontal budget without
//!   helping.
//! - Honest empty state (`(no engine events yet — waiting …)`)
//!   when the ring is empty. A blank pane would look calm even
//!   when the subscriber is offline, which is exactly the wrong
//!   signal to send on a trading terminal.
//! - Broadcast-lag markers render as `!! lag · dropped N events`
//!   in alert color; silently losing events after a burst is a
//!   worse failure mode than a loud row.

use chrono::{DateTime, Timelike, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use zero_engine_client::EngineEvent;

use crate::app::event_ring::{EventRing, RingItem};
use crate::theme::Theme;

/// Header height (`" live stream "` row) subtracted from the
/// pane's total area before computing how many event rows fit.
const HEADER_ROWS: u16 = 1;

#[derive(Debug)]
pub struct LiveStreamPane<'a> {
    pub ring: &'a EventRing,
    pub theme: Theme,
}

impl Widget for LiveStreamPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        clear(area, buf);
        if area.height == 0 || area.width == 0 {
            return;
        }

        let header_line = Line::from(vec![
            Span::styled(
                " live stream ",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· {} buffered", self.ring.len()),
                Style::default().fg(self.theme.metadata),
            ),
        ]);
        header_line.render(subrect(area, 0, 1), buf);

        // Usable rows for event lines — pane height minus the header.
        let body_rows = area.height.saturating_sub(HEADER_ROWS);
        if body_rows == 0 {
            return;
        }

        if self.ring.is_empty() {
            Line::from(vec![Span::styled(
                " (no engine events yet — waiting for the subscriber)",
                Style::default().fg(self.theme.metadata),
            )])
            .render(subrect(area, HEADER_ROWS, 1), buf);
            return;
        }

        // Render the last `body_rows` items in chronological
        // order (newest at the bottom of the pane). `tail`
        // already clamps to the ring length so we never overshoot.
        let take = usize::from(body_rows);
        let items: Vec<&RingItem> = self.ring.tail(take).collect();
        for (i, item) in items.iter().enumerate() {
            let y = u16::try_from(usize::from(HEADER_ROWS) + i).unwrap_or(u16::MAX);
            let r = subrect(area, y, 1);
            if r.y >= area.bottom() {
                break;
            }
            Line::from(format_item(item, self.theme)).render(r, buf);
        }
    }
}

/// Format one ring item as a line of `Span`s. Public so tests
/// can assert text + color without threading a fake terminal.
#[must_use]
pub fn format_item(item: &RingItem, theme: Theme) -> Vec<Span<'static>> {
    match item {
        RingItem::Event(e) => format_event(e.ts, &e.event, theme),
        RingItem::Lagged { ts, skipped } => format_lagged(*ts, *skipped, theme),
    }
}

fn format_event(ts: DateTime<Utc>, evt: &EngineEvent, theme: Theme) -> Vec<Span<'static>> {
    let (kind, detail, color) = kind_detail_color(evt, theme);
    vec![
        Span::styled(
            format!(" {}", fmt_hms(ts)),
            Style::default().fg(theme.metadata),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(format!("[{kind:<9}]"), Style::default().fg(color)),
        Span::styled("  ", Style::default()),
        Span::styled(detail, Style::default().fg(theme.primary)),
    ]
}

fn format_lagged(ts: DateTime<Utc>, skipped: u64, theme: Theme) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {}", fmt_hms(ts)),
            Style::default().fg(theme.metadata),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            "[!! lag   ]",
            Style::default()
                .fg(theme.alert)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("broadcast channel dropped {skipped} events"),
            Style::default().fg(theme.alert),
        ),
    ]
}

fn kind_detail_color(
    evt: &EngineEvent,
    theme: Theme,
) -> (&'static str, String, ratatui::style::Color) {
    match evt {
        EngineEvent::Heartbeat(_) => ("heartbeat", "engine alive".into(), theme.muted),
        EngineEvent::Status(s) => {
            let regime = s.regime().unwrap_or("—");
            let conf = s
                .engine_confidence()
                .map_or("—".to_string(), |v| format!("{v:.0}"));
            let eq = s.equity().map_or("—".to_string(), |v| format!("{v:.2}"));
            (
                "status",
                format!("regime={regime}  conf={conf}  eq={eq}"),
                theme.primary,
            )
        }
        EngineEvent::Positions(p) => {
            let n = p.items.len();
            (
                "positions",
                format!("{n} open position{}", if n == 1 { "" } else { "s" }),
                theme.primary,
            )
        }
        EngineEvent::Risk(r) => {
            let halted = r.is_halted();
            let dd = r
                .drawdown_pct
                .map_or("—".to_string(), |v| format!("{v:.2}%"));
            let loss = r
                .daily_loss_pct()
                .map(|v| format!("{v:.2}%"))
                .or_else(|| r.daily_loss_usd.map(|v| format!("${v:.2}")))
                .unwrap_or_else(|| "—".to_string());
            let pnl = r
                .daily_pnl_usd
                .map_or("—".to_string(), |v| format!("{v:+.2}"));
            let line = format!("dd={dd}  daily-loss={loss}  daily-pnl={pnl}");
            // Halted risk is the one WS event that absolutely must
            // grab the operator's eye even inside a fast-scrolling
            // pane; paint its row in alert.
            let color = if halted { theme.alert } else { theme.primary };
            ("risk", line, color)
        }
        EngineEvent::Regime(r) => {
            let name = r.regime.as_deref().unwrap_or("—");
            let conf = r.confidence.map_or("—".to_string(), |v| format!("{v:.2}"));
            ("regime", format!("{name}  conf={conf}"), theme.caution)
        }
        EngineEvent::Unknown { event, .. } => ("unknown", format!("event={event}"), theme.metadata),
    }
}

fn fmt_hms(ts: DateTime<Utc>) -> String {
    format!("{:02}:{:02}:{:02}", ts.hour(), ts.minute(), ts.second())
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use zero_engine_client::models::{Positions, Risk};

    fn theme() -> Theme {
        Theme::phosphor()
    }

    fn ts_at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2030, 1, 2, h, m, s).unwrap()
    }

    fn render_line(item: &RingItem) -> String {
        format_item(item, theme())
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    }

    #[test]
    fn heartbeat_row_shows_hms_and_muted_label() {
        let item = RingItem::Event(super::super::super::app::event_ring::RingEntry {
            ts: ts_at(9, 30, 15),
            event: EngineEvent::Heartbeat(ts_at(9, 30, 15)),
        });
        let out = render_line(&item);
        assert!(out.contains("09:30:15"), "ts: {out}");
        assert!(out.contains("heartbeat"), "kind: {out}");
        assert!(out.contains("engine alive"), "detail: {out}");
    }

    #[test]
    fn risk_row_includes_percentages() {
        let risk = Risk {
            drawdown_pct: Some(1.25),
            daily_loss_usd: Some(5.0),
            peak_equity: Some(1000.0),
            ..Default::default()
        };
        let item = RingItem::Event(super::super::super::app::event_ring::RingEntry {
            ts: ts_at(9, 30, 15),
            event: EngineEvent::Risk(Box::new(risk)),
        });
        let out = render_line(&item);
        assert!(out.contains("dd=1.25%"), "{out}");
        // daily_loss_usd=5 / peak_equity=1000 => 0.50%
        assert!(out.contains("daily-loss=0.50%"), "{out}");
    }

    #[test]
    fn positions_row_pluralizes() {
        let p = Positions::default();
        let item_none = RingItem::Event(super::super::super::app::event_ring::RingEntry {
            ts: ts_at(0, 0, 0),
            event: EngineEvent::Positions(Box::new(p)),
        });
        let out_none = render_line(&item_none);
        assert!(out_none.contains("0 open positions"), "{out_none}");
    }

    #[test]
    fn lagged_row_is_loud() {
        let item = RingItem::Lagged {
            ts: ts_at(12, 0, 0),
            skipped: 42,
        };
        let out = render_line(&item);
        assert!(out.contains("!! lag"), "prefix: {out}");
        assert!(out.contains("42 events"), "count: {out}");
    }

    #[test]
    fn unknown_event_falls_back_to_event_kind_label() {
        let item = RingItem::Event(super::super::super::app::event_ring::RingEntry {
            ts: ts_at(1, 2, 3),
            event: EngineEvent::Unknown {
                event: "scar_fired".into(),
                ts: ts_at(1, 2, 3),
                data: serde_json::Value::default(),
            },
        });
        let out = render_line(&item);
        assert!(out.contains("unknown"), "{out}");
        assert!(out.contains("scar_fired"), "{out}");
    }

    #[test]
    fn pane_renders_empty_state_when_ring_has_nothing() {
        let ring = EventRing::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 6));
        LiveStreamPane {
            ring: &ring,
            theme: theme(),
        }
        .render(Rect::new(0, 0, 80, 6), &mut buf);
        let row1 = row_string(&buf, 1);
        assert!(
            row1.contains("no engine events yet"),
            "honest empty state: {row1}"
        );
    }

    #[test]
    fn pane_renders_last_rows_only_when_ring_exceeds_height() {
        let mut ring = EventRing::with_capacity(20);
        for s in 0..10 {
            ring.push_event(EngineEvent::Heartbeat(ts_at(9, 0, s)));
        }
        // Pane with 1 header + 3 body rows should show the last 3 heartbeats.
        let rect = Rect::new(0, 0, 80, 4);
        let mut buf = Buffer::empty(rect);
        LiveStreamPane {
            ring: &ring,
            theme: theme(),
        }
        .render(rect, &mut buf);
        let body = (1..4).map(|y| row_string(&buf, y)).collect::<Vec<_>>();
        assert!(body[0].contains("09:00:07"), "{body:?}");
        assert!(body[1].contains("09:00:08"), "{body:?}");
        assert!(body[2].contains("09:00:09"), "{body:?}");
    }

    #[test]
    fn zero_height_pane_does_not_panic() {
        let ring = EventRing::new();
        let rect = Rect::new(0, 0, 80, 0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 2));
        LiveStreamPane {
            ring: &ring,
            theme: theme(),
        }
        .render(rect, &mut buf);
    }

    fn row_string(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
    }
}
