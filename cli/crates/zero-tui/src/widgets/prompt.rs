//! Prompt widget — renders the [`PromptBuffer`] as N rows.
//!
//! The widget paints the leading `> ` cue on the first row and a
//! continuation cue (`. `) on every subsequent row, matching the
//! convention most readers know from REPLs. Empty trailing rows
//! still get the continuation cue so the operator sees that
//! `Shift+Enter` actually opened a new line.
//!
//! Cursor placement is the caller's responsibility (ratatui
//! requires an explicit `Frame::set_cursor_position`). The widget
//! exposes [`PromptWidget::cursor_position`] for that purpose.
//!
//! Styling is uniform across rows — no syntax highlighting. The
//! widget intentionally does not inspect buffer contents, so
//! command-name colorization (if added later) lives in a
//! companion overlay rather than inside the editor.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::prompt::PromptBuffer;
use crate::theme::Theme;

/// Leading cue on the first prompt row.
pub const PROMPT_CUE: &str = "> ";
/// Continuation cue on subsequent rows of a multi-line prompt.
pub const PROMPT_CONTINUATION: &str = ". ";

/// Cue width in columns. Both cues are 2 ASCII chars; we hard-code
/// the constant to avoid a `chars().count()` per render.
const CUE_WIDTH: u16 = 2;

#[derive(Debug)]
pub struct PromptWidget<'a> {
    pub prompt: &'a PromptBuffer,
    pub theme: Theme,
}

impl PromptWidget<'_> {
    /// Cursor position relative to the widget's `area` origin.
    /// Returns `(col, row)` so it composes naturally with
    /// `(area.x + col, area.y + row)` at the call site.
    #[must_use]
    pub fn cursor_position(&self) -> (u16, u16) {
        let col = CUE_WIDTH.saturating_add(self.prompt.cursor_column());
        let row = u16::try_from(self.prompt.cursor_row()).unwrap_or(u16::MAX);
        (col, row)
    }

    /// Backwards-compatible single-int cursor (column on the
    /// active row). Retained because some snapshot tests still
    /// reference it.
    #[must_use]
    pub fn cursor_column(&self) -> u16 {
        self.cursor_position().0
    }
}

impl Widget for PromptWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        // Clear the area before painting so a shrinking prompt
        // doesn't leave ghost characters from the previous frame.
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_char(' ');
            }
        }

        let cue_style = Style::default().fg(self.theme.primary);
        let body_style = Style::default().fg(self.theme.primary);
        let cont_style = Style::default().fg(self.theme.metadata);

        let visible_rows = usize::from(area.height);
        for visible_row in 0..visible_rows {
            let buf_row = visible_row;
            let line_chars = self.prompt.line(buf_row);
            // Only the rows the buffer actually owns get content.
            // Beyond that, leave the row blank — `area.height` is
            // chosen by the layout to match buffer height, so this
            // branch is mostly defensive.
            let Some(chars) = line_chars else {
                break;
            };
            let body: String = chars.iter().collect();
            let cue = if buf_row == 0 {
                PROMPT_CUE
            } else {
                PROMPT_CONTINUATION
            };
            let cue_span = Span::styled(cue, if buf_row == 0 { cue_style } else { cont_style });
            let body_span = Span::styled(body, body_style);
            let row_area = Rect {
                x: area.x,
                y: area.y + u16::try_from(visible_row).unwrap_or(u16::MAX),
                width: area.width,
                height: 1,
            };
            Line::from(vec![cue_span, body_span]).render(row_area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(prompt: &PromptBuffer, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).expect("terminal");
        term.draw(|f| {
            let w = PromptWidget {
                prompt,
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
            })
            .collect()
    }

    #[test]
    fn single_row_prompt_uses_primary_cue() {
        let mut p = PromptBuffer::new();
        for c in "/help".chars() {
            p.insert(c);
        }
        let lines = render(&p, 20, 1);
        assert_eq!(lines[0].trim_end(), "> /help");
    }

    #[test]
    fn second_row_uses_continuation_cue() {
        let mut p = PromptBuffer::new();
        for c in "abc".chars() {
            p.insert(c);
        }
        p.insert_newline();
        for c in "def".chars() {
            p.insert(c);
        }
        let lines = render(&p, 20, 2);
        assert_eq!(lines[0].trim_end(), "> abc");
        assert_eq!(lines[1].trim_end(), ". def");
    }

    #[test]
    fn cursor_position_accounts_for_cue_and_row() {
        let mut p = PromptBuffer::new();
        for c in "ab".chars() {
            p.insert(c);
        }
        p.insert_newline();
        for c in "cdef".chars() {
            p.insert(c);
        }
        let w = PromptWidget {
            prompt: &p,
            theme: Theme::default(),
        };
        // Cursor is at row=1 col=4 → screen col = 2 + 4 = 6.
        assert_eq!(w.cursor_position(), (6, 1));
    }
}
