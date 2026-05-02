//! The wizard's output: a [`Plan`] that is pure-data and can be
//! applied, inspected, or serialised without touching the network.
//!
//! The wizard *never* touches the filesystem or the keychain
//! directly. It produces a `Plan`; a caller (`Plan::apply`)
//! chooses what to commit. This split keeps the wizard testable
//! and lets a future dry-run mode echo the plan without altering
//! anything.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zero_config::Config;

use crate::Error;

/// A completed onboarding plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Config to be written to `~/.zero/config.toml`.
    pub config: Config,
    /// Engine URL the operator confirmed. Stored in the config
    /// *and* used as the engine `base_url` in this session.
    pub api_url: String,
    /// Operator token to be persisted in the OS keychain. `None`
    /// means "no token was provided"; the caller decides whether
    /// that is acceptable (e.g. Plan mode against a localhost
    /// engine that does not require auth).
    pub token: Option<String>,
    /// Whether engine reachability was confirmed. `false` does
    /// not fail the wizard — the operator may want to set up
    /// config before the engine is running — but it is surfaced
    /// so the caller can decide whether to print a warning.
    pub engine_reachable: bool,
    /// Timestamp at which this plan was generated. Used to stamp
    /// the `welcome_shown` milestone.
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// Where `Plan::apply` wrote things. Returned so the binary can
/// print an honest receipt.
#[derive(Debug, Clone)]
pub struct Receipt {
    pub config_path: PathBuf,
    pub token_in_keychain: bool,
    pub welcome_recorded: bool,
}

impl Plan {
    /// Commit the plan to disk and keychain.
    ///
    /// Failure modes are surfaced as structured errors; partial
    /// commits are avoided where possible by ordering the writes
    /// so the operator is never left in a half-configured state
    /// that makes recovery harder than re-running `zero init`.
    ///
    /// # Errors
    /// Returns `Error::Config` on config write problems and
    /// `Error::Session` on session-store problems.
    pub fn apply(&self) -> Result<Receipt, Error> {
        // 1. Write config first — this is the lightest write and
        //    the one most likely to succeed. If it fails, we have
        //    changed nothing.
        let config_path = zero_config::save_config(&self.config)?;

        // 2. Store the token in the keychain. If this fails, the
        //    config is already written but useless without a
        //    token; we surface the error and let the operator
        //    retry via `zero pair`.
        let token_in_keychain = if let Some(token) = &self.token {
            zero_config::keyring_store_engine_token(token).map_err(|e| {
                Error::Config(format!(
                    "wrote {path} but keychain store failed: {e}",
                    path = config_path.display()
                ))
            })?;
            true
        } else {
            false
        };

        // 3. Record the welcome milestone. Best-effort: a failure
        //    here does not invalidate the onboarding — the
        //    operator is already configured, they just may see the
        //    welcome copy a second time. We report the status on
        //    the receipt rather than erroring.
        let welcome_recorded = record_welcome(self.generated_at).is_ok();

        Ok(Receipt {
            config_path,
            token_in_keychain,
            welcome_recorded,
        })
    }

    /// Describe the plan in a handful of lines suitable for a
    /// confirmation prompt. Deliberately terse — the operator
    /// should be able to read it at a glance.
    #[must_use]
    pub fn summary(&self) -> String {
        let auth = if self.token.is_some() {
            "with token"
        } else {
            "no token"
        };
        let reach = if self.engine_reachable {
            "reachable"
        } else {
            "unreachable"
        };
        format!(
            "handle: {handle}\n\
             engine: {api} ({reach}, {auth})\n\
             mode:   {mode}\n\
             guardrails: max-position {mp:.1}% · daily-loss {dl:.1}% · max-drawdown {dd:.1}% · max-concurrent {mc}",
            handle = self.config.identity.handle,
            api = self.api_url,
            mode = self.config.mode.default,
            mp = self.config.guardrails.max_position_pct,
            dl = self.config.guardrails.daily_loss_pct,
            dd = self.config.guardrails.drawdown_pct,
            mc = self.config.guardrails.max_concurrent,
        )
    }
}

fn record_welcome(at: chrono::DateTime<chrono::Utc>) -> Result<(), Error> {
    let paths =
        zero_config::runtime_paths().map_err(|e| Error::Session(format!("runtime paths: {e}")))?;
    let db_path = paths.state_db_path;
    let store =
        zero_session::Store::open(&db_path).map_err(|e| Error::Session(format!("open: {e}")))?;
    store
        .set_milestone(zero_session::milestones::WELCOME_SHOWN, &at.to_rfc3339())
        .map_err(|e| Error::Session(format!("milestone: {e}")))?;
    Ok(())
}
