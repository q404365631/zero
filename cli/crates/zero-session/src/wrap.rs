//! Daily wrap generator (Addendum A §9.1).
//!
//! When a session ends after more than 2 hours of live use, the
//! CLI renders a *wrap*: a short, honest summary of what the
//! operator did this session. The wrap is saved under
//! `~/.zero/state/wraps/<session_ulid>.json` and printed as a
//! single advisory line on stderr. The next session does not see
//! the wrap — it is a closing statement, not a reminder.
//!
//! # Honesty contract
//!
//! - The wrap is *computed*, never curated. Every number traces
//!   back to rows in the session store; no rankings, no
//!   gamification, no "streak" counters (§15's "no cute"
//!   locks those out).
//! - The wrap never editorialises. It reports: duration,
//!   command counts grouped by risk direction, a few top-N
//!   tallies, and — if present — the number of `warn` /
//!   `alert` lines the dispatcher emitted. That is it.
//! - `/wrap-off` suppresses the current session's wrap only.
//!   The operator cannot permanently disable it (§15).
//!
//! # Separation of concerns
//!
//! [`generate`] is pure: it takes a [`SessionRow`] + its stored
//! events and returns a [`WrapReport`]. No disk I/O, no clock.
//! This lets tests run fast and deterministically.
//!
//! [`write_wrap`] is the I/O half: it takes a report + a target
//! dir and writes `<ulid>.json`, returning the final path. Tests
//! use `tempfile::TempDir` or a throwaway path under
//! `std::env::temp_dir()` so production `~/.zero` is never
//! touched.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event::{EventKind, SessionRow, StoredEvent};

/// Minimum session length before a wrap is generated. The spec
/// (Addendum A §9.1) says "session exit >2h"; re-declaring the
/// threshold here lets callers avoid importing a magic number.
pub const MIN_WRAP_DURATION: chrono::Duration = chrono::Duration::hours(2);

/// The wrap artifact as persisted to disk and printed as a line
/// in the log.
///
/// `#[serde(deny_unknown_fields)]` is intentional on the
/// **reader** side (not here) — a future wrap schema that drops
/// a field would be a silent honesty regression if old tooling
/// kept deserialising the old shape. For now the writer is the
/// only producer, so no deny-unknown on this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrapReport {
    /// Schema version of this wrap. Bump on shape changes so a
    /// future reader can refuse an incompatible blob up front.
    pub schema: u32,

    /// The session's ULID. Matches the artifact filename so
    /// reading a wraps directory without loading JSON is
    /// possible.
    pub session_ulid: String,

    /// Session start — ISO-8601 with zone.
    pub started_at: DateTime<Utc>,

    /// Session end — the timestamp the wrap is being generated
    /// at (the event-loop's `drive()` returns and we snapshot
    /// `Utc::now()`). Not the last-event timestamp because the
    /// operator may have idled at the prompt for a while after
    /// the last command and that idle is still session time.
    pub ended_at: DateTime<Utc>,

    /// Duration in seconds. Redundant with `ended_at -
    /// started_at` but materialised so a reader does not have
    /// to do the subtraction and does not have to agree on
    /// leap-second handling.
    pub duration_secs: u64,

    /// Total events the store captured this session. Includes
    /// every line that hit `SessionSink::push`.
    pub total_events: u64,

    /// Per-kind event counts. Stable insertion order: prompt,
    /// command, system, warn, alert, mode_change. A future
    /// kind added to [`EventKind`] surfaces as a zero here
    /// until the generator is updated (caught by the
    /// exhaustive-match test).
    pub event_counts: EventCounts,

    /// The top-N most-invoked slash commands this session.
    /// Computed from `prompt` events whose text starts with
    /// `/`. Ordered by descending count, then alphabetically
    /// on ties for determinism. `N = 10` is the hard cap so
    /// a session that hammered `/status` does not bury the
    /// rest.
    pub top_commands: Vec<CommandCount>,
}

/// Per-kind event counts. Named fields rather than a HashMap
/// so the JSON shape is self-documenting and readers do not
/// have to probe for keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventCounts {
    pub prompt: u64,
    pub command: u64,
    pub system: u64,
    pub warn: u64,
    pub alert: u64,
    pub mode_change: u64,
}

/// A single entry in [`WrapReport::top_commands`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandCount {
    /// The slash-command, leading slash included. We store
    /// `/status` not `status` so the JSON reads the same as
    /// the operator's input; no post-hoc normalisation on the
    /// reader side.
    pub name: String,

    /// How many times the operator invoked it this session.
    pub count: u64,
}

/// Current schema version. Bump on any shape change — new
/// optional field is fine, renamed or removed field requires
/// a bump + a reader-side compat shim.
const SCHEMA: u32 = 1;

/// Maximum top-N commands surfaced in a wrap.
const TOP_COMMANDS_CAP: usize = 10;

/// Pure wrap computation. No clock, no disk.
///
/// `ended_at` is the caller's snapshot of "when did the
/// session end?" — in production it is `Utc::now()` at the
/// moment `app.run()` returns. Keeping it an argument makes
/// this function deterministic: tests pin a specific end
/// timestamp and get a reproducible report.
///
/// Events outside the session's `[started_at, ended_at]`
/// window are included as-is — the session sink only writes
/// events belonging to the current session row, so a stray
/// out-of-window event would already indicate a store bug,
/// and silently filtering it here would mask the bug.
#[must_use]
pub fn generate(
    session: &SessionRow,
    events: &[StoredEvent],
    ended_at: DateTime<Utc>,
) -> WrapReport {
    // `num_seconds()` returns i64; `.max(0)` clamps a
    // clock-went-backwards edge to zero. `cast_unsigned` is
    // the clippy-blessed u64 reinterpretation of a known-
    // non-negative i64 — equivalent to `as u64` but the
    // intent is documented by the call name.
    let duration_secs = (ended_at - session.started_at)
        .num_seconds()
        .max(0)
        .cast_unsigned();

    let mut counts = EventCounts::default();
    for e in events {
        match e.kind {
            EventKind::Prompt => counts.prompt += 1,
            EventKind::Command => counts.command += 1,
            EventKind::System => counts.system += 1,
            EventKind::Warn => counts.warn += 1,
            EventKind::Alert => counts.alert += 1,
            EventKind::ModeChange => counts.mode_change += 1,
        }
    }

    WrapReport {
        schema: SCHEMA,
        session_ulid: session.ulid.clone(),
        started_at: session.started_at,
        ended_at,
        duration_secs,
        // `usize → u64` is a widening on 64-bit platforms and
        // saturating on 32-bit; either way we would never
        // overflow a realistic session. `u64::try_from` is
        // exactly-correct and Clippy-clean.
        total_events: u64::try_from(events.len()).unwrap_or(u64::MAX),
        event_counts: counts,
        top_commands: compute_top_commands(events),
    }
}

/// Compute the top-N slash-command tally from prompt events.
///
/// We only count events whose text starts with `/` to avoid
/// pulling free-form conversation into the histogram. The
/// first whitespace-separated token is the command name; args
/// are stripped so `/status`, `/status BTC`, and `/status ETH`
/// collapse into one `/status` row.
///
/// Ordering is descending count, then alphabetical on ties,
/// so an operator running the same set of commands at the
/// same cadence day after day gets a stable wrap.
fn compute_top_commands(events: &[StoredEvent]) -> Vec<CommandCount> {
    use std::collections::HashMap;

    let mut tally: HashMap<String, u64> = HashMap::new();
    for e in events {
        if e.kind != EventKind::Prompt {
            continue;
        }
        let trimmed = e.text.trim();
        if !trimmed.starts_with('/') {
            continue;
        }
        let first_token = trimmed.split_whitespace().next().unwrap_or(trimmed);
        *tally.entry(first_token.to_string()).or_insert(0) += 1;
    }

    let mut rows: Vec<CommandCount> = tally
        .into_iter()
        .map(|(name, count)| CommandCount { name, count })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    rows.truncate(TOP_COMMANDS_CAP);
    rows
}

/// Determine whether this session qualifies for a wrap.
///
/// Two conditions, both must hold:
/// 1. Duration ≥ [`MIN_WRAP_DURATION`] (§9.1 "session exit >2h").
/// 2. At least one prompt event was captured. A 2-hour idle
///    session with zero input is not worth a wrap — the
///    operator never actually operated.
#[must_use]
pub fn should_wrap(session: &SessionRow, events: &[StoredEvent], ended_at: DateTime<Utc>) -> bool {
    let duration = ended_at - session.started_at;
    if duration < MIN_WRAP_DURATION {
        return false;
    }
    events.iter().any(|e| e.kind == EventKind::Prompt)
}

/// Persist a wrap report as `<dir>/<session_ulid>.json`,
/// creating `dir` if needed. Returns the final path.
///
/// Atomicity: write-to-temp-then-rename so a crash mid-write
/// never leaves a half-written wrap. The rename target name
/// is stable, so two concurrent writes (e.g. a double-wrap
/// race) are last-writer-wins without corruption.
///
/// # Errors
///
/// Returns [`crate::SessionError::Io`] for any filesystem
/// issue (directory create, temp-write, rename) and
/// [`crate::SessionError::Serde`] if serialisation fails.
pub fn write_wrap(dir: &Path, report: &WrapReport) -> Result<PathBuf, crate::SessionError> {
    std::fs::create_dir_all(dir)?;
    let final_path = dir.join(format!("{}.json", report.session_ulid));
    let tmp_path = dir.join(format!("{}.json.tmp", report.session_ulid));
    let json = serde_json::to_vec_pretty(report)?;
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn row(started_at: DateTime<Utc>) -> SessionRow {
        SessionRow {
            id: 1,
            ulid: "01HTEST".into(),
            started_at,
            ended_at: None,
            engine_base_url: Some("https://example".into()),
            cli_version: "0.3.0-test".into(),
            parent_ulid: None,
        }
    }

    fn ev(
        session_id: i64,
        seq: i64,
        at: DateTime<Utc>,
        kind: EventKind,
        text: &str,
    ) -> StoredEvent {
        StoredEvent {
            id: seq,
            session_id,
            seq,
            at,
            kind,
            text: text.into(),
        }
    }

    #[test]
    fn generate_counts_every_kind_and_computes_duration() {
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let end = start + Duration::hours(3);
        let r = row(start);
        let evs = vec![
            ev(1, 1, start, EventKind::Prompt, "/status"),
            ev(1, 2, start, EventKind::Command, "engine: OK"),
            ev(1, 3, start, EventKind::Prompt, "/status BTC"),
            ev(1, 4, start, EventKind::System, "poller started"),
            ev(1, 5, start, EventKind::Warn, "slow response"),
            ev(1, 6, start, EventKind::Alert, "engine unreachable"),
            ev(1, 7, start, EventKind::ModeChange, "positions"),
            ev(1, 8, start, EventKind::Prompt, "/risk"),
        ];
        let w = generate(&r, &evs, end);
        assert_eq!(w.schema, SCHEMA);
        assert_eq!(w.session_ulid, "01HTEST");
        assert_eq!(w.started_at, start);
        assert_eq!(w.ended_at, end);
        assert_eq!(w.duration_secs, 3 * 3600);
        assert_eq!(w.total_events, 8);
        assert_eq!(w.event_counts.prompt, 3);
        assert_eq!(w.event_counts.command, 1);
        assert_eq!(w.event_counts.system, 1);
        assert_eq!(w.event_counts.warn, 1);
        assert_eq!(w.event_counts.alert, 1);
        assert_eq!(w.event_counts.mode_change, 1);
    }

    #[test]
    fn top_commands_strips_args_and_sorts_stably() {
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let r = row(start);
        let evs = vec![
            ev(1, 1, start, EventKind::Prompt, "/status"),
            ev(1, 2, start, EventKind::Prompt, "/status BTC"),
            ev(1, 3, start, EventKind::Prompt, "/status ETH"),
            ev(1, 4, start, EventKind::Prompt, "/risk"),
            ev(1, 5, start, EventKind::Prompt, "/regime"),
            ev(1, 6, start, EventKind::Prompt, "/regime BTC"),
            // Non-slash prompt: free-form chat, should be ignored.
            ev(1, 7, start, EventKind::Prompt, "what is going on"),
            // Non-prompt row that happens to start with /: should
            // also be ignored (only `prompt` rows count).
            ev(1, 8, start, EventKind::System, "/auto-line"),
        ];
        let w = generate(&r, &evs, start + Duration::hours(3));
        let top: Vec<(&str, u64)> = w
            .top_commands
            .iter()
            .map(|c| (c.name.as_str(), c.count))
            .collect();
        // /status=3 beats /regime=2 beats /risk=1; tie-break
        // is alphabetical (no tie among these three).
        assert_eq!(top, vec![("/status", 3), ("/regime", 2), ("/risk", 1)]);
    }

    #[test]
    fn top_commands_tie_breaks_alphabetically() {
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let r = row(start);
        let evs = vec![
            ev(1, 1, start, EventKind::Prompt, "/zebra"),
            ev(1, 2, start, EventKind::Prompt, "/alpha"),
            ev(1, 3, start, EventKind::Prompt, "/mango"),
        ];
        let w = generate(&r, &evs, start + Duration::hours(3));
        let names: Vec<&str> = w.top_commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["/alpha", "/mango", "/zebra"]);
    }

    #[test]
    fn top_commands_caps_at_n() {
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let r = row(start);
        let mut evs = Vec::new();
        // TOP_COMMANDS_CAP is a usize constant; widen via
        // `i64::try_from` to keep Clippy happy and avoid any
        // platform-sensitive `as` casts.
        let cap = i64::try_from(TOP_COMMANDS_CAP).expect("cap fits in i64");
        for i in 0..(cap + 5) {
            evs.push(ev(
                1,
                i + 1,
                start,
                EventKind::Prompt,
                &format!("/cmd{i:02}"),
            ));
        }
        let w = generate(&r, &evs, start + Duration::hours(3));
        assert_eq!(w.top_commands.len(), TOP_COMMANDS_CAP);
    }

    #[test]
    fn should_wrap_respects_duration_floor() {
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let r = row(start);
        let evs = vec![ev(1, 1, start, EventKind::Prompt, "/status")];
        assert!(!should_wrap(&r, &evs, start + Duration::minutes(119)));
        assert!(should_wrap(&r, &evs, start + Duration::hours(2)));
        assert!(should_wrap(&r, &evs, start + Duration::hours(5)));
    }

    #[test]
    fn should_wrap_requires_at_least_one_prompt() {
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let r = row(start);
        // 3 hours of polling with zero operator input. No wrap.
        let evs: Vec<_> = (0..10)
            .map(|i| ev(1, i + 1, start, EventKind::System, "poll"))
            .collect();
        assert!(!should_wrap(&r, &evs, start + Duration::hours(3)));
    }

    #[test]
    fn should_wrap_handles_clock_going_backwards() {
        // Defensive: if `ended_at` is before `started_at`
        // (wall-clock NTP adjustment mid-session), the
        // duration is negative; `should_wrap` must return
        // false rather than panicking on a Duration arithmetic
        // edge case.
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let r = row(start);
        let evs = vec![ev(1, 1, start, EventKind::Prompt, "/status")];
        assert!(!should_wrap(&r, &evs, start - Duration::minutes(5)));
    }

    #[test]
    fn write_wrap_round_trips_through_disk() {
        use std::fs;
        let start = Utc.with_ymd_and_hms(2026, 4, 21, 10, 0, 0).unwrap();
        let r = row(start);
        let evs = vec![ev(1, 1, start, EventKind::Prompt, "/status")];
        let report = generate(&r, &evs, start + Duration::hours(3));

        let dir = std::env::temp_dir().join(format!("zero-wrap-test-{}", report.session_ulid));
        let _ = fs::remove_dir_all(&dir);
        let path = write_wrap(&dir, &report).expect("write");
        assert!(path.ends_with("01HTEST.json"));
        let bytes = fs::read(&path).expect("read");
        let back: WrapReport = serde_json::from_slice(&bytes).expect("parse");
        assert_eq!(back, report);
        let _ = fs::remove_dir_all(&dir);
    }
}
