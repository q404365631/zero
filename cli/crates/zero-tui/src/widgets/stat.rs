//! `<Stat>` widget — the honesty primitive.
//!
//! Renders a value plus its freshness and sample size per spec §3.1.
//! Refuses to render when the value's `as_of` exceeds the configured
//! stale threshold without an explicit stale badge.
//!
//! Lint rule (enforced in CI): numeric fields of `EngineState` may
//! only be rendered through this widget. See ADR-010.

use chrono::{DateTime, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use zero_engine_client::{Source, Stat};

use crate::theme::Theme;

/// Render a `Stat<T>` with value, freshness, and optional n.
#[derive(Debug)]
pub struct StatWidget<'a, T: std::fmt::Display> {
    stat: &'a Stat<T>,
    now: DateTime<Utc>,
    stale_after: chrono::Duration,
    theme: Theme,
    show_source: bool,
}

impl<'a, T: std::fmt::Display> StatWidget<'a, T> {
    pub fn new(stat: &'a Stat<T>) -> Self {
        Self {
            stat,
            now: Utc::now(),
            stale_after: chrono::Duration::seconds(5),
            theme: Theme::default(),
            show_source: false,
        }
    }

    #[must_use]
    pub fn stale_after(mut self, d: chrono::Duration) -> Self {
        self.stale_after = d;
        self
    }

    #[must_use]
    pub fn theme(mut self, t: Theme) -> Self {
        self.theme = t;
        self
    }

    #[must_use]
    pub fn show_source(mut self, yes: bool) -> Self {
        self.show_source = yes;
        self
    }
}

impl<T: std::fmt::Display> Widget for StatWidget<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let stale = self.stat.is_stale(self.now, self.stale_after);
        let value_style = if stale {
            Style::default()
                .fg(self.theme.caution)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(self.theme.primary)
        };

        let mut spans: Vec<Span<'_>> = vec![Span::styled(self.stat.value.to_string(), value_style)];

        if let Some(n) = self.stat.n {
            spans.push(Span::styled(
                format!(" n={n}"),
                Style::default().fg(self.theme.metadata),
            ));
        }

        if self.show_source {
            let source = match self.stat.source {
                Source::Http => "http",
                Source::Ws => "ws",
                Source::Mcp => "mcp",
                Source::Derived => "derived",
                Source::Mock => "mock",
            };
            spans.push(Span::styled(
                format!(" [{source}]"),
                Style::default().fg(self.theme.metadata),
            ));
        }

        Line::from(spans).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::StatWidget;
    use chrono::Utc;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use zero_engine_client::{Source, Stat};

    #[test]
    fn renders_value_and_n() {
        let stat: Stat<f64> = Stat::new(58.4, Source::Ws).with_n(312);
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        StatWidget::new(&stat)
            .stale_after(chrono::Duration::seconds(60))
            .render(area, &mut buf);
        let rendered: String = (0..area.width)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect();
        assert!(rendered.contains("58.4"));
        assert!(rendered.contains("n=312"));
    }

    #[test]
    fn stale_badge_triggers_on_age() {
        let stale_ts = Utc::now() - chrono::Duration::seconds(30);
        let stat: Stat<f64> = Stat::new(1.0, Source::Ws).with_as_of(stale_ts);
        assert!(stat.is_stale(Utc::now(), chrono::Duration::seconds(5)));
    }
}
