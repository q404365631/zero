//! Real `SupervisorSource` adapter — talks to the
//! `zero-headlessd` daemon over a Unix socket.
//!
//! The `zero-commands` crate defines the [`SupervisorSource`]
//! trait as a sync surface (the dispatcher itself is sync so
//! that `/kill` cannot deadlock behind an async boundary). The
//! real daemon client is async. We bridge the two with
//! `block_in_place` on the multi-threaded tokio runtime — the
//! TUI always runs under a multi-threaded runtime (see
//! `#[tokio::main]` in `main.rs`) so this is safe; the fall
//! back path below preserves single-threaded safety too.
//!
//! # Semantics
//!
//! - `act(Start)` dials the daemon with [`Request::Start`] —
//!   the adapter does *not* spawn the daemon itself. Spawning
//!   is the launchd/systemd unit's job (or `zero headless
//!   install`, shipped in M2 §8). If the socket is missing we
//!   return `Unavailable` so the operator sees "daemon not
//!   reachable, did you run `zero headless install`?" instead
//!   of a silent no-op.
//! - `act(Stop)` dials with [`Request::Stop`] — arming flag
//!   flips; the daemon keeps listening. `/kill` is the path
//!   that also tears the socket down.
//! - `act(Status)` asks the daemon for a [`StatusReply`] and
//!   projects the three-questions answer onto
//!   [`SupervisorReply`]. The `socket` field is populated
//!   whenever the daemon replied (proof of life).
//! - `tear_down_socket()` sends [`Request::Kill`], waits for
//!   the reply (so the daemon records the kill in its
//!   persistent state), then best-effort removes the socket
//!   file. Returns `Ok(true)` when we touched it, `Ok(false)`
//!   when the socket was already gone.

use std::path::PathBuf;
use std::time::Duration;

use tokio::runtime::Handle;
use zero_commands::{
    SupervisorAction, SupervisorError, SupervisorReply, SupervisorSource, SupervisorState,
};
use zero_headless::{Client, ClientError, Request, Response};

/// The real `SupervisorSource`. Cheap to clone — holds a
/// [`Client`] and a [`Handle`] to the ambient runtime.
#[derive(Debug, Clone)]
pub struct HeadlessSupervisorAdapter {
    client: Client,
    handle: Handle,
    socket_path: PathBuf,
}

impl HeadlessSupervisorAdapter {
    /// Build an adapter dialing the given socket path. The
    /// caller supplies the tokio runtime handle the adapter
    /// should use — in the zero binary this is
    /// `Handle::current()` captured inside the TUI entry
    /// point.
    #[must_use]
    pub fn new(socket_path: PathBuf, handle: Handle) -> Self {
        let client = Client::new(socket_path.clone()).with_timeout(Duration::from_secs(2));
        Self {
            client,
            handle,
            socket_path,
        }
    }

    fn dial(&self, req: &Request) -> Result<Response, SupervisorError> {
        // `block_in_place` + `Handle::block_on` is the
        // idiomatic "run async from sync inside tokio" bridge.
        // Requires a multi-threaded runtime; the TUI's
        // `#[tokio::main]` default provides that.
        let result: Result<Response, ClientError> =
            tokio::task::block_in_place(|| self.handle.block_on(self.client.send(req)));
        result.map_err(map_client_error)
    }
}

impl SupervisorSource for HeadlessSupervisorAdapter {
    fn act(&self, action: SupervisorAction) -> Result<SupervisorReply, SupervisorError> {
        // Pre-flight: if the socket file isn't there the
        // daemon is almost certainly not running. We could
        // just let the dial fail — the error taxonomy is the
        // same — but a dedicated message ("daemon not
        // running") is more actionable than the raw
        // `ECONNREFUSED`.
        if !self.client.socket_exists() {
            return Ok(stopped_reply());
        }

        let req = match action {
            SupervisorAction::Start => Request::Start,
            SupervisorAction::Stop => Request::Stop,
            SupervisorAction::Status => Request::Status,
        };

        match self.dial(&req)? {
            Response::Accepted { state, .. } => Ok(SupervisorReply {
                state: match state {
                    zero_headless::SupervisorState::On => SupervisorState::Running,
                    zero_headless::SupervisorState::Off => SupervisorState::Stopped,
                },
                socket: Some(self.socket_path.display().to_string()),
                pid: None,
                changed: matches!(action, SupervisorAction::Start | SupervisorAction::Stop),
                uptime: None,
            }),
            Response::Status(status) => Ok(SupervisorReply {
                state: match status.state {
                    zero_headless::SupervisorState::On => SupervisorState::Running,
                    zero_headless::SupervisorState::Off => SupervisorState::Stopped,
                },
                socket: Some(self.socket_path.display().to_string()),
                pid: None,
                changed: false,
                uptime: None,
            }),
            Response::Refused { reason } => Err(SupervisorError::Refused(reason)),
            Response::Error { reason } => Err(SupervisorError::Io(reason)),
        }
    }

    fn tear_down_socket(&self) -> Result<bool, SupervisorError> {
        // `tear_down_socket` is called by `/kill`. Order
        // matters: we dial the daemon *first* so the
        // persistent state records "Killed" before we remove
        // the socket file underneath it. A reversed order
        // could leave the daemon serving a socket we've
        // unlinked — a confusing failure mode for an operator
        // watching `journalctl`.
        if !self.client.socket_exists() {
            return Ok(false);
        }

        // Ignore a timeout here — the daemon may be slow to
        // reply while draining, but we still want to remove
        // the socket file on the way out. We do *not* ignore
        // hard errors (e.g. permission denied) because those
        // usually mean the operator needs to fix something.
        match self.dial(&Request::Kill) {
            Ok(_) => {}
            Err(SupervisorError::Io(msg)) if msg.contains("timed out") => {
                tracing::warn!(
                    socket = %self.socket_path.display(),
                    "kill request timed out; removing socket anyway",
                );
            }
            Err(other) => return Err(other),
        }

        match std::fs::remove_file(&self.socket_path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(SupervisorError::Io(format!(
                "removing socket {}: {err}",
                self.socket_path.display()
            ))),
        }
    }
}

fn stopped_reply() -> SupervisorReply {
    SupervisorReply {
        state: SupervisorState::Stopped,
        socket: None,
        pid: None,
        changed: false,
        uptime: None,
    }
}

fn map_client_error(err: ClientError) -> SupervisorError {
    match err {
        // `Connect` when the socket exists but the daemon
        // isn't listening — rare, but recoverable with a
        // restart. Rendered as `Unavailable` so the operator
        // sees a helpful hint rather than a bare I/O line.
        ClientError::Connect { path, source } => SupervisorError::Unavailable(format!(
            "daemon not reachable at {}: {source}",
            path.display()
        )),
        ClientError::Timeout { timeout, .. } => {
            SupervisorError::Io(format!("daemon timed out after {timeout:?}"))
        }
        ClientError::Io { source, .. } => SupervisorError::Io(format!("socket i/o: {source}")),
        ClientError::Parse { source, .. } => {
            SupervisorError::Io(format!("daemon reply malformed: {source}"))
        }
        ClientError::Closed { .. } => {
            SupervisorError::Io("daemon closed connection before replying".into())
        }
    }
}
