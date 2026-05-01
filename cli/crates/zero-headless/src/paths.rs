//! Filesystem path helpers for the daemon and its clients.
//!
//! Both the daemon and the CLI-side dialer need to agree on
//! *where* the socket and state file live; isolating the logic
//! here means a future move (e.g. `/var/run/zero.sock` on Linux,
//! `~/Library/Application Support/getzero/zero/sock` on macOS)
//! is a single-file change instead of a grep-and-replace.
//!
//! The defaults land under [`zero_config::zero_dir`] — the same
//! `~/.zero` directory the rest of the CLI uses. We intentionally
//! *do not* put the socket under `XDG_RUNTIME_DIR` on Linux,
//! because that directory is lost across login sessions and the
//! daemon's whole purpose is surviving the operator's terminal.

use std::path::{Path, PathBuf};

/// Socket file name under the zero directory. Short enough to
/// fit comfortably in `nc -U ~/.zero/sock` muscle memory.
pub const SOCKET_FILE_NAME: &str = "sock";

/// State-file path relative to the zero directory.
/// `state/headless.json` keeps the daemon's persistence sibling
/// to future `state/*.json` files (session DB already lives
/// under `~/.zero/state.db`; we deliberately namespace the new
/// files under a `state/` subdir to avoid crowding the top
/// level).
pub const STATE_SUBDIR: &str = "state";
pub const STATE_FILE_NAME: &str = "headless.json";

/// Default Unix socket path: `<zero_dir>/sock`. Falls back to
/// `<tmpdir>/zero-sock` when `zero_dir()` is unavailable (rare —
/// the CLI refuses to launch without it), so the daemon can at
/// least start for a smoke test under CI with no home dir.
#[must_use]
pub fn default_socket_path() -> PathBuf {
    zero_config::zero_dir().map_or_else(
        |_| std::env::temp_dir().join("zero-sock"),
        |d| d.join(SOCKET_FILE_NAME),
    )
}

/// Default state-file path: `<zero_dir>/state/headless.json`.
#[must_use]
pub fn default_state_path() -> PathBuf {
    zero_config::zero_dir().map_or_else(
        |_| std::env::temp_dir().join("zero-headless.json"),
        |d| d.join(STATE_SUBDIR).join(STATE_FILE_NAME),
    )
}

/// Socket path rooted at an arbitrary base — used by tests to
/// point both the daemon and its dialer at a temp dir.
#[must_use]
pub fn socket_in(base: &Path) -> PathBuf {
    base.join(SOCKET_FILE_NAME)
}

/// State-file path rooted at an arbitrary base. Mirrors the
/// production layout so tests exercise the same
/// `state/headless.json` resolution as prod.
#[must_use]
pub fn state_in(base: &Path) -> PathBuf {
    base.join(STATE_SUBDIR).join(STATE_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_and_state_share_base() {
        let base = std::path::Path::new("/tmp/zero-test");
        assert_eq!(socket_in(base), PathBuf::from("/tmp/zero-test/sock"));
        assert_eq!(
            state_in(base),
            PathBuf::from("/tmp/zero-test/state/headless.json"),
        );
    }

    #[test]
    fn defaults_are_absolute_or_fallback() {
        // Either the HOME-rooted default resolved or the tmpdir
        // fallback fired — both yield absolute paths. An empty
        // / relative result would be a regression.
        let s = default_socket_path();
        assert!(s.is_absolute(), "socket path not absolute: {s:?}");
        let p = default_state_path();
        assert!(p.is_absolute(), "state path not absolute: {p:?}");
    }
}
