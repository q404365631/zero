//! Supervisor + Auto-mode dispatch surfaces.
//!
//! The `zero-commands` crate does not know how the headless
//! supervisor or the engine's Auto-mode flag are actually
//! implemented; it only needs to know there is a typed seam for
//! **asking** for a specific action. The adapters in
//! `crates/zero/src/main.rs` (for the engine-facing [`AutoSource`])
//! and in `zero-headless` (for the daemon-facing
//! [`SupervisorSource`]) turn those asks into concrete work —
//! an HTTP call, a launchd/systemd spawn, a Unix-socket probe.
//!
//! Keeping the traits here (rather than in an adapter crate)
//! means the dispatcher, the tests, and the TUI adapters all
//! agree on the wire shape of the "request" and the "reply" by
//! construction. ADR-006 and M2_PLAN §5 are the source of
//! truth for the verbs.

use std::error::Error;
use std::fmt;
use std::time::Duration;

/// A command issued to the engine's Auto-mode switch.
///
/// `Status` is read-only — the adapter reports the current mode
/// without mutating engine state. `On` / `Off` are the mutating
/// verbs; the friction ladder has already gated `On` by the time
/// the adapter sees the request (Phase-2: `On` is Increases, `Off`
/// / `Status` are Neutral — see `Command::risk`).
///
/// Distinct from [`crate::command::AutoAction`] (the user-typed
/// subcommand which also carries `Missing` / `Unknown` for usage
/// hints) — this enum is the *resolved* adapter request, so it
/// only carries verbs the adapter can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoRequest {
    On,
    Off,
    Status,
}

/// Reply from an [`AutoSource`] call. `mode` is the effective mode
/// after the adapter acted (so `On` returns `AutoMode::On` on
/// success, `AutoMode::Off` on adapter-side refusal), and
/// `changed` is `true` when the call flipped the mode. The
/// dispatcher renders different lines for the two cases so the
/// operator is never left guessing whether the toggle actually
/// did something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoReply {
    pub mode: AutoMode,
    pub changed: bool,
}

/// Observed Auto-mode state. Mirrors the engine's two-state
/// switch; no `Paused` / `Transitioning` intermediate — the
/// engine either accepts Plan verdicts or it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoMode {
    On,
    Off,
}

impl AutoMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

/// A command issued to the operator-local supervisor daemon.
///
/// `Start` asks the adapter to spawn the daemon (idempotent — a
/// second `Start` on an already-running daemon is a no-op, the
/// dispatcher surfaces a "already running" line). `Stop` asks the
/// adapter to signal the daemon to exit and tear down the
/// listener socket. `Status` reports whether the daemon is alive
/// and — if so — what pid + socket path it is using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorAction {
    Start,
    Stop,
    Status,
}

/// Reply from a [`SupervisorSource`] call. `state` is the
/// observed state after the adapter acted. `socket` is the
/// daemon's Unix socket path when the daemon is running (always
/// `~/.zero/sock` in production; tests stub this to any path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorReply {
    pub state: SupervisorState,
    pub socket: Option<String>,
    pub pid: Option<u32>,
    /// `true` when the call changed daemon state (started a
    /// stopped daemon, stopped a running one). Lets the dispatch
    /// layer render "headless: started" vs "already running" off
    /// a single reply shape.
    pub changed: bool,
    /// Monotonic uptime when the daemon is running; `None`
    /// otherwise. Rendered in `/headless status` lines.
    pub uptime: Option<Duration>,
}

/// Observed daemon lifecycle state.
///
/// `Running` / `Stopped` are the steady states. `Failed` carries
/// a short free-form reason so a `/headless status` line can
/// distinguish a clean stop from a crash — silent conflation would
/// hide the 2 AM case where the daemon died in the night.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorState {
    Running,
    Stopped,
    Failed(String),
}

/// Errors an adapter can return.
///
/// Typed (rather than `Box<dyn Error>`) so the dispatcher can
/// choose different copy for each class without string-matching
/// on message bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    /// Adapter is not configured for this invocation (no daemon
    /// binary installed, or `--no-headless` flag set).
    Unavailable(String),
    /// Transport-level failure — socket gone, permission denied,
    /// pipe closed mid-call.
    Io(String),
    /// The daemon refused the request (e.g. asked to stop while
    /// already stopping). Rendered as a warn line, not an alert —
    /// the call was understood, just not honored.
    Refused(String),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(s) => write!(f, "supervisor unavailable: {s}"),
            Self::Io(s) => write!(f, "supervisor IO error: {s}"),
            Self::Refused(s) => write!(f, "supervisor refused: {s}"),
        }
    }
}

impl Error for SupervisorError {}

/// Dispatcher-side handle to the operator-local supervisor.
///
/// Implemented by the `zero-headless` adapter (production), by
/// `crates/zero/src/main.rs` when stubbed (current-M2 CLI has
/// no daemon binary yet), and by test scaffolding. When no
/// adapter is attached to [`crate::DispatchContext`] the
/// dispatcher emits a single "headless supervisor unavailable"
/// alert rather than hanging — same honesty contract as
/// [`crate::SessionSource`] on `--no-persist`.
pub trait SupervisorSource: Send + Sync + 'static {
    /// Issue an action to the supervisor.
    ///
    /// # Errors
    /// Propagates adapter-specific failures — see
    /// [`SupervisorError`].
    fn act(&self, action: SupervisorAction) -> Result<SupervisorReply, SupervisorError>;

    /// Tear down the daemon's listener socket as part of a
    /// `/kill`. Idempotent: when the daemon is already stopped
    /// this returns `Ok(false)` and the dispatch layer treats
    /// that as "no tear-down needed". When it did tear one
    /// down, returns `Ok(true)` so the `/kill` line can tag
    /// the compound behavior.
    ///
    /// # Errors
    /// Propagates adapter-specific failures.
    fn tear_down_socket(&self) -> Result<bool, SupervisorError>;
}

/// Dispatcher-side handle to the engine's Auto-mode switch.
///
/// Production impl lives in the `zero-engine-client`-aware
/// adapter in `crates/zero/src/main.rs`. Tests use
/// [`MockAutoSource`]. When no adapter is attached the
/// dispatcher surfaces "auto mode unavailable" rather than
/// pretending — same pattern as every other optional source on
/// [`crate::DispatchContext`].
pub trait AutoSource: Send + Sync + 'static {
    /// Issue an Auto-mode action.
    ///
    /// # Errors
    /// Same taxonomy as [`SupervisorError`] (transport, refusal,
    /// unavailability) — re-using the type avoids a parallel
    /// error enum for an isomorphic surface.
    fn act(&self, action: AutoRequest) -> Result<AutoReply, SupervisorError>;
}

/// In-memory [`AutoSource`] used by tests and offline paths.
/// Flips the stored mode on `On` / `Off`, returns it on
/// `Status`. `changed` is computed by comparing the requested
/// action against the current mode, matching the production
/// adapter's contract.
#[derive(Debug)]
pub struct MockAutoSource {
    mode: std::sync::Mutex<AutoMode>,
}

impl MockAutoSource {
    #[must_use]
    pub fn new(initial: AutoMode) -> Self {
        Self {
            mode: std::sync::Mutex::new(initial),
        }
    }

    /// Current mode. Handy for assertions.
    #[must_use]
    pub fn current(&self) -> AutoMode {
        *self
            .mode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for MockAutoSource {
    fn default() -> Self {
        Self::new(AutoMode::Off)
    }
}

impl AutoSource for MockAutoSource {
    fn act(&self, action: AutoRequest) -> Result<AutoReply, SupervisorError> {
        let mut guard = self
            .mode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prior = *guard;
        let (mode, changed) = match action {
            AutoRequest::On => (AutoMode::On, prior != AutoMode::On),
            AutoRequest::Off => (AutoMode::Off, prior != AutoMode::Off),
            AutoRequest::Status => (prior, false),
        };
        *guard = mode;
        Ok(AutoReply { mode, changed })
    }
}

/// In-memory [`SupervisorSource`] used by tests and the M2
/// CLI's own "no daemon yet" path. Tracks daemon state as a
/// boolean and reports a stubbed socket path on `Start` /
/// `Status` so the dispatcher copy has something concrete to
/// print.
#[derive(Debug)]
pub struct MockSupervisorSource {
    inner: std::sync::Mutex<MockSupervisorInner>,
}

#[derive(Debug)]
struct MockSupervisorInner {
    running: bool,
    socket: String,
    pid: u32,
    started_at: std::time::Instant,
    /// Simulate the daemon having torn itself down on a prior
    /// `/kill`. Purely for test ergonomics.
    socket_torn_down: bool,
}

impl MockSupervisorSource {
    #[must_use]
    pub fn new(running: bool) -> Self {
        Self {
            inner: std::sync::Mutex::new(MockSupervisorInner {
                running,
                socket: "~/.zero/sock".to_owned(),
                pid: 4242,
                started_at: std::time::Instant::now(),
                socket_torn_down: false,
            }),
        }
    }

    /// Observed running-state. Handy for assertions.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .running
    }

    /// Did any prior call tear down the socket? Lets
    /// `/kill`-compound tests assert the behavior happened
    /// without exposing internal state on every reply.
    #[must_use]
    pub fn socket_torn_down(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .socket_torn_down
    }
}

impl Default for MockSupervisorSource {
    fn default() -> Self {
        Self::new(false)
    }
}

impl SupervisorSource for MockSupervisorSource {
    fn act(&self, action: SupervisorAction) -> Result<SupervisorReply, SupervisorError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match action {
            SupervisorAction::Start => {
                let changed = !inner.running;
                if changed {
                    inner.running = true;
                    inner.started_at = std::time::Instant::now();
                    inner.socket_torn_down = false;
                }
                Ok(SupervisorReply {
                    state: SupervisorState::Running,
                    socket: Some(inner.socket.clone()),
                    pid: Some(inner.pid),
                    changed,
                    uptime: Some(inner.started_at.elapsed()),
                })
            }
            SupervisorAction::Stop => {
                let changed = inner.running;
                inner.running = false;
                Ok(SupervisorReply {
                    state: SupervisorState::Stopped,
                    socket: None,
                    pid: None,
                    changed,
                    uptime: None,
                })
            }
            SupervisorAction::Status => {
                if inner.running {
                    Ok(SupervisorReply {
                        state: SupervisorState::Running,
                        socket: Some(inner.socket.clone()),
                        pid: Some(inner.pid),
                        changed: false,
                        uptime: Some(inner.started_at.elapsed()),
                    })
                } else {
                    Ok(SupervisorReply {
                        state: SupervisorState::Stopped,
                        socket: None,
                        pid: None,
                        changed: false,
                        uptime: None,
                    })
                }
            }
        }
    }

    fn tear_down_socket(&self) -> Result<bool, SupervisorError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.running {
            inner.running = false;
            inner.socket_torn_down = true;
            Ok(true)
        } else {
            // Already stopped — nothing to tear down. The compound
            // `/kill` line still renders; it just omits the
            // headless tag.
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AutoMode, AutoRequest, AutoSource, MockAutoSource, MockSupervisorSource, SupervisorAction,
        SupervisorSource, SupervisorState,
    };

    #[test]
    fn mock_auto_flips_on_then_is_idempotent() {
        let src = MockAutoSource::new(AutoMode::Off);
        let first = src.act(AutoRequest::On).unwrap();
        assert!(first.changed);
        assert_eq!(first.mode, AutoMode::On);
        let again = src.act(AutoRequest::On).unwrap();
        assert!(!again.changed);
        assert_eq!(again.mode, AutoMode::On);
    }

    #[test]
    fn mock_auto_status_is_pure() {
        let src = MockAutoSource::new(AutoMode::On);
        let reply = src.act(AutoRequest::Status).unwrap();
        assert!(!reply.changed);
        assert_eq!(reply.mode, AutoMode::On);
        assert_eq!(src.current(), AutoMode::On);
    }

    #[test]
    fn mock_supervisor_start_then_status_reports_running() {
        let src = MockSupervisorSource::new(false);
        let started = src.act(SupervisorAction::Start).unwrap();
        assert!(started.changed);
        assert_eq!(started.state, SupervisorState::Running);
        let status = src.act(SupervisorAction::Status).unwrap();
        assert!(!status.changed);
        assert_eq!(status.state, SupervisorState::Running);
        assert_eq!(status.socket.as_deref(), Some("~/.zero/sock"));
    }

    #[test]
    fn mock_supervisor_tear_down_only_when_running() {
        let src = MockSupervisorSource::new(true);
        assert!(src.tear_down_socket().unwrap());
        assert!(!src.is_running());
        assert!(src.socket_torn_down());
        // Second call is a no-op.
        assert!(!src.tear_down_socket().unwrap());
    }
}
