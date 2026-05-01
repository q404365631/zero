//! Session glue — adapts [`zero_session::Store`] to the TUI.
//!
//! The TUI does not care whether persistence is on. If the user
//! asked for `--no-persist`, or the DB failed to open, we fall
//! back to a no-op sink so the render path is unchanged.
//!
//! This module hosts two adaptor surfaces:
//! - [`SessionSink`] — the write side. Every [`LogEntry`]
//!   flowing through `AppState::push` is mirrored here.
//! - [`SessionAdapter`] — the read side plus the fork/save
//!   hooks. It implements [`zero_commands::SessionSource`] so
//!   `/sessions`, `/resume`, `/fork`, `/save` all reach the store
//!   without `zero-commands` taking a hard dep on
//!   `zero-session`.
//!
//! Both share an `Arc<Mutex<ActiveSession>>` so a `/fork` command
//! can atomically swap the sink's target under the dispatcher's
//! feet without a round-trip through `apply_dispatch` — keeping
//! the "every persisted line lands in the current session" rule
//! enforceable without ceremony.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use zero_commands::{
    ReplayEvent, ReplayKind, SessionError as CmdSessionError, SessionSource, SessionSummary,
};
use zero_session::{EventKind as SessionKind, SessionError, SessionRow, Store, StoredEvent};

use crate::app::log::{EntryKind, LogEntry};

/// The `(session_id, ulid)` pair currently receiving writes.
/// `None` is reached only after [`SessionAdapter::end_current`]
/// (which we do not call yet) — the field exists today for the
/// `/fork` swap.
#[derive(Debug, Default, Clone)]
struct ActiveSession {
    row_id: Option<i64>,
    ulid: Option<String>,
}

/// A write sink for session persistence. `None` means persistence
/// is disabled; callers treat it as an append-only log.
#[derive(Clone)]
pub struct SessionSink {
    store: Arc<Store>,
    active: Arc<Mutex<ActiveSession>>,
}

impl std::fmt::Debug for SessionSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let active = self.active.lock().unwrap();
        f.debug_struct("SessionSink")
            .field("row_id", &active.row_id)
            .field("ulid", &active.ulid)
            .finish_non_exhaustive()
    }
}

impl SessionSink {
    #[must_use]
    pub fn new(store: Arc<Store>, session_id: i64, ulid: String) -> Self {
        Self {
            store,
            active: Arc::new(Mutex::new(ActiveSession {
                row_id: Some(session_id),
                ulid: Some(ulid),
            })),
        }
    }

    /// Clone the shared handle so a [`SessionAdapter`] sees the
    /// same active ulid/row-id whenever `/fork` swaps it.
    #[must_use]
    pub fn adapter(&self) -> SessionAdapter {
        SessionAdapter {
            store: Arc::clone(&self.store),
            active: Arc::clone(&self.active),
        }
    }

    /// Record one log entry. Errors are logged but do not propagate
    /// — a DB hiccup must not deny the operator a visible render.
    pub fn record(&self, entry: &LogEntry) {
        let Some(session_id) = self.active.lock().unwrap().row_id else {
            return;
        };
        let kind = to_session_kind(entry.kind);
        if let Err(e) = self.store.append(session_id, kind, &entry.text) {
            tracing::warn!(err = %e, "session append failed");
        }
    }

    /// Close the *originally-opened* session row. Called during
    /// shutdown. Forks open their own rows but do not close on
    /// exit — a child session whose parent is still marked live
    /// is the honest representation of a crash-exit.
    pub fn end(&self) {
        if let Some(session_id) = self.active.lock().unwrap().row_id
            && let Err(e) = self.store.end_session(session_id)
        {
            tracing::warn!(err = %e, "session end failed");
        }
    }

    /// The store this sink writes to. Exposed so a post-run
    /// caller (daily-wrap generator, milestone writer) can
    /// reach the same DB handle the sink has been using
    /// without the caller having to carry a separate `Arc`.
    ///
    /// Returned as a shared reference — the caller must not
    /// mutate the Arc; the write path remains the sink itself.
    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Snapshot of the originally-opened row's id. Returns
    /// `None` if the session was forked away and never forked
    /// back — today `end()` is still keyed on this same id,
    /// so `None` is only reachable via a `/fork` without a
    /// return, which the fork command never performs in M1.
    #[must_use]
    pub fn session_id(&self) -> Option<i64> {
        self.active.lock().unwrap().row_id
    }

    /// Snapshot of the current active ULID. Same caveat as
    /// [`Self::session_id`].
    #[must_use]
    pub fn ulid(&self) -> Option<String> {
        self.active.lock().unwrap().ulid.clone()
    }
}

/// Read + fork/save adaptor over a [`Store`], implementing
/// [`SessionSource`] so the dispatcher can reach the on-disk
/// history. Carries the same `Arc<Mutex<ActiveSession>>` as
/// [`SessionSink`] so `/fork` atomically swaps the write target.
#[derive(Clone)]
pub struct SessionAdapter {
    store: Arc<Store>,
    active: Arc<Mutex<ActiveSession>>,
}

impl std::fmt::Debug for SessionAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionAdapter")
            .field("active_ulid", &self.active.lock().unwrap().ulid)
            .finish_non_exhaustive()
    }
}

impl SessionAdapter {
    /// Resolve a `needle` (ulid prefix or saved label) to a full
    /// session row. Prefix match requires ≥ 6 chars so
    /// cross-session collisions are vanishingly unlikely (ulids
    /// are 26 base-32 chars; the first 10 encode time). Label
    /// lookup runs only when the prefix path misses. Returns
    /// `Ok(None)` for a clean "no such session" so callers can
    /// translate to [`CmdSessionError::NotFound`] without
    /// depending on `rusqlite`.
    fn resolve_needle(&self, needle: &str) -> Result<Option<SessionRow>, SessionError> {
        if let Some(row) = self.store.get_session_by_ulid(needle)? {
            return Ok(Some(row));
        }
        if needle.len() >= 6 {
            let rows = self.store.list_sessions(1000)?;
            if let Some(hit) = rows.into_iter().find(|r| r.ulid.starts_with(needle)) {
                return Ok(Some(hit));
            }
        }
        let key = label_key(needle);
        if let Some(ulid) = self.store.get_milestone(&key)?
            && let Some(row) = self.store.get_session_by_ulid(&ulid)?
        {
            return Ok(Some(row));
        }
        Ok(None)
    }
}

impl SessionSource for SessionAdapter {
    fn current_ulid(&self) -> Option<String> {
        self.active.lock().unwrap().ulid.clone()
    }

    fn list(&self, limit: u32) -> Result<Vec<SessionSummary>, CmdSessionError> {
        let rows = self.store.list_sessions(limit).map_err(io_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            // count_events is cheap (indexed COUNT) so we pay it
            // per row. If the table grows beyond toy sizes we can
            // switch to a single JOIN query; for now clarity wins.
            let n_events = self.store.count_events(row.id).map_err(io_err)?;
            out.push(row_to_summary(row, n_events));
        }
        Ok(out)
    }

    fn find(&self, needle: &str) -> Result<SessionSummary, CmdSessionError> {
        let row = self
            .resolve_needle(needle)
            .map_err(io_err)?
            .ok_or(CmdSessionError::NotFound)?;
        let n_events = self.store.count_events(row.id).map_err(io_err)?;
        Ok(row_to_summary(row, n_events))
    }

    fn list_events(&self, ulid: &str, limit: u32) -> Result<Vec<ReplayEvent>, CmdSessionError> {
        let row = self
            .store
            .get_session_by_ulid(ulid)
            .map_err(io_err)?
            .ok_or(CmdSessionError::NotFound)?;
        let events = self.store.list_events(row.id, limit).map_err(io_err)?;
        Ok(events.into_iter().map(stored_to_replay).collect())
    }

    fn save_label(&self, ulid: &str, label: &str) -> Result<(), CmdSessionError> {
        // Guard against empty / whitespace-only labels — a bare
        // `/save  ` would otherwise overwrite the sentinel key.
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return Err(CmdSessionError::Io("empty label".into()));
        }
        self.store
            .set_milestone(&label_key(trimmed), ulid)
            .map_err(io_err)
    }

    fn fork_from_current(&self) -> Result<Option<String>, CmdSessionError> {
        let parent = self.active.lock().unwrap().ulid.clone();
        let Some(parent_ulid) = parent else {
            return Ok(None);
        };
        // We don't know the engine_base_url / cli_version here —
        // those were captured at initial startup. The store keeps
        // them on the parent row; the child row inherits them
        // only via `parent_ulid` so `/sessions` shows the link.
        // That is honest: a fork happens mid-session and the
        // engine URL could have changed via `/connect` (future),
        // so re-using the original value would be a subtle lie.
        let new_ulid = new_ulid();
        let new_row_id = self
            .store
            .start_session(
                &new_ulid,
                None,
                env!("CARGO_PKG_VERSION"),
                Some(&parent_ulid),
            )
            .map_err(io_err)?;
        let mut g = self.active.lock().unwrap();
        g.row_id = Some(new_row_id);
        g.ulid = Some(new_ulid.clone());
        Ok(Some(new_ulid))
    }
}

/// Translate the persisted kind back into a TUI entry kind. The
/// schema's `mode_change` rows are folded into `System` on replay
/// — the mode switch already happened; the row is a breadcrumb.
#[must_use]
pub fn to_entry_kind(k: SessionKind) -> EntryKind {
    match k {
        SessionKind::Prompt => EntryKind::Prompt,
        SessionKind::System | SessionKind::ModeChange => EntryKind::System,
        SessionKind::Command => EntryKind::Command,
        SessionKind::Warn => EntryKind::Warn,
        SessionKind::Alert => EntryKind::Alert,
    }
}

fn to_session_kind(k: EntryKind) -> SessionKind {
    match k {
        EntryKind::Prompt => SessionKind::Prompt,
        EntryKind::System => SessionKind::System,
        EntryKind::Command => SessionKind::Command,
        EntryKind::Warn => SessionKind::Warn,
        EntryKind::Alert => SessionKind::Alert,
    }
}

/// Rehydrate stored events into log entries, preserving the
/// original timestamps so rendered "age" reads stay truthful.
#[must_use]
pub fn replay(events: &[StoredEvent]) -> Vec<LogEntry> {
    events
        .iter()
        .map(|e| LogEntry::new(to_entry_kind(e.kind), &e.text).at(e.at))
        .collect()
}

/// Heuristic summary of a prior session for the resume banner.
#[must_use]
pub fn summarize(row: &SessionRow, n_events: usize) -> String {
    let ts = row.started_at.format("%Y-%m-%d %H:%M UTC");
    let status = if row.ended_at.is_some() {
        "ended"
    } else {
        "interrupted"
    };
    format!("resuming: {ts} · {status} · {n_events} prior event(s)")
}

fn row_to_summary(row: SessionRow, n_events: i64) -> SessionSummary {
    SessionSummary {
        ulid: row.ulid,
        started_at_ms: row.started_at.timestamp_millis(),
        ended_at_ms: row.ended_at.map(|dt| dt.timestamp_millis()),
        engine_base_url: row.engine_base_url,
        cli_version: row.cli_version,
        parent_ulid: row.parent_ulid,
        n_events,
    }
}

fn stored_to_replay(e: StoredEvent) -> ReplayEvent {
    ReplayEvent {
        kind: stored_kind_to_replay(e.kind),
        at_ms: e.at.timestamp_millis(),
        text: e.text,
    }
}

fn stored_kind_to_replay(k: SessionKind) -> ReplayKind {
    match k {
        SessionKind::Prompt => ReplayKind::Prompt,
        SessionKind::System | SessionKind::ModeChange => ReplayKind::System,
        SessionKind::Command => ReplayKind::Command,
        SessionKind::Warn => ReplayKind::Warn,
        SessionKind::Alert => ReplayKind::Alert,
    }
}

// Passed by value because the caller's error is already about to
// be discarded — there is no use for a borrowed reference, and
// `.map_err(io_err)` is the idiomatic short form. Clippy's
// needless-pass-by-value fires here despite that.
#[allow(clippy::needless_pass_by_value)]
fn io_err(e: SessionError) -> CmdSessionError {
    CmdSessionError::Io(e.to_string())
}

fn label_key(label: &str) -> String {
    // Namespace labels under a fixed prefix so they cannot collide
    // with the in-tree milestone constants (`welcome_shown`, etc.).
    format!("session.label.{label}")
}

/// Minimal ULID-ish id — see the matching function in
/// `zero/src/main.rs`. Copied (rather than exposed as a public
/// helper) because session-sensitive crates should not fork the
/// `ulid` crate transitively through `zero-tui`.
fn new_ulid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let rand = fastrand_hex(6);
    format!("{ms:013x}{rand}")
}

fn fastrand_hex(n: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut state: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x9E37_79B9_7F4A_7C15, |d| {
            u64::try_from(d.as_nanos()).unwrap_or(0x9E37_79B9_7F4A_7C15)
        });
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            char::from_digit(u32::try_from((state >> 60) & 0xF).unwrap_or(0), 16).unwrap_or('0')
        })
        .collect()
}

// Unused — keeps the dead-code lint quiet since the rfc3339
// parser helper previously exported here is no longer referenced
// after the adapter refactor.
#[allow(dead_code)]
fn _nudge(_: DateTime<Utc>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_current_ulid_tracks_active_session() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let id = store.start_session("01HX", None, "0.3.0", None).unwrap();
        let sink = SessionSink::new(Arc::clone(&store), id, "01HX".into());
        let adapter = sink.adapter();
        assert_eq!(adapter.current_ulid().as_deref(), Some("01HX"));
    }

    #[test]
    fn adapter_fork_swaps_active_ulid_and_links_parent() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let id = store
            .start_session("01HPARENT", None, "0.3.0", None)
            .unwrap();
        let sink = SessionSink::new(Arc::clone(&store), id, "01HPARENT".into());
        let adapter = sink.adapter();

        let child_ulid = adapter
            .fork_from_current()
            .unwrap()
            .expect("fork produced ulid");
        // Active ulid must have swapped from both views.
        assert_eq!(adapter.current_ulid(), Some(child_ulid.clone()));
        assert_eq!(
            sink.active.lock().unwrap().ulid.as_deref(),
            Some(child_ulid.as_str()),
            "sink must see the fork under it",
        );

        // Child row in DB should carry parent_ulid.
        let child = store.get_session_by_ulid(&child_ulid).unwrap().unwrap();
        assert_eq!(child.parent_ulid.as_deref(), Some("01HPARENT"));
    }

    #[test]
    fn adapter_save_label_then_find_by_label() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let id = store.start_session("01HLBL", None, "0.3.0", None).unwrap();
        let sink = SessionSink::new(Arc::clone(&store), id, "01HLBL".into());
        let adapter = sink.adapter();

        adapter.save_label("01HLBL", "pre-cpi").unwrap();
        let hit = adapter.find("pre-cpi").unwrap();
        assert_eq!(hit.ulid, "01HLBL");
    }

    #[test]
    fn adapter_find_missing_returns_not_found() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let id = store.start_session("01HX", None, "0.3.0", None).unwrap();
        let sink = SessionSink::new(store, id, "01HX".into());
        let adapter = sink.adapter();
        assert!(matches!(
            adapter.find("nope").unwrap_err(),
            CmdSessionError::NotFound
        ));
    }

    #[test]
    fn adapter_save_rejects_empty_label() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let id = store.start_session("01HE", None, "0.3.0", None).unwrap();
        let sink = SessionSink::new(store, id, "01HE".into());
        let adapter = sink.adapter();
        assert!(matches!(
            adapter.save_label("01HE", "   ").unwrap_err(),
            CmdSessionError::Io(_)
        ));
    }
}
