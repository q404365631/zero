//! Conversation pane — scrollback of log entries with an
//! explicit offset-from-bottom cursor.
//!
//! # Scrollback model
//!
//! `ConversationPane::scroll` is the number of rows the viewport
//! is shifted *up* from the newest entry. A scroll of 0 is the
//! "stuck to bottom" state — new entries appear at the bottom and
//! the oldest-visible entry scrolls up to make room. Any non-zero
//! scroll detaches the viewport; newly appended entries continue
//! to grow the backing log but do not yank the viewport. The
//! input layer re-zeroes the offset on submit so command output
//! always lands in view.
//!
//! The pane clamps scroll to `[0, max_offset]` where `max_offset`
//! is `log.len() - visible_rows`; scrolling past either end is a
//! no-op rather than a panic so a held PageUp does not wrap.
//!
//! # Screen-reader mode
//!
//! When `screen_reader` is set the pane switches to a plainer
//! render path:
//!
//! - timestamps drop their DIM modifier (AT-SPI and NVDA often
//!   skip dimmed text entirely);
//! - entry kind becomes an explicit `[system]` / `[alert]`
//!   prefix instead of relying on color; and
//! - the reversed-video and bold modifiers are removed so a
//!   high-contrast terminal does not double-style the row.
//!
//! Keyboard behavior is unchanged — PageUp/PageDown still scroll
//! whether the mode is on or off.

use chrono::{DateTime, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::log::{ConversationLog, EntryKind};
use crate::theme::Theme;

#[derive(Debug)]
pub struct ConversationPane<'a> {
    pub log: &'a ConversationLog,
    pub theme: Theme,
    /// Rows of scroll *up* from the newest entry. Clamped here at
    /// render time; the input layer can stash arbitrary values
    /// without worrying about log length.
    pub scroll: u16,
    /// Plain-ASCII / explicit-role rendering path.
    pub screen_reader: bool,
    /// When set, timestamps expand from `HH:MM:SS` to `MM-DD
    /// HH:MM:SS` so entries that cross midnight are still
    /// readable without having to check the status bar. Toggled
    /// via `/verbose`.
    pub verbose: bool,
}

impl Widget for ConversationPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = usize::from(area.height);
        if rows == 0 {
            return;
        }
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_char(' ');
            }
        }

        let total = self.log.len();
        if total == 0 {
            return;
        }

        // Clamp scroll so we never drop off the top.
        let max_off = total.saturating_sub(rows);
        let offset = usize::from(self.scroll).min(max_off);
        // Determine the first-visible index: from bottom minus
        // offset, step back by rows.
        let end_exclusive = total - offset;
        let start = end_exclusive.saturating_sub(rows);
        let slice = &self.log.entries()[start..end_exclusive];

        let start_y = area.top();
        for (i, entry) in slice.iter().enumerate() {
            let y = start_y + u16::try_from(i).unwrap_or(u16::MAX);
            if y >= area.bottom() {
                break;
            }
            let row_area = Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            };
            render_entry(
                entry.at,
                entry.kind,
                &entry.text,
                &self.theme,
                EntryOpts {
                    screen_reader: self.screen_reader,
                    verbose: self.verbose,
                },
                row_area,
                buf,
            );
        }

        // "You are scrolled up" cue — a single-cell glyph in the
        // top-right corner. Keeps the operator oriented when they
        // step back through history.
        if offset > 0 && area.width > 0 {
            let x = area.right().saturating_sub(1);
            buf[(x, start_y)].set_char('↑').set_style(
                Style::default()
                    .fg(self.theme.caution)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

/// Per-entry rendering knobs bundled together to keep
/// [`render_entry`] under the 7-arg clippy cap. Adding a new
/// knob should land here rather than growing a longer signature.
#[derive(Debug, Clone, Copy)]
struct EntryOpts {
    screen_reader: bool,
    verbose: bool,
}

fn render_entry(
    at: DateTime<Utc>,
    kind: EntryKind,
    text: &str,
    theme: &Theme,
    opts: EntryOpts,
    area: Rect,
    buf: &mut Buffer,
) {
    let EntryOpts {
        screen_reader,
        verbose,
    } = opts;
    // Verbose mode prepends the month/day. Year is omitted even
    // in verbose because a conversation pane rarely spans months
    // and the extra width would push body text off narrower
    // terminals. Operators who need full dates have the
    // `/sessions` listing already.
    let ts = if verbose {
        at.format("%m-%d %H:%M:%S ").to_string()
    } else {
        at.format("%H:%M:%S ").to_string()
    };
    let ts_style = if screen_reader {
        Style::default().fg(theme.metadata)
    } else {
        Style::default()
            .fg(theme.metadata)
            .add_modifier(Modifier::DIM)
    };
    let ts_span = Span::styled(ts, ts_style);

    // Color is the same in both rendering modes; only the bold
    // modifier on Prompt / Alert rows differs. Screen-reader mode
    // drops the modifier so AT tooling does not see a doubled-up
    // style on top of the `[role]` prefix.
    let base_color = match kind {
        EntryKind::Prompt | EntryKind::Command => theme.primary,
        EntryKind::System => theme.metadata,
        EntryKind::Warn => theme.caution,
        EntryKind::Alert => theme.alert,
    };
    let mut body_style = Style::default().fg(base_color);
    if !screen_reader && matches!(kind, EntryKind::Prompt | EntryKind::Alert) {
        body_style = body_style.add_modifier(Modifier::BOLD);
    }

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(3);
    spans.push(ts_span);
    if screen_reader {
        spans.push(Span::styled(
            format!("{} ", role_prefix(kind)),
            Style::default().fg(theme.metadata),
        ));
    }
    spans.push(Span::styled(text.to_string(), body_style));
    Line::from(spans).render(area, buf);
}

const fn role_prefix(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Prompt => "[you]",
        EntryKind::System => "[system]",
        EntryKind::Command => "[command]",
        EntryKind::Warn => "[warn]",
        EntryKind::Alert => "[alert]",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::log::LogEntry;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn mk_log(rows: usize) -> ConversationLog {
        let mut log = ConversationLog::with_capacity(0);
        for i in 0..rows {
            log.push(LogEntry::new(EntryKind::System, format!("row-{i:02}")));
        }
        log
    }

    fn render(log: &ConversationLog, scroll: u16, screen_reader: bool) -> Vec<String> {
        render_v(log, scroll, screen_reader, false)
    }

    fn render_v(
        log: &ConversationLog,
        scroll: u16,
        screen_reader: bool,
        verbose: bool,
    ) -> Vec<String> {
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).expect("term");
        term.draw(|f| {
            let w = ConversationPane {
                log,
                theme: Theme::default(),
                scroll,
                screen_reader,
                verbose,
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

    #[test]
    fn scroll_zero_sticks_to_bottom_newest_rows_visible() {
        let log = mk_log(10);
        let rendered = render(&log, 0, false);
        // 4 visible rows → rows 06..09.
        assert!(rendered[3].ends_with("row-09"), "got {:?}", rendered[3]);
        assert!(rendered[0].ends_with("row-06"), "got {:?}", rendered[0]);
    }

    #[test]
    fn scroll_shifts_viewport_and_shows_up_arrow_cue() {
        let log = mk_log(10);
        let rendered = render(&log, 3, false);
        // With 4 visible rows and scroll=3, visible slice ends
        // at index 10 - 3 = 7 → rows 03..06.
        assert!(rendered[3].contains("row-06"), "got {:?}", rendered[3]);
        assert!(rendered[0].contains("row-03"), "got {:?}", rendered[0]);
        assert!(
            rendered[0].contains('↑'),
            "scrolled-up cue missing: {:?}",
            rendered[0]
        );
    }

    #[test]
    fn scroll_past_top_clamps_without_panicking() {
        let log = mk_log(5);
        // width 4 rows, 5 entries, scroll way past top → clamp.
        let rendered = render(&log, 1_000, false);
        assert!(rendered[0].contains("row-00"), "got {:?}", rendered[0]);
    }

    #[test]
    fn screen_reader_mode_prefixes_role_label() {
        let mut log = ConversationLog::with_capacity(0);
        log.push(LogEntry::new(EntryKind::Alert, "kill-switch tripped"));
        let rendered = render(&log, 0, true);
        assert!(
            rendered[0].contains("[alert]"),
            "screen-reader mode must emit an explicit role prefix; got {:?}",
            rendered[0]
        );
    }

    #[test]
    fn verbose_mode_prepends_month_day_to_timestamp() {
        // Default rendering leads with `HH:MM:SS `; verbose
        // mode leads with `MM-DD HH:MM:SS `. Assert both via
        // a regex-free substring check: look for a `-`
        // separator in the first 6 columns of the verbose
        // render (the `MM-DD` chunk) that is absent in the
        // default render.
        let mut log = ConversationLog::with_capacity(0);
        log.push(LogEntry::new(EntryKind::System, "hello"));
        let default = render(&log, 0, false);
        let verbose = render_v(&log, 0, false, true);
        let default_prefix = &default[0][..default[0].len().min(6)];
        let verbose_prefix = &verbose[0][..verbose[0].len().min(6)];
        assert!(
            !default_prefix.contains('-'),
            "default render should not have date: prefix={default_prefix:?}"
        );
        assert!(
            verbose_prefix.contains('-'),
            "verbose render must prepend a date: prefix={verbose_prefix:?}"
        );
    }

    #[test]
    fn default_mode_omits_role_prefix() {
        let mut log = ConversationLog::with_capacity(0);
        log.push(LogEntry::new(EntryKind::Alert, "kill-switch tripped"));
        let rendered = render(&log, 0, false);
        assert!(
            !rendered[0].contains("[alert]"),
            "default render must not emit role prefix; got {:?}",
            rendered[0]
        );
    }
}
