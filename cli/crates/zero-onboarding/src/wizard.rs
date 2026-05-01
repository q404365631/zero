//! The interactive wizard — the "Honest Welcome" flow of
//! spec v2.1 §11.
//!
//! The wizard is deliberately modest. It collects the operator's
//! handle, the engine URL, and an auth token; it probes the
//! engine for reachability so the operator doesn't leave with a
//! silently broken config; it displays the agreed guardrails and
//! asks for confirmation. Nothing else.
//!
//! Advanced configuration (mode defaults, display theme,
//! blocked-symbols lists) is reachable via `zero config edit`
//! later. The first-run bar is kept low on purpose — a brand-new
//! operator should be able to finish setup in under two minutes.

use std::time::Duration;

use chrono::Utc;
use zero_config::{Config, DEFAULT_API_URL};

use crate::Error;
use crate::plan::Plan;
use crate::prompt::Prompt;

/// Pre-computed answers for the non-interactive (scripted) run.
///
/// Every field is optional so partial automation is possible:
/// supply `api` and `token` via environment in CI, let the rest
/// default. If `handle` is absent and stdin is a TTY, the wizard
/// falls back to interactive — otherwise it uses `"operator"`.
#[derive(Debug, Default, Clone)]
pub struct Flags {
    pub handle: Option<String>,
    pub api: Option<String>,
    pub token: Option<String>,
    pub accept_defaults: bool,
}

/// Run the interactive wizard.
///
/// # Errors
/// Returns [`Error::Io`] on stdin failures, [`Error::Validation`]
/// on inputs that do not parse (e.g. a malformed URL).
pub async fn run_interactive<P: Prompt + Send>(prompt: &mut P) -> Result<Plan, Error> {
    header(prompt)?;

    let handle = ask_handle(prompt, None)?;
    let api_url = ask_api(prompt, None)?;
    let token = ask_token(prompt)?;

    let engine_reachable = probe_engine(&api_url, token.as_deref()).await;
    report_reachability(prompt, &api_url, engine_reachable)?;

    let config = Config::starter(&handle);
    let plan = Plan {
        config,
        api_url,
        token,
        engine_reachable,
        generated_at: Utc::now(),
    };

    prompt.say("")?;
    prompt.say("Plan:")?;
    for line in plan.summary().lines() {
        prompt.say(&format!("  {line}"))?;
    }
    prompt.say("")?;

    if !prompt.confirm("Write config and store token?", true)? {
        return Err(Error::Validation("operator declined the plan".into()));
    }

    Ok(plan)
}

/// Run the non-interactive wizard. Uses supplied flags and
/// defaults; never prompts. If required pieces are missing and
/// `accept_defaults` is false, returns `Error::Validation`.
///
/// # Errors
/// Returns `Error::Validation` when required flags are missing.
pub async fn run_non_interactive(flags: &Flags) -> Result<Plan, Error> {
    let handle = flags
        .handle
        .clone()
        .or_else(|| {
            if flags.accept_defaults {
                Some("operator".to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            Error::Validation("missing --handle (or pass --yes to accept 'operator')".into())
        })?;

    let api_url = flags
        .api
        .clone()
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());
    validate_api_url(&api_url)?;

    let token = flags.token.clone().filter(|t| !t.is_empty());

    let engine_reachable = probe_engine(&api_url, token.as_deref()).await;

    Ok(Plan {
        config: Config::starter(handle),
        api_url,
        token,
        engine_reachable,
        generated_at: Utc::now(),
    })
}

fn header<P: Prompt>(prompt: &mut P) -> Result<(), Error> {
    prompt.say("zero — first-run setup")?;
    prompt.say("")?;
    prompt.say("This wizard writes ~/.zero/config.toml and stores your engine")?;
    prompt.say("token in the OS keychain. It does not contact Hyperliquid or")?;
    prompt.say("send telemetry. You can re-run it any time with `zero init`.")?;
    prompt.say("")?;
    Ok(())
}

fn ask_handle<P: Prompt>(prompt: &mut P, existing: Option<&str>) -> Result<String, Error> {
    let default = existing.unwrap_or("operator");
    let answer = prompt.ask("Operator handle (shown in logs and wraps)", Some(default))?;
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return Err(Error::Validation("handle is empty".into()));
    }
    // Permissive — we only exclude characters that would break
    // path / log escaping. No length cap; the operator knows what
    // they want to be called.
    if trimmed.contains(['\n', '\r', '\t', '\0']) {
        return Err(Error::Validation(
            "handle must not contain whitespace control chars".into(),
        ));
    }
    Ok(trimmed.to_string())
}

fn ask_api<P: Prompt>(prompt: &mut P, existing: Option<&str>) -> Result<String, Error> {
    let default = existing.unwrap_or("http://localhost:8000");
    let answer = prompt.ask("Engine API URL", Some(default))?;
    let trimmed = answer.trim().to_string();
    validate_api_url(&trimmed)?;
    Ok(trimmed)
}

fn ask_token<P: Prompt>(prompt: &mut P) -> Result<Option<String>, Error> {
    let answer = prompt.ask_secret("Engine token (blank to skip)")?;
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn validate_api_url(url: &str) -> Result<(), Error> {
    if url.is_empty() {
        return Err(Error::Validation("api url is empty".into()));
    }
    let parsed = url::Url::parse(url).map_err(|e| Error::Validation(format!("api url: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(Error::Validation(format!(
            "api url must be http or https, got {}",
            parsed.scheme()
        )));
    }
    if parsed.host().is_none() {
        return Err(Error::Validation("api url has no host".into()));
    }
    Ok(())
}

fn report_reachability<P: Prompt>(prompt: &mut P, api: &str, reachable: bool) -> Result<(), Error> {
    if reachable {
        prompt.say(&format!("engine: {api} — reachable"))?;
    } else {
        // We deliberately do *not* treat this as an error. A fresh
        // operator may be configuring before starting their
        // engine. The doctor command will confirm the state later.
        prompt.say(&format!(
            "engine: {api} — unreachable right now (config will still be saved; run `zero doctor` after the engine starts)"
        ))?;
    }
    Ok(())
}

async fn probe_engine(api: &str, token: Option<&str>) -> bool {
    let Ok(client) = zero_engine_client::HttpClient::new(api, token.map(str::to_owned)) else {
        return false;
    };
    // Keep the probe short — the operator is waiting on their
    // terminal. Two seconds is enough to tell "the process is up"
    // from "unreachable".
    let probe = async { client.health().await.is_ok() };
    tokio::time::timeout(Duration::from_secs(2), probe)
        .await
        .unwrap_or_default()
}
