//! Filesystem path helpers for the daemon and its clients.
//!
//! Both the daemon and the CLI-side dialer need to agree on
//! *where* the socket and state file live; isolating the logic
//! here means a future move (e.g. `/var/run/zero/<operator>.sock`
//! on Linux) is a single-file change instead of a grep-and-replace.
//!
//! The defaults land under [`zero_config::runtime_paths`]. We
//! intentionally *do not* put the socket under `XDG_RUNTIME_DIR`
//! on Linux, because that directory is lost across login sessions
//! and the daemon's whole purpose is surviving the operator's
//! terminal.

use std::path::{Path, PathBuf};

/// Socket file name under the operator directory.
pub const SOCKET_FILE_NAME: &str = "sock";

/// State-file path relative to the operator directory.
/// `state/headless.json` keeps daemon persistence under the same
/// operator partition as session state and wraps.
pub const STATE_SUBDIR: &str = "state";
pub const STATE_FILE_NAME: &str = "headless.json";

/// Default Unix socket path:
/// `<zero_dir>/operators/<operator-slug>/sock`. Falls back to
/// `<tmpdir>/zero-sock` when config paths are unavailable.
#[must_use]
pub fn default_socket_path() -> PathBuf {
    zero_config::runtime_paths().map_or_else(
        |_| std::env::temp_dir().join("zero-sock"),
        |paths| paths.headless_socket_path,
    )
}

/// Default state-file path:
/// `<zero_dir>/operators/<operator-slug>/state/headless.json`.
#[must_use]
pub fn default_state_path() -> PathBuf {
    zero_config::runtime_paths().map_or_else(
        |_| std::env::temp_dir().join("zero-headless.json"),
        |paths| paths.headless_state_path,
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
