//! Conversation log — append-only list of entries the operator has
//! seen. The widget renders the tail that fits the visible area.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Text the operator typed.
    Prompt,
    /// Reply from the engine or an ambient system message.
    System,
    /// A slash-command output line.
    Command,
    /// A warning the operator should not miss (muted amber).
    Warn,
    /// A blocked / denied action (alert red).
    Alert,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub at: DateTime<Utc>,
    pub kind: EntryKind,
    pub text: String,
    /// Opaque correlation id for streaming appends. `Some(id)`
    /// marks the entry as "the engine may send more text for
    /// this id"; [`ConversationLog::extend_last`] targets it.
    /// `None` is a finalized entry — the normal case.
    pub stream_id: Option<String>,
}

impl LogEntry {
    #[must_use]
    pub fn new(kind: EntryKind, text: impl Into<String>) -> Self {
        Self {
            at: Utc::now(),
            kind,
            text: text.into(),
            stream_id: None,
        }
    }

    #[must_use]
    pub fn at(mut self, ts: DateTime<Utc>) -> Self {
        self.at = ts;
        self
    }

    /// Attach a streaming id so subsequent
    /// [`ConversationLog::extend_last`] calls can append to this
    /// row rather than emitting a new one. The engine-side
    /// streaming source lands with M2 (HTTP SSE/WS partial
    /// messages); this accessor is scaffolding so the TUI does
    /// not need another schema change when it arrives.
    #[must_use]
    pub fn streaming(mut self, id: impl Into<String>) -> Self {
        self.stream_id = Some(id.into());
        self
    }
}

#[derive(Debug, Default)]
pub struct ConversationLog {
    entries: Vec<LogEntry>,
    cap: usize,
}

impl ConversationLog {
    /// `cap = 0` means unbounded. Production uses ~2048; tests use
    /// smaller values to exercise wrapping cheaply.
    #[must_use]
    pub const fn with_capacity(cap: usize) -> Self {
        Self {
            entries: Vec::new(),
            cap,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        self.entries.push(entry);
        if self.cap > 0 && self.entries.len() > self.cap {
            // Drop the oldest; a ring buffer would be faster but
            // this path runs at typing speed, not market speed.
            let drop_n = self.entries.len() - self.cap;
            self.entries.drain(..drop_n);
        }
    }

    #[must_use]
    pub fn tail(&self, n: usize) -> &[LogEntry] {
        let len = self.entries.len();
        let start = len.saturating_sub(n);
        &self.entries[start..]
    }

    /// Full entry slice, ordered oldest → newest. The conversation
    /// pane uses this for scrollback indexing; [`tail`] stays for
    /// callers that only care about the most recent N rows.
    #[must_use]
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Mutable access to the newest entry, if any. Used by the
    /// streaming hook to append text onto a partial assistant
    /// reply without allocating a fresh row.
    pub fn last_mut(&mut self) -> Option<&mut LogEntry> {
        self.entries.last_mut()
    }

    /// Streaming append — extend the text of the entry whose
    /// [`LogEntry::stream_id`] matches `id`. The engine-side
    /// streaming transport (SSE / WS partial messages) is not yet
    /// wired in M1; this method exists so the UI is ready to
    /// consume it in M2 without a further widget rewrite.
    ///
    /// Returns `true` when the append landed, `false` if no
    /// matching entry was found (the caller should create a new
    /// entry in that case).
    pub fn extend_last(&mut self, id: &str, more: &str) -> bool {
        let Some(last) = self.entries.last_mut() else {
            return false;
        };
        if last.stream_id.as_deref() != Some(id) {
            return false;
        }
        last.text.push_str(more);
        true
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

#[cfg(test)]
mod tests {
    use super::{ConversationLog, EntryKind, LogEntry};

    #[test]
    fn tail_returns_last_n() {
        let mut log = ConversationLog::with_capacity(0);
        for i in 0..5 {
            log.push(LogEntry::new(EntryKind::System, format!("{i}")));
        }
        let tail: Vec<&str> = log.tail(3).iter().map(|e| e.text.as_str()).collect();
        assert_eq!(tail, ["2", "3", "4"]);
    }

    #[test]
    fn capacity_trims_oldest() {
        let mut log = ConversationLog::with_capacity(3);
        for i in 0..5 {
            log.push(LogEntry::new(EntryKind::System, format!("{i}")));
        }
        assert_eq!(log.len(), 3);
        let tail: Vec<&str> = log.tail(10).iter().map(|e| e.text.as_str()).collect();
        assert_eq!(tail, ["2", "3", "4"]);
    }

    #[test]
    fn extend_last_appends_to_streaming_entry() {
        let mut log = ConversationLog::with_capacity(0);
        log.push(LogEntry::new(EntryKind::System, "hello").streaming("sid-1"));
        assert!(log.extend_last("sid-1", ", world"));
        assert_eq!(log.tail(1)[0].text, "hello, world");
    }

    #[test]
    fn extend_last_refuses_mismatched_id() {
        let mut log = ConversationLog::with_capacity(0);
        log.push(LogEntry::new(EntryKind::System, "partial").streaming("sid-1"));
        assert!(
            !log.extend_last("sid-2", " nope"),
            "mismatched id must not silently append"
        );
        assert_eq!(log.tail(1)[0].text, "partial");
    }

    #[test]
    fn extend_last_refuses_finalized_entry() {
        let mut log = ConversationLog::with_capacity(0);
        log.push(LogEntry::new(EntryKind::System, "final"));
        assert!(
            !log.extend_last("any", " more"),
            "finalized entry (no stream_id) must reject appends"
        );
    }
}
