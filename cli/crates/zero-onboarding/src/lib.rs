//! First-run onboarding wizard — the "Honest Welcome" of
//! spec v2.1 §11.
//!
//! The wizard collects the operator's handle, the engine URL,
//! and an auth token; probes the engine for reachability;
//! displays a plan; and on confirmation writes
//! `~/.zero/config.toml`, stores the token in the OS keychain,
//! and stamps a `welcome_shown` milestone in the session store.
//!
//! Design notes:
//!
//! - The wizard is line-based (not ratatui). It runs before the
//!   TUI exists.
//! - All I/O is funnelled through the [`Prompt`] trait. Tests
//!   script answers via `MockPrompt`.
//! - The wizard never writes on its own — it produces a [`Plan`]
//!   that a caller commits via [`Plan::apply`]. This split
//!   keeps dry-run easy and tests fast.

#![allow(clippy::module_name_repetitions)]

pub mod plan;
pub mod prompt;
pub mod wizard;

pub use plan::{Plan, Receipt};
pub use prompt::{Prompt, StdioPrompt};
pub use wizard::{Flags, run_interactive, run_non_interactive};

use thiserror::Error;

/// Errors raised by the wizard and its helpers.
#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("validation: {0}")]
    Validation(String),
    #[error("config: {0}")]
    Config(String),
    #[error("session: {0}")]
    Session(String),
}

impl From<zero_config::ConfigError> for Error {
    fn from(e: zero_config::ConfigError) -> Self {
        Self::Config(e.to_string())
    }
}

/// Legacy step-enum kept for compatibility with older docs. Will
/// be removed once `M0_ADR.md` and the spec diff note the new
/// three-step flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Handle,
    EngineUrl,
    Token,
    Probe,
    Confirm,
}
