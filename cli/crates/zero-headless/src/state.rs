//! Persistent supervisor state under
//! `~/.zero/state/headless.json`.
//!
//! # Why persist
//!
//! Without persistence, a daemon crash-and-respawn would default
//! to "off" — silently reverting the operator's stated intent.
//! The M2 spec rejects that: if the operator asked for the
//! supervisor to be armed, a restart must reinstate that posture
//! and the operator's next `/headless status` must show
//! "started", not "stopped".
//!
//! # Atomic writes
//!
//! Every state mutation is written via the classic
//! *write-to-tempfile + `fsync` + `rename`* dance. Partial writes
//! on power loss would otherwise leave the daemon with
//! unparseable state, and the honest response to unparseable
//! state is to refuse to start — not a great experience when
//! the alternative is "safely lose the last change".
//!
//! # Wire format
//!
//! The on-disk format is deliberately the same JSON shape as
//! the IPC protocol's state envelope. An operator who wants to
//! see why the daemon started the way it did can just
//! `cat ~/.zero/state/headless.json` instead of learning a
//! second schema.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{ActionKind, ActionRecord, SupervisorState};

/// How many recent actions to keep on disk. Spec wants
/// `/headless status` to be able to show the last 3 under
/// `headless_recent_actions`; we keep one extra slot as cheap
/// context for debugging without turning the state file into
/// an append-only log.
pub const MAX_RECENT_ACTIONS: usize = 4;

/// Persistent state shape. Kept intentionally small so the
/// file is human-grippable under `cat`. Additions that don't
/// alter the supervisor's recovery behaviour should stay out
/// of this struct and into a sibling file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    /// Operator-intent: should the supervisor consider itself
    /// "armed" on boot.
    pub intent: SupervisorState,
    /// Monotonically-updated bookkeeping timestamps.
    pub last_updated: DateTime<Utc>,
    /// Ring buffer of the most recent decisions, newest first.
    /// Capped at [`MAX_RECENT_ACTIONS`] on every write.
    pub recent_actions: Vec<ActionRecord>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            intent: SupervisorState::Off,
            last_updated: Utc::now(),
            recent_actions: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("state i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("state file at {path} is unparseable: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl State {
    /// Load from disk, or return `Default` if the file does not
    /// exist yet. An *unparseable* file is an error, not a
    /// silent reset — the daemon refuses to run rather than
    /// forget the operator's intent.
    pub fn load(path: &Path) -> Result<Self, PersistError> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| PersistError::Parse {
                path: path.to_path_buf(),
                source,
            }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(PersistError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Persist atomically. Creates parent directories if
    /// missing; the daemon's lifecycle doesn't include a
    /// separate "install" step that would create them.
    pub fn save(&self, path: &Path) -> Result<(), PersistError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| PersistError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let tmp = sibling_tempfile(path);
        let bytes = serde_json::to_vec_pretty(self).map_err(|source| PersistError::Parse {
            path: path.to_path_buf(),
            source,
        })?;

        {
            let mut f = fs::File::create(&tmp).map_err(|source| PersistError::Io {
                path: tmp.clone(),
                source,
            })?;
            f.write_all(&bytes).map_err(|source| PersistError::Io {
                path: tmp.clone(),
                source,
            })?;
            f.sync_all().map_err(|source| PersistError::Io {
                path: tmp.clone(),
                source,
            })?;
        }

        fs::rename(&tmp, path).map_err(|source| PersistError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    /// Record an action, pushing it to the front of
    /// `recent_actions` and trimming to cap. `note` must be
    /// non-empty — the protocol forbids empty reasons and we
    /// enforce it at the persistence boundary too.
    pub fn push_action(&mut self, kind: ActionKind, note: impl Into<String>) {
        let note = note.into();
        debug_assert!(!note.is_empty(), "empty action note is a protocol lie");
        let record = ActionRecord {
            kind,
            at: Utc::now(),
            note,
        };
        self.recent_actions.insert(0, record);
        if self.recent_actions.len() > MAX_RECENT_ACTIONS {
            self.recent_actions.truncate(MAX_RECENT_ACTIONS);
        }
        self.last_updated = Utc::now();
    }

    /// Flip the intent flag and bump `last_updated`. Does not
    /// push an action — the caller decides whether that edge
    /// deserves a record (e.g. `Stop` during boot might not).
    pub fn set_intent(&mut self, intent: SupervisorState) {
        self.intent = intent;
        self.last_updated = Utc::now();
    }
}

fn sibling_tempfile(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    // Suffix rather than prefix so fs tools that sort by name
    // still group the tempfile with its target.
    name.push(".tmp");
    match path.parent() {
        Some(p) => p.join(name),
        None => PathBuf::from(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_yields_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state/headless.json");
        let s = State::load(&path).unwrap();
        assert_eq!(s.intent, SupervisorState::Off);
        assert!(s.recent_actions.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state/headless.json");
        let mut s = State::default();
        s.set_intent(SupervisorState::On);
        s.push_action(ActionKind::Started, "boot");
        s.save(&path).unwrap();

        let back = State::load(&path).unwrap();
        assert_eq!(back.intent, SupervisorState::On);
        assert_eq!(back.recent_actions.len(), 1);
        assert_eq!(back.recent_actions[0].kind, ActionKind::Started);
        assert_eq!(back.recent_actions[0].note, "boot");
    }

    #[test]
    fn recent_actions_cap_is_enforced() {
        let mut s = State::default();
        for i in 0..MAX_RECENT_ACTIONS + 3 {
            s.push_action(ActionKind::Probed, format!("probe {i}"));
        }
        assert_eq!(s.recent_actions.len(), MAX_RECENT_ACTIONS);
        assert_eq!(
            s.recent_actions[0].note,
            format!("probe {}", MAX_RECENT_ACTIONS + 2)
        );
    }

    #[test]
    fn unparseable_state_is_error_not_reset() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("headless.json");
        fs::write(&path, "{not json").unwrap();
        let err = State::load(&path).unwrap_err();
        assert!(matches!(err, PersistError::Parse { .. }));
    }

    #[test]
    fn atomic_write_leaves_no_tempfile_behind() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state/headless.json");
        State::default().save(&path).unwrap();
        let tmp = path.with_file_name("headless.json.tmp");
        assert!(!tmp.exists(), "tempfile not cleaned up: {tmp:?}");
        assert!(path.exists());
    }
}
