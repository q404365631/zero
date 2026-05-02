//! Operator-local supervisor daemon (ADR-006, M2 §6).
//!
//! `zero-headless` ships two artifacts:
//!
//! 1. **`zero-headlessd` binary** — the supervisor daemon. Runs
//!    under launchd (macOS) or systemd (Linux); listens on a
//!    Unix socket at
//!    `<zero_dir>/operators/<operator-slug>/sock` for typed line-delimited JSON
//!    requests from the `zero` CLI (and, in M3, the Telegram
//!    bot). Persists its "operator intended me to be running"
//!    state under
//!    `<zero_dir>/operators/<operator-slug>/state/headless.json` so a
//!    crash-and-restart recovers the operator's posture rather
//!    than silently defaulting to "off".
//!
//! 2. **Library surface** — the [`protocol`] envelopes, the
//!    [`client::Client`] dialer, and the [`state::State`]
//!    persistence type. The `zero-commands` crate's real
//!    `SupervisorSource` adapter (shipped in §6, not in this
//!    module) is built on top of [`client::Client`] so the
//!    shipped binary and the CLI-side dialer speak the same
//!    wire format by construction.
//!
//! # Invariants
//!
//! - **Line-delimited JSON framing.** One request per line,
//!   one response per line. Chosen over length-prefixed framing
//!   so an operator debugging a stuck daemon can
//!   `nc -U <zero_dir>/operators/<operator-slug>/sock` and read
//!   what the daemon is saying without tooling. Every envelope has
//!   a `kind` tag so the
//!   protocol is forward-extensible.
//!
//! - **Kill-switch paths are never gated.** A `/kill` from the
//!   CLI, a `SIGTERM`, or a Telegram-originated kill all reach
//!   the same `graceful_drain` path. Silence on a kill request
//!   is the 2 AM failure mode the architecture exists to
//!   prevent — the daemon logs a single-line breadcrumb for
//!   every kill attempt and honours it within a hard deadline.
//!
//! - **Three honest questions in one `status`.** The protocol's
//!   [`protocol::StatusReply`] answers (a) is the daemon
//!   running, (b) is the engine reachable from the daemon, (c)
//!   what was the daemon's most recent action. Silence on any
//!   of the three is a lie; the protocol type forces all three
//!   to be populated on every reply.
//!
//! The daemon itself does *not* make trading decisions — those
//! live in the engine. `zero-headlessd` is a watchdog + kill-
//! switch surface + structured keepalive; it exists so an
//! operator who closes their laptop still has a path to halt
//! the engine from a phone (via the M3 Telegram bot) or a
//! second terminal (via `zero kill`, §8).

#![allow(clippy::module_name_repetitions)]

pub mod client;
pub mod daemon;
pub mod paths;
pub mod protocol;
pub mod state;

pub use client::{Client, ClientError};
pub use paths::{default_socket_path, default_state_path};
pub use protocol::{
    ActionKind, ActionRecord, EngineHealth, PROTOCOL_VERSION, Request, Response, StatusReply,
    SupervisorState,
};
pub use state::{PersistError, State};
