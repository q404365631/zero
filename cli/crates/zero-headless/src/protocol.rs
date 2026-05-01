//! Line-delimited JSON IPC protocol between the `zero` CLI and
//! the `zero-headlessd` daemon.
//!
//! # Framing
//!
//! One JSON object per line. The newline `\n` is the record
//! separator; no embedded newlines are permitted in the payload
//! (serde_json's default emitter satisfies this). Line framing
//! was chosen over length-prefixed framing so that an operator
//! debugging a stuck daemon can run
//!
//! ```sh
//! { echo '{"kind":"status"}'; sleep 0.2; } | nc -U ~/.zero/sock
//! ```
//!
//! and see the reply without needing a framing parser. The M2
//! spec treats this as a hard operability requirement, not a
//! nice-to-have.
//!
//! # Extensibility
//!
//! Each envelope is `#[serde(tag = "kind", rename_all = …)]`
//! so adding a new variant is additive: old daemons will
//! reject unknown kinds with a structured `Response::Error`
//! rather than silently accepting them. The protocol version
//! is bumped [`PROTOCOL_VERSION`] whenever the wire format
//! changes in a non-additive way — matching versions is the
//! dialer's responsibility.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Protocol version. Bump on non-additive changes (removing
/// variants, changing field types). Additive changes (new
/// variants, new optional fields) do not require a bump, but
/// the daemon must tolerate unknown additive fields from
/// newer clients without erroring.
pub const PROTOCOL_VERSION: u32 = 1;

/// Request frame sent by the CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    /// Health + state probe. Always safe — the daemon must
    /// answer even while draining.
    Status,
    /// Arm the operator-intent flag (the daemon stays up; the
    /// flag persists across restarts so a crash-and-respawn
    /// recovers the operator's posture).
    Start,
    /// Disarm the operator-intent flag. Does *not* shut the
    /// daemon process down; use [`Request::Kill`] for that.
    /// Keeping start/stop and kill distinct lets an operator
    /// say "disarm but stay reachable" without burning the
    /// launchd/systemd supervisor's restart budget.
    Stop,
    /// Immediate kill-switch: graceful drain then exit. This
    /// is the path `/kill` + SIGTERM funnel through. Never
    /// gated, never silently dropped.
    Kill,
}

/// Response frame returned by the daemon. `Status` is the only
/// variant that carries a non-trivial payload; the others are
/// ack/error shapes so a misbehaving daemon cannot pretend to
/// have acted when it has not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    /// Three-questions answer from [`StatusReply`].
    Status(StatusReply),
    /// Request accepted; daemon will (or has) acted. Carries
    /// the post-action state so the CLI can render it without
    /// a follow-up round-trip.
    Accepted {
        state: SupervisorState,
        protocol_version: u32,
    },
    /// Request understood but refused. Carries a single-line
    /// operator-readable reason — never the empty string.
    Refused { reason: String },
    /// Request malformed or unsupported. Dialer should surface
    /// this verbatim; this is the "I don't know that verb"
    /// path, not the "I tried and failed" path.
    Error { reason: String },
}

/// Status payload — the spec's three honest questions.
///
/// 1. `state` — is the operator-intent flag on?
/// 2. `engine` — can the daemon see the engine right now?
/// 3. `latest_action` — what was my most recent decision?
///
/// All three fields are mandatory on the wire. A `None` on
/// `latest_action` means "I haven't acted yet since boot",
/// which is a truthful answer; it is *not* the same as "I'm
/// suppressing this field". Structural honesty beats
/// convenience here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusReply {
    pub state: SupervisorState,
    pub engine: EngineHealth,
    pub latest_action: Option<ActionRecord>,
    pub protocol_version: u32,
}

/// Operator-intent flag — what the operator *asked* the daemon
/// to do, not whether the daemon's socket is currently
/// listening (if you're receiving a reply, it is).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorState {
    On,
    Off,
}

/// Engine reachability from the daemon's perspective.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineHealth {
    /// Daemon has never probed — state file said "off" since
    /// boot and no Start request has arrived. The honest
    /// answer under those conditions is "unknown".
    Unknown,
    /// Most recent probe succeeded at `at`.
    Reachable { at: DateTime<Utc> },
    /// Most recent probe failed at `at` with `reason`.
    Unreachable { at: DateTime<Utc>, reason: String },
}

/// One durable record of a daemon decision. `kind` is a
/// coarse-grained enum so `/headless status` can paint a terse
/// "last 3 decisions" line without loading the full log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionRecord {
    pub kind: ActionKind,
    pub at: DateTime<Utc>,
    /// One-line operator-readable summary. Must be non-empty.
    pub note: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Arming succeeded.
    Started,
    /// Disarming succeeded.
    Stopped,
    /// Kill switch fired — daemon is draining.
    Killed,
    /// Probe result observed (reachable or unreachable).
    Probed,
    /// Refusal recorded (e.g. missing config).
    Refused,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_through_json() {
        for req in [
            Request::Status,
            Request::Start,
            Request::Stop,
            Request::Kill,
        ] {
            let line = serde_json::to_string(&req).unwrap();
            assert!(!line.contains('\n'), "line framing broken: {line}");
            let back: Request = serde_json::from_str(&line).unwrap();
            assert_eq!(req, back);
        }
    }

    #[test]
    fn status_reply_answers_three_questions() {
        let reply = StatusReply {
            state: SupervisorState::On,
            engine: EngineHealth::Unknown,
            latest_action: None,
            protocol_version: PROTOCOL_VERSION,
        };
        let line = serde_json::to_string(&reply).unwrap();
        // All three fields are on the wire — even the `None`
        // latest_action is explicitly `null`, not dropped.
        assert!(line.contains("\"state\""));
        assert!(line.contains("\"engine\""));
        assert!(line.contains("\"latest_action\""));
    }

    #[test]
    fn unknown_kind_fails_to_deserialize() {
        // Old daemon, new verb → explicit error, not silent accept.
        let bogus = "{\"kind\":\"detonate\"}";
        let parsed: Result<Request, _> = serde_json::from_str(bogus);
        assert!(parsed.is_err());
    }

    #[test]
    fn protocol_version_is_one() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }
}
