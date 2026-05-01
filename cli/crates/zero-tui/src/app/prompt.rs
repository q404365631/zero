//! Prompt buffer — backing state for the prompt widget.
//!
//! The buffer is **multi-line**: `Shift+Enter` inserts a newline,
//! plain `Enter` submits the joined text. Cursor is `(row, col)`,
//! both 0-based char positions (not bytes), so multi-byte input
//! does not corrupt navigation.
//!
//! The buffer also owns a [`PromptHistory`] — a small ring of
//! previously submitted lines that the operator can recall with
//! `Up`/`Down`. Recalling preserves the in-flight draft: stepping
//! past the newest history entry restores whatever the operator
//! was typing before they started navigating.
//!
//! Design constraints (kept narrow on purpose):
//!
//! - No syntax highlighting or word-wrap. ratatui's render path
//!   does the visible-width math; if a line is wider than the
//!   prompt area, it crops at the right edge. The fix for
//!   long-line ergonomics is "use `Shift+Enter`", not a wrapping
//!   engine that has to track logical vs visual position.
//! - Char-position cursor only. ratatui currently expects ASCII
//!   columns; wide chars in command text are rare. The day we
//!   need grapheme cluster math, this module is the place to
//!   add it — every other widget reads `cursor_row()` /
//!   `cursor_column()` and the joined `as_string()`.

use std::collections::VecDeque;

/// History ring size — large enough for a long live trading
/// session, small enough that recall is constant-time even when
/// every keystroke walks the buffer.
pub const HISTORY_CAP: usize = 256;

/// Maximum prompt rows the buffer will accept. The render layer
/// caps the *visible* prompt height separately (see
/// `app::render`); this constant just guards against a runaway
/// `Shift+Enter` repeat eating memory in pathological cases.
pub const MAX_LINES: usize = 64;

/// History of submitted prompt lines.
///
/// `cursor` semantics:
/// - `None` → operator is at the live draft (newest).
/// - `Some(i)` → operator is recalling `entries[i]`.
///
/// Stepping past index 0 (Up at oldest) clamps; stepping past
/// the live draft (Down at newest) leaves `cursor = None` and
/// restores the saved draft.
#[derive(Debug, Default, Clone)]
pub struct PromptHistory {
    entries: VecDeque<String>,
    cap: usize,
    cursor: Option<usize>,
}

impl PromptHistory {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(HISTORY_CAP)
    }

    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            cap,
            cursor: None,
        }
    }

    /// Append a submitted line. Empty lines are ignored. Two
    /// consecutive identical entries are deduped (bash-style) so
    /// hammering Enter doesn't bury everything else under copies
    /// of `/status`.
    pub fn push(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        if self.entries.back().is_some_and(|last| last == line) {
            self.cursor = None;
            return;
        }
        if self.cap > 0 && self.entries.len() >= self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(line.to_string());
        self.cursor = None;
    }

    /// Cursor reset — call after a successful submission so the
    /// next Up starts from the newest entry again.
    pub fn reset_cursor(&mut self) {
        self.cursor = None;
    }

    /// Up arrow — step toward older history. Returns the recalled
    /// entry, or `None` if history is empty / already at oldest.
    pub fn recall_prev(&mut self) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let next = match self.cursor {
            None => self.entries.len().saturating_sub(1),
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.cursor = Some(next);
        self.entries.get(next).map(String::as_str)
    }

    /// Down arrow — step toward newer history. Returns the
    /// recalled entry, or `None` to signal "you've stepped past
    /// the newest entry; restore the live draft" (the buffer
    /// owns the saved draft and handles that branch).
    pub fn recall_next(&mut self) -> Option<&str> {
        let cur = self.cursor?;
        if cur + 1 >= self.entries.len() {
            self.cursor = None;
            return None;
        }
        let next = cur + 1;
        self.cursor = Some(next);
        self.entries.get(next).map(String::as_str)
    }

    /// True when the operator is currently navigating history
    /// (not editing the live draft).
    #[must_use]
    pub const fn is_recalling(&self) -> bool {
        self.cursor.is_some()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug)]
pub struct PromptBuffer {
    lines: Vec<Vec<char>>,
    row: usize,
    col: usize,
    /// Sticky column intent: when moving up/down across short
    /// lines, the cursor remembers the column the operator wanted
    /// even if a passing line was too short. Reset on any
    /// horizontal motion or insert.
    desired_col: Option<usize>,
    history: PromptHistory,
    /// Snapshot of the live draft taken at the moment the
    /// operator began recalling history. Restored when they step
    /// past the newest entry. `None` between recalls.
    saved_draft: Option<Vec<Vec<char>>>,
}

impl Default for PromptBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptBuffer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            lines: vec![Vec::new()],
            row: 0,
            col: 0,
            desired_col: None,
            history: PromptHistory::new(),
            saved_draft: None,
        }
    }

    // ─── Read accessors ────────────────────────────────────────

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(Vec::is_empty)
    }

    /// Joined string with embedded `\n` between rows. Allocates.
    #[must_use]
    pub fn as_string(&self) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            for c in line {
                out.push(*c);
            }
        }
        out
    }

    /// Number of rows in the current buffer. Always ≥ 1.
    #[must_use]
    pub fn height(&self) -> usize {
        self.lines.len()
    }

    /// Current cursor row (0-based).
    #[must_use]
    pub const fn cursor_row(&self) -> usize {
        self.row
    }

    /// Current cursor column on the active row.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.col
    }

    /// Same as [`cursor`] but as a `u16` for ratatui's column API.
    #[must_use]
    pub fn cursor_column(&self) -> u16 {
        u16::try_from(self.col).unwrap_or(u16::MAX)
    }

    /// Read a row's contents — used by the widget for rendering.
    #[must_use]
    pub fn line(&self, row: usize) -> Option<&[char]> {
        self.lines.get(row).map(Vec::as_slice)
    }

    /// Borrowed access to the history ring (read-only).
    #[must_use]
    pub const fn history(&self) -> &PromptHistory {
        &self.history
    }

    // ─── Edits ─────────────────────────────────────────────────

    /// Insert a literal character at the cursor. Discards the
    /// history-recall flag — once you start typing on a recalled
    /// entry it's part of your draft.
    pub fn insert(&mut self, c: char) {
        self.history.reset_cursor();
        self.saved_draft = None;
        self.desired_col = None;
        let line = &mut self.lines[self.row];
        line.insert(self.col, c);
        self.col += 1;
    }

    /// `Shift+Enter` — break the current line at the cursor and
    /// move down. Capped at [`MAX_LINES`]; further newlines are
    /// silently dropped so a stuck Repeat key cannot grow the
    /// prompt indefinitely.
    pub fn insert_newline(&mut self) {
        if self.lines.len() >= MAX_LINES {
            return;
        }
        self.history.reset_cursor();
        self.saved_draft = None;
        self.desired_col = None;
        let tail = self.lines[self.row].split_off(self.col);
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.col = 0;
    }

    /// Backspace — delete the char to the left of the cursor; if
    /// at column 0 of a non-first row, merge with the previous
    /// row instead.
    pub fn backspace(&mut self) {
        self.desired_col = None;
        if self.col > 0 {
            self.col -= 1;
            self.lines[self.row].remove(self.col);
            return;
        }
        if self.row == 0 {
            return;
        }
        // Merge current line into the one above.
        let tail = std::mem::take(&mut self.lines[self.row]);
        self.lines.remove(self.row);
        self.row -= 1;
        self.col = self.lines[self.row].len();
        self.lines[self.row].extend(tail);
    }

    /// Delete — char at the cursor, or if at end of line, splice
    /// the next line up.
    pub fn delete(&mut self) {
        self.desired_col = None;
        let line_len = self.lines[self.row].len();
        if self.col < line_len {
            self.lines[self.row].remove(self.col);
            return;
        }
        if self.row + 1 >= self.lines.len() {
            return;
        }
        let next = self.lines.remove(self.row + 1);
        self.lines[self.row].extend(next);
    }

    // ─── Movement ──────────────────────────────────────────────

    pub fn move_left(&mut self) {
        self.desired_col = None;
        if self.col > 0 {
            self.col -= 1;
            return;
        }
        if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].len();
        }
    }

    pub fn move_right(&mut self) {
        self.desired_col = None;
        if self.col < self.lines[self.row].len() {
            self.col += 1;
            return;
        }
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn move_home(&mut self) {
        self.desired_col = None;
        self.col = 0;
    }

    pub fn move_end(&mut self) {
        self.desired_col = None;
        self.col = self.lines[self.row].len();
    }

    /// Up arrow — *intra-buffer* movement. The caller (input.rs)
    /// is responsible for routing Up to history recall when the
    /// cursor is at the first row; this method only moves up
    /// inside a multi-line buffer.
    pub fn move_up(&mut self) {
        if self.row == 0 {
            return;
        }
        let want = self.desired_col.unwrap_or(self.col);
        self.row -= 1;
        self.col = want.min(self.lines[self.row].len());
        self.desired_col = Some(want);
    }

    pub fn move_down(&mut self) {
        if self.row + 1 >= self.lines.len() {
            return;
        }
        let want = self.desired_col.unwrap_or(self.col);
        self.row += 1;
        self.col = want.min(self.lines[self.row].len());
        self.desired_col = Some(want);
    }

    /// True when the cursor is on the *visual* top row of the
    /// buffer. Input router uses this to decide whether Up should
    /// recall history or move within the buffer.
    #[must_use]
    pub const fn cursor_on_first_row(&self) -> bool {
        self.row == 0
    }

    /// True when the cursor is on the *visual* last row.
    #[must_use]
    pub fn cursor_on_last_row(&self) -> bool {
        self.row + 1 == self.lines.len()
    }

    // ─── History ───────────────────────────────────────────────

    /// Recall the previous (older) history entry. Stashes the
    /// live draft on first use so a subsequent `recall_next`
    /// past the newest can restore it.
    pub fn recall_prev(&mut self) {
        if !self.history.is_recalling() {
            self.saved_draft = Some(self.lines.clone());
        }
        if let Some(line) = self.history.recall_prev().map(str::to_string) {
            self.replace_with(&line);
        }
    }

    /// Recall the next (newer) history entry, or restore the
    /// saved draft when stepping past the newest entry.
    pub fn recall_next(&mut self) {
        if !self.history.is_recalling() {
            return;
        }
        match self.history.recall_next() {
            Some(line) => {
                let line = line.to_string();
                self.replace_with(&line);
            }
            None => {
                // Past newest → restore the in-flight draft.
                if let Some(draft) = self.saved_draft.take() {
                    self.lines = draft;
                    self.move_to_buffer_end();
                }
            }
        }
    }

    fn replace_with(&mut self, s: &str) {
        self.lines = s.split('\n').map(|l| l.chars().collect()).collect();
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        self.move_to_buffer_end();
    }

    fn move_to_buffer_end(&mut self) {
        self.row = self.lines.len() - 1;
        self.col = self.lines[self.row].len();
        self.desired_col = None;
    }

    /// Submit the buffer. Returns `None` for an all-whitespace
    /// buffer so the caller can short-circuit. Also pushes the
    /// joined text onto history and resets the recall cursor.
    pub fn take(&mut self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let s = self.as_string();
        if s.trim().is_empty() {
            self.clear();
            return None;
        }
        self.history.push(&s);
        self.clear();
        Some(s)
    }

    /// Discard the buffer and reset the cursor, but keep history
    /// intact. Used by Esc.
    pub fn clear(&mut self) {
        self.lines = vec![Vec::new()];
        self.row = 0;
        self.col = 0;
        self.desired_col = None;
        self.saved_draft = None;
        self.history.reset_cursor();
    }

    /// Replace the whole buffer with a literal value (used by
    /// the slash-command picker on Tab-complete). Cursor lands at
    /// end-of-buffer.
    pub fn replace_all(&mut self, s: &str) {
        self.replace_with(s);
        self.history.reset_cursor();
        self.saved_draft = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{PromptBuffer, PromptHistory};

    fn type_str(p: &mut PromptBuffer, s: &str) {
        for c in s.chars() {
            p.insert(c);
        }
    }

    #[test]
    fn insert_and_backspace() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "hello");
        assert_eq!(p.as_string(), "hello");
        assert_eq!(p.cursor(), 5);
        p.backspace();
        p.backspace();
        assert_eq!(p.as_string(), "hel");
        assert_eq!(p.cursor(), 3);
    }

    #[test]
    fn move_and_delete_midway() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "foobar");
        p.move_home();
        p.move_right();
        p.move_right();
        p.delete();
        assert_eq!(p.as_string(), "fobar");
    }

    #[test]
    fn take_clears_and_pushes_history() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "/status");
        assert_eq!(p.take().as_deref(), Some("/status"));
        assert!(p.is_empty());
        assert!(p.take().is_none());
        assert_eq!(p.history().len(), 1);
    }

    #[test]
    fn shift_enter_creates_new_row() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "first");
        p.insert_newline();
        type_str(&mut p, "second");
        assert_eq!(p.height(), 2);
        assert_eq!(p.as_string(), "first\nsecond");
        assert_eq!(p.cursor_row(), 1);
        assert_eq!(p.cursor(), 6);
    }

    #[test]
    fn newline_in_middle_splits_line() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "abcdef");
        p.move_home();
        p.move_right();
        p.move_right();
        p.move_right();
        p.insert_newline();
        assert_eq!(p.as_string(), "abc\ndef");
        assert_eq!(p.cursor_row(), 1);
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn backspace_at_col0_merges_lines() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "foo");
        p.insert_newline();
        type_str(&mut p, "bar");
        p.move_home();
        p.backspace();
        assert_eq!(p.as_string(), "foobar");
        assert_eq!(p.height(), 1);
        assert_eq!(p.cursor_row(), 0);
        assert_eq!(p.cursor(), 3);
    }

    #[test]
    fn delete_at_eol_merges_with_next_line() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "foo");
        p.insert_newline();
        type_str(&mut p, "bar");
        // Move to end of first line.
        p.move_up();
        p.move_end();
        p.delete();
        assert_eq!(p.as_string(), "foobar");
        assert_eq!(p.height(), 1);
    }

    #[test]
    fn move_up_keeps_desired_column_across_short_lines() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "longest line");
        p.insert_newline();
        type_str(&mut p, "x");
        p.insert_newline();
        type_str(&mut p, "another long line");
        // Cursor at row=2, col=17. Move up to row=1 (col clamps
        // to 1 because line is "x"); then up to row=0 — desired
        // column should snap back to 17.
        p.move_up();
        assert_eq!(p.cursor_row(), 1);
        assert_eq!(p.cursor(), 1);
        p.move_up();
        assert_eq!(p.cursor_row(), 0);
        assert_eq!(
            p.cursor(),
            12,
            "desired col not preserved across short lines"
        );
    }

    #[test]
    fn history_dedupe_and_recall() {
        let mut h = PromptHistory::with_capacity(8);
        h.push("a");
        h.push("b");
        h.push("b");
        h.push("c");
        assert_eq!(h.len(), 3, "consecutive duplicates are deduped");
        assert_eq!(h.recall_prev(), Some("c"));
        assert_eq!(h.recall_prev(), Some("b"));
        assert_eq!(h.recall_prev(), Some("a"));
        assert_eq!(h.recall_prev(), Some("a"), "clamps at oldest");
        assert_eq!(h.recall_next(), Some("b"));
        assert_eq!(h.recall_next(), Some("c"));
        assert_eq!(
            h.recall_next(),
            None,
            "stepping past newest signals draft restore"
        );
    }

    #[test]
    fn recall_round_trip_preserves_draft() {
        let mut p = PromptBuffer::new();
        // Pre-populate history.
        type_str(&mut p, "/status");
        let _ = p.take();
        type_str(&mut p, "/risk");
        let _ = p.take();

        // Type a draft, then recall up twice and back down twice.
        type_str(&mut p, "abc");
        assert_eq!(p.as_string(), "abc");
        p.recall_prev();
        assert_eq!(p.as_string(), "/risk");
        p.recall_prev();
        assert_eq!(p.as_string(), "/status");
        p.recall_next();
        assert_eq!(p.as_string(), "/risk");
        p.recall_next();
        assert_eq!(
            p.as_string(),
            "abc",
            "draft must be restored at end of history walk"
        );
    }

    #[test]
    fn typing_on_recalled_entry_drops_recall_state() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "/status");
        let _ = p.take();
        p.recall_prev();
        assert!(p.history().is_recalling());
        p.insert('x');
        assert!(
            !p.history().is_recalling(),
            "edits commit the recalled line"
        );
        // recall_next now does nothing — there is no draft to
        // restore because the recalled entry has been adopted.
        p.recall_next();
        assert_eq!(p.as_string(), "/statusx");
    }

    #[test]
    fn empty_submit_does_not_pollute_history() {
        let mut p = PromptBuffer::new();
        for c in "   ".chars() {
            p.insert(c);
        }
        assert!(p.take().is_none());
        assert_eq!(p.history().len(), 0);
    }

    #[test]
    fn replace_all_lands_cursor_at_end() {
        let mut p = PromptBuffer::new();
        type_str(&mut p, "ab");
        p.replace_all("/positions ");
        assert_eq!(p.as_string(), "/positions ");
        assert_eq!(p.cursor(), 11);
    }

    #[test]
    fn newline_capped_at_max_lines() {
        let mut p = PromptBuffer::new();
        // MAX_LINES = 64. Pump enough Shift+Enters and verify it
        // never exceeds the cap.
        for _ in 0..200 {
            p.insert_newline();
        }
        assert_eq!(p.height(), super::MAX_LINES);
    }
}
