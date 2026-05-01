//! Session-store abstraction for the dispatcher.
//!
//! The session cohort commands (`/sessions`, `/resume`, `/fork`,
//! `/save`) need read/write access to the on-disk session store.
//! We reach it through a trait (rather than a hard dependency on
//! `zero-session`) for the same reason dispatch reaches
//! operator-state through [`crate::StateSource`]: keep
//! `zero-commands` pluggable, testable without SQLite, and free of
//! migrations noise.
//!
//! Data crossing the trait is plain Rust — `String` ulids, epoch
//! milliseconds, and the crate-local [`ReplayKind`] enum. Callers
//! translate to / from their durable types (`zero_session::EventKind`,
//! `chrono::DateTime<Utc>`) at the boundary.
//!
//! Error policy mirrors `StateSource`: the trait returns `Result`
//! so SQLite-backed impls can surface IO failures, but the
//! dispatcher wraps every call in a function that downgrades the
//! error to an [`OutputLine::Alert`]. A DB hiccup must never take
//! down the TUI — the operator still needs to read the engine.

use std::fmt;

/// One session row surfaced to the dispatcher. Minimal by design:
/// everything beyond what `/sessions` renders is looked up on
/// demand through [`SessionSource::list_events`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// Stable public identifier. What `/resume <arg>` takes as
    /// input and `/sessions` shows first.
    pub ulid: String,
    /// Epoch milliseconds of `started_at`. Callers format for
    /// display; keeping it as a raw integer means the trait is
    /// `chrono`-free.
    pub started_at_ms: i64,
    /// Epoch milliseconds of `ended_at`, or `None` if the session
    /// was interrupted (crash, kill -9). A missing `ended_at` is
    /// load-bearing — it tells the operator a session did not
    /// wrap cleanly, which is information they need when deciding
    /// whether to `/resume` or `/fork`.
    pub ended_at_ms: Option<i64>,
    /// Engine base URL at session start. Surfaced so the operator
    /// can spot cross-environment mismatches ("why is this
    /// resuming from a paper-trading URL?") before replaying.
    pub engine_base_url: Option<String>,
    /// CLI version string at session start. Same rationale as
    /// `engine_base_url`: out-of-date sessions produce subtle
    /// replay surprises and the operator deserves a heads-up.
    pub cli_version: String,
    /// Parent ulid when this session was created via `/fork`.
    /// Rendered as a `parent:<ulid>` tag in the `/sessions` list.
    pub parent_ulid: Option<String>,
    /// Number of events in the session. `-1` means the impl
    /// could not count cheaply; renderers should omit the count
    /// rather than show a lying zero.
    pub n_events: i64,
}

/// Categorisation of a replayed event, stripped of the concrete
/// `EventKind` enum in `zero-session`. The dispatcher hands these
/// to the TUI which renders them through its own
/// [`EntryKind`-equivalent] palette.
///
/// Adding a variant here is a two-site change (this enum + the
/// two translation sites in the runtime impl); leaving the
/// surface narrow is worth the minor duplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplayKind {
    Prompt,
    System,
    Command,
    Warn,
    Alert,
}

/// One replayed event. `at_ms` is the original wall-clock
/// timestamp, so "resuming: X prior events, most recent 14m ago"
/// reads from a truthful clock source rather than a rendered
/// approximation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEvent {
    pub kind: ReplayKind,
    pub at_ms: i64,
    pub text: String,
}

/// Errors a `SessionSource` can surface.
///
/// Deliberately tiny: the dispatcher only needs to know whether a
/// requested id was missing (for a friendly "no such session"
/// line) vs. something else went wrong (alert the operator).
#[derive(Debug, Clone)]
pub enum SessionError {
    NotFound,
    Io(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "session not found"),
            Self::Io(s) => write!(f, "session store error: {s}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// The dispatcher's view of the session store. Implemented by the
/// TUI (wrapping a `zero_session::Store` + a label index on top
/// of milestones) and by test scaffolding that keeps every call
/// in-memory.
///
/// All methods take `&self` — impls are expected to handle
/// interior mutability. The SQLite-backed impl already does, via
/// the store's own `Mutex<Connection>`.
pub trait SessionSource: Send + Sync + 'static {
    /// Ulid of the session currently being recorded into, if any.
    /// `None` when persistence is disabled (`--no-persist`) — the
    /// dispatcher surfaces a clear "persistence disabled" line
    /// rather than pretending the save went through.
    fn current_ulid(&self) -> Option<String>;

    /// Newest-first list of up to `limit` sessions.
    ///
    /// # Errors
    /// Propagates backend IO errors; a listing that errors cannot
    /// be partially rendered without inviting mismatched counts.
    fn list(&self, limit: u32) -> Result<Vec<SessionSummary>, SessionError>;

    /// Look up a single session by ulid **or** a human-assigned
    /// label (see [`Self::save_label`]). Impls should match ulid
    /// first (exact prefix match on at least 6 chars is fine —
    /// ulids are time-sortable so a prefix rarely collides) and
    /// fall through to label resolution.
    ///
    /// # Errors
    /// Returns [`SessionError::NotFound`] when no row matches the
    /// argument. Any other error variant is a backend failure.
    fn find(&self, needle: &str) -> Result<SessionSummary, SessionError>;

    /// All events for a session in seq order (oldest first), up
    /// to `limit`. Mirrors `zero_session::Store::list_events`'s
    /// semantics but returns this crate's [`ReplayEvent`].
    ///
    /// # Errors
    /// Propagates backend IO errors.
    fn list_events(&self, ulid: &str, limit: u32) -> Result<Vec<ReplayEvent>, SessionError>;

    /// Associate a short human label with a session ulid. Labels
    /// are stored as milestones (`session.label.<label>` →
    /// `<ulid>`) so they survive CLI restarts and are queryable
    /// without schema changes. Overwriting a label is fine — the
    /// old one is silently reassigned.
    ///
    /// # Errors
    /// Propagates backend IO errors.
    fn save_label(&self, ulid: &str, label: &str) -> Result<(), SessionError>;

    /// Start a new session whose `parent_ulid` is the current
    /// one. The impl becomes the authority for the new session's
    /// ulid; returns it so the dispatcher can echo the fork line.
    /// `None` is returned when persistence is disabled.
    ///
    /// # Errors
    /// Propagates backend IO errors.
    fn fork_from_current(&self) -> Result<Option<String>, SessionError>;
}

#[cfg(test)]
pub(crate) mod test_support {
    //! In-memory `SessionSource` for dispatcher tests. Concrete
    //! enough to exercise argument parsing + every error path —
    //! no SQLite, no async, no tempdir. Mirror the runtime impl
    //! close enough that parity between the two is easy to eyeball.

    use super::*;
    use std::sync::Mutex;

    #[derive(Default, Debug)]
    pub struct MockSessions {
        pub inner: Mutex<MockInner>,
    }

    #[derive(Default, Debug)]
    pub struct MockInner {
        pub sessions: Vec<SessionSummary>,
        pub events: std::collections::HashMap<String, Vec<ReplayEvent>>,
        pub labels: std::collections::HashMap<String, String>,
        pub current: Option<String>,
        /// When set, every call returns this error instead of the
        /// normal result. Used to prove the dispatcher's error
        /// paths surface alerts without crashing.
        pub fail_with: Option<SessionError>,
    }

    impl MockSessions {
        pub fn with_current(ulid: &str) -> Self {
            Self {
                inner: Mutex::new(MockInner {
                    current: Some(ulid.to_string()),
                    ..MockInner::default()
                }),
            }
        }

        pub fn insert(&self, summary: SessionSummary, events: Vec<ReplayEvent>) {
            let mut g = self.inner.lock().unwrap();
            g.events.insert(summary.ulid.clone(), events);
            g.sessions.push(summary);
            // Keep list_sessions' newest-first invariant.
            g.sessions
                .sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
        }
    }

    impl SessionSource for MockSessions {
        fn current_ulid(&self) -> Option<String> {
            self.inner.lock().unwrap().current.clone()
        }

        fn list(&self, limit: u32) -> Result<Vec<SessionSummary>, SessionError> {
            let g = self.inner.lock().unwrap();
            if let Some(e) = g.fail_with.clone() {
                return Err(e);
            }
            Ok(g.sessions
                .iter()
                .take(usize::try_from(limit).unwrap_or(usize::MAX))
                .cloned()
                .collect())
        }

        fn find(&self, needle: &str) -> Result<SessionSummary, SessionError> {
            let g = self.inner.lock().unwrap();
            if let Some(e) = g.fail_with.clone() {
                return Err(e);
            }
            // Label first so the mock matches the "exact ulid wins,
            // label resolves otherwise" rule we want at runtime.
            let ulid = g
                .labels
                .get(needle)
                .cloned()
                .or_else(|| {
                    g.sessions
                        .iter()
                        .find(|s| s.ulid == needle || s.ulid.starts_with(needle))
                        .map(|s| s.ulid.clone())
                })
                .ok_or(SessionError::NotFound)?;
            g.sessions
                .iter()
                .find(|s| s.ulid == ulid)
                .cloned()
                .ok_or(SessionError::NotFound)
        }

        fn list_events(&self, ulid: &str, limit: u32) -> Result<Vec<ReplayEvent>, SessionError> {
            let g = self.inner.lock().unwrap();
            if let Some(e) = g.fail_with.clone() {
                return Err(e);
            }
            Ok(g.events
                .get(ulid)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(usize::try_from(limit).unwrap_or(usize::MAX))
                .collect())
        }

        fn save_label(&self, ulid: &str, label: &str) -> Result<(), SessionError> {
            let mut g = self.inner.lock().unwrap();
            if let Some(e) = g.fail_with.clone() {
                return Err(e);
            }
            g.labels.insert(label.to_string(), ulid.to_string());
            Ok(())
        }

        fn fork_from_current(&self) -> Result<Option<String>, SessionError> {
            let mut g = self.inner.lock().unwrap();
            if let Some(e) = g.fail_with.clone() {
                return Err(e);
            }
            let Some(parent) = g.current.clone() else {
                return Ok(None);
            };
            let child = format!("{parent}-fork");
            let next_ts = g.sessions.first().map_or(0, |s| s.started_at_ms + 1);
            g.sessions.insert(
                0,
                SessionSummary {
                    ulid: child.clone(),
                    started_at_ms: next_ts,
                    ended_at_ms: None,
                    engine_base_url: None,
                    cli_version: "test".into(),
                    parent_ulid: Some(parent),
                    n_events: 0,
                },
            );
            g.current = Some(child.clone());
            Ok(Some(child))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockSessions;
    use super::*;

    #[test]
    fn mock_list_returns_newest_first_and_respects_limit() {
        let m = MockSessions::with_current("01HA");
        m.insert(
            SessionSummary {
                ulid: "01HA".into(),
                started_at_ms: 1,
                ended_at_ms: None,
                engine_base_url: None,
                cli_version: "0.3.0".into(),
                parent_ulid: None,
                n_events: 0,
            },
            vec![],
        );
        m.insert(
            SessionSummary {
                ulid: "01HB".into(),
                started_at_ms: 2,
                ended_at_ms: Some(3),
                engine_base_url: None,
                cli_version: "0.3.0".into(),
                parent_ulid: None,
                n_events: 2,
            },
            vec![],
        );

        let rows = m.list(10).unwrap();
        assert_eq!(
            rows.iter().map(|s| s.ulid.as_str()).collect::<Vec<_>>(),
            vec!["01HB", "01HA"]
        );
        assert_eq!(m.list(1).unwrap().len(), 1);
    }

    #[test]
    fn mock_find_resolves_ulid_and_label() {
        let m = MockSessions::with_current("01HA");
        m.insert(
            SessionSummary {
                ulid: "01HA".into(),
                started_at_ms: 1,
                ended_at_ms: None,
                engine_base_url: None,
                cli_version: "0.3.0".into(),
                parent_ulid: None,
                n_events: 0,
            },
            vec![],
        );
        m.save_label("01HA", "scratch").unwrap();
        assert_eq!(m.find("01HA").unwrap().ulid, "01HA");
        assert_eq!(m.find("scratch").unwrap().ulid, "01HA");
        assert!(matches!(
            m.find("nope").unwrap_err(),
            SessionError::NotFound
        ));
    }

    #[test]
    fn mock_fork_returns_none_when_no_current_session() {
        let m = MockSessions {
            inner: std::sync::Mutex::new(super::test_support::MockInner::default()),
        };
        assert_eq!(m.fork_from_current().unwrap(), None);
    }
}
