//! Slash-command framework and the M1 command set.
//!
//! Commands are routed by the TUI's prompt, the command palette,
//! and the non-interactive `zero <command>` entrypoint. All three
//! paths produce the same [`DispatchOutput`], which downstream
//! renders as text, JSON, or a typed widget.
//!
//! The crate enforces ADR-014 (risk asymmetry) at the type level
//! via [`risk::FrictionGate`]: only `Increases`-classified commands
//! can be friction-wrapped. Risk-reducing actions (`/quit`,
//! `/kill`, `/flatten-all`, `/pause-entries`, `/break`) are
//! instant and cannot be gated — the compiler refuses.

#![allow(clippy::module_name_repetitions)]

pub mod command;
pub mod config;
pub mod dispatch;
pub mod friction;
pub mod parse;
pub mod risk;
pub mod session;
pub mod supervisor;

pub use command::{
    AutoAction, COMMAND_CATALOG, Command, CommandInfo, HeadlessAction, ModeTarget, OverlayTarget,
    resolve,
};
pub use config::{ConfigDoctorFinding, ConfigShowRow, ConfigSource, DoctorSeverity};
pub use dispatch::{
    DispatchContext, DispatchOutput, Never, OutputLine, ReplayLine, StateSource, StaticLabel,
    dispatch, run_bypass_friction,
};
pub use friction::{
    FALLBACK_REREAD_PHRASE, FrictionDecision, TYPED_CONFIRM_WORD, decide, decide_with_risk,
};
pub use parse::{ParsedLine, parse_line};
pub use risk::{FrictionGate, Gateable, Increases, RiskDirection};
pub use session::{ReplayEvent, ReplayKind, SessionError, SessionSource, SessionSummary};
pub use supervisor::{
    AutoMode, AutoReply, AutoRequest, AutoSource, MockAutoSource, MockSupervisorSource,
    SupervisorAction, SupervisorError, SupervisorReply, SupervisorSource, SupervisorState,
};
