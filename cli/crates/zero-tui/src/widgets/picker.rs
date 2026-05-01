//! Slash-command picker widget — small popup above the prompt
//! listing fuzzy-matched commands with their risk badge and
//! summary.
//!
//! Layout convention: the picker occupies the bottom rows of the
//! conversation pane, directly above the prompt. It is *not* a
//! modal — operators can ignore it entirely and press Enter to
//! submit whatever they typed; the picker exists purely as
//! discovery. Tab is the only key that commits a selection into
//! the buffer.
//!
//! Matched characters in each row's name are bolded so the
//! operator can see why a result ranked; unmatched characters
//! render dim. The selected row gets a reversed-video background
//! and a leading chevron (`›`) so screen-reader users can also
//! tell which row is active (a paired ARIA-role pass lands with
//! the `screen-reader` mode addition).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use zero_commands::RiskDirection;

use crate::app::picker::{PICKER_MAX_VISIBLE, SlashMatch, SlashPicker};
use crate::theme::Theme;

/// Rows the picker will actually consume in the layout at the
/// caller's current width. Never more than `PICKER_MAX_VISIBLE`;
/// never more than the number of matches.
#[must_use]
pub fn picker_rows(picker: &SlashPicker) -> u16 {
    let n = picker.matches().len().min(PICKER_MAX_VISIBLE);
    u16::try_from(n).unwrap_or(0)
}

#[derive(Debug)]
pub struct PickerWidget<'a> {
    pub picker: &'a SlashPicker,
    pub theme: Theme,
}

impl Widget for PickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 || !self.picker.is_active() {
            return;
        }
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_char(' ');
            }
        }

        let matches = self.picker.matches();
        let visible = usize::from(area.height).min(matches.len());
        // Window slide: keep the selected row in view. Small list
        // so a naive center-scroll is overkill; if the selected
        // row is past the visible window, slide the window down.
        let sel = self.picker.selected_index();
        let start = if sel < visible { 0 } else { sel + 1 - visible };

        for (i, m) in matches.iter().enumerate().skip(start).take(visible) {
            let visible_row = i - start;
            let y = area.top() + u16::try_from(visible_row).unwrap_or(u16::MAX);
            if y >= area.bottom() {
                break;
            }
            let row_area = Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            };
            render_row(m, i == sel, &self.theme, row_area, buf);
        }
    }
}

fn render_row(m: &SlashMatch, selected: bool, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let base_style = if selected {
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme.primary)
    };
    let dim = Style::default()
        .fg(theme.metadata)
        .add_modifier(Modifier::DIM);
    let bold = base_style.add_modifier(Modifier::BOLD);
    let risk_style = match m.info.risk {
        RiskDirection::Reduces => Style::default().fg(theme.primary),
        RiskDirection::Neutral => dim,
        RiskDirection::Increases => Style::default()
            .fg(theme.alert)
            .add_modifier(Modifier::BOLD),
    };

    let chevron = if selected { "› " } else { "  " };
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(m.info.name.chars().count() + 4);
    spans.push(Span::styled(chevron.to_string(), base_style));
    for (i, c) in m.info.name.chars().enumerate() {
        let styled = if m.matched_chars.contains(&i) {
            bold
        } else {
            base_style
        };
        spans.push(Span::styled(c.to_string(), styled));
    }
    spans.push(Span::styled(
        format!(" [{}] ", risk_label(m.info.risk)),
        risk_style,
    ));
    spans.push(Span::styled(m.info.summary.to_string(), dim));
    Line::from(spans).render(area, buf);
}

const fn risk_label(r: RiskDirection) -> &'static str {
    match r {
        RiskDirection::Reduces => "reduce",
        RiskDirection::Neutral => "read",
        RiskDirection::Increases => "trade",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(picker: &SlashPicker, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).expect("term");
        term.draw(|f| {
            let w = PickerWidget {
                picker,
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
    fn renders_selected_row_with_chevron() {
        let picker = SlashPicker::from_prompt_line("/h").expect("picker");
        let rows = picker_rows(&picker);
        assert!(rows >= 1);
        let lines = render(&picker, 60, rows);
        // First row is selected by default.
        assert!(
            lines[0].starts_with('›'),
            "selected row should lead with chevron: {:?}",
            lines[0]
        );
    }

    #[test]
    fn inactive_picker_renders_nothing_visible() {
        // Build an inactive picker by filtering against a string
        // with no subsequence match.
        let picker = SlashPicker::from_prompt_line("/xyzzyq").expect("picker");
        assert!(!picker.is_active());
        let lines = render(&picker, 60, 3);
        // All cells blank — no chevron, no name.
        for line in &lines {
            assert!(
                line.chars().all(|c| c == ' '),
                "expected all-blank row, got {line:?}"
            );
        }
    }
}
