//! `~/.zero/config.toml` + OS keychain secrets.
//!
//! Schema is typed (serde-derived); secrets never touch disk unencrypted.
//! Secret lookup order: keychain → env var → file fallback (dev only).

#![allow(clippy::module_name_repetitions)]

use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors raised by config loading, validation, and secret resolution.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config directory unavailable")]
    NoConfigDir,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("serialize: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("keychain: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("validation: {0}")]
    Validation(String),
}

/// Top-level operator config. Matches spec §16.1.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub identity: Identity,
    pub mode: Mode,
    pub guardrails: Guardrails,
    pub display: Display,
    pub session: Session,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
    pub handle: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Mode {
    pub default: String,
    pub allow_auto: bool,
    pub allow_headless: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Guardrails {
    pub max_position_pct: f64,
    pub max_concurrent: u32,
    pub daily_loss_pct: f64,
    pub drawdown_pct: f64,
    pub blocked_symbols: Vec<String>,
    pub blocked_directions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub theme: String,
    pub live_stream_default: bool,
    pub verbose_default: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub auto_title: bool,
    pub storage_path: Option<PathBuf>,
}

/// Return the canonical `~/.zero` directory.
pub fn zero_dir() -> Result<PathBuf, ConfigError> {
    let dirs = ProjectDirs::from("dev", "getzero", "zero").ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.data_dir().to_path_buf())
}

/// Return the path to `~/.zero/config.toml`.
pub fn config_path() -> Result<PathBuf, ConfigError> {
    Ok(zero_dir()?.join("config.toml"))
}

/// Default engine API URL when nothing else is configured.
pub const DEFAULT_API_URL: &str = "https://api.getzero.dev";

/// Environment variable names (single source of truth).
pub mod env {
    pub const API_URL: &str = "ZERO_API_URL";
    pub const API_TOKEN: &str = "ZERO_API_TOKEN";
}

/// Keychain service identifiers. See `SECURITY_THREAT_MODEL.md` §4.2.
pub mod keychain {
    /// Engine token — issued by the operator's engine, used by the CLI.
    pub const ENGINE_SERVICE: &str = "dev.getzero.zero";
    /// ZERO Intelligence token — issued by `api.getzero.dev` (future crate).
    pub const INTELLIGENCE_SERVICE: &str = "dev.getzero.zero-intelligence";
    /// Account name under which we store the engine token. A
    /// single logical operator slot per CLI machine; operators with
    /// multiple engines switch via `zero pair --target`.
    pub const DEFAULT_ACCOUNT: &str = "default";
}

/// Resolve the engine API URL from explicit override → env → default.
#[must_use]
pub fn resolve_api_url(explicit: Option<&str>) -> String {
    explicit
        .map(str::to_owned)
        .or_else(|| std::env::var(env::API_URL).ok())
        .unwrap_or_else(|| DEFAULT_API_URL.to_string())
}

/// Resolve the operator token. Precedence matches
/// `SECURITY_THREAT_MODEL.md` §4.2: flag → env → OS keychain.
/// Returns `None` when unset; callers decide whether that's fatal.
///
/// The file-fallback path (Linux headless) is deliberately not
/// implemented yet — it requires `age` and a passphrase prompt.
/// Lands with the Linux-headless support pass.
#[must_use]
pub fn resolve_token(explicit: Option<&str>) -> Option<String> {
    if let Some(t) = explicit.filter(|s| !s.is_empty()) {
        return Some(t.to_owned());
    }
    if let Ok(t) = std::env::var(env::API_TOKEN)
        && !t.is_empty()
    {
        return Some(t);
    }
    // Escape hatch 1 — explicit: `ZERO_NO_KEYCHAIN=1` (any
    // non-empty value) suppresses the keychain fallback. For
    // TUI / cockpit launches where the OS keychain prompt
    // can't be serviced (e.g. a sandboxed shell, or a session
    // that already rejected the password prompt once), this
    // lets the operator fall through with `None` and land on
    // the anonymous HTTP paths rather than block on a modal
    // prompt.
    if std::env::var_os("ZERO_NO_KEYCHAIN").is_some_and(|v| !v.is_empty()) {
        return None;
    }
    // Escape hatch 2 — implicit: non-TTY on both stdin AND
    // stdout means there is no human at this process who can
    // service a macOS / GNOME / KDE keyring unlock prompt. The
    // honest default there is "no token" — we'd rather a
    // scripted `zero run` emit `auth_missing` on stderr and
    // exit non-zero than block forever on a GUI prompt the
    // invoker can never click. Interactive shells (`zero` /
    // `zero run` from a terminal) keep hitting the keyring as
    // before; this only short-circuits the scripted / piped /
    // test-harness path.
    //
    // We check *both* stdin AND stdout on purpose. `cargo
    // test` redirects stdout for capture but leaves stdin on
    // the parent TTY; without the AND, a plain interactive
    // `zero doctor | less` would also trip this branch and
    // silently skip the keyring. Requiring both-non-TTY means
    // "nobody is home on either end" — a strong-enough signal
    // that a GUI prompt cannot be serviced.
    {
        use std::io::IsTerminal as _;
        if !std::io::stdin().is_terminal() && !std::io::stdout().is_terminal() {
            return None;
        }
    }
    keyring_read_engine_token().ok().flatten()
}

/// Write the engine token to the OS keychain. Overwrites any
/// existing entry.
///
/// # Errors
/// Returns `ConfigError::Keyring` on platform-specific failures
/// (no secret service running on Linux, locked keychain on macOS).
pub fn keyring_store_engine_token(token: &str) -> Result<(), ConfigError> {
    let entry = keyring::Entry::new(keychain::ENGINE_SERVICE, keychain::DEFAULT_ACCOUNT)?;
    entry.set_password(token)?;
    Ok(())
}

/// Read the engine token from the keychain.
///
/// # Errors
/// Returns `ConfigError::Keyring` for platform errors other than
/// "no entry found"; a missing entry resolves to `Ok(None)` so
/// callers can treat "no token yet" as the normal first-run state.
pub fn keyring_read_engine_token() -> Result<Option<String>, ConfigError> {
    let entry = keyring::Entry::new(keychain::ENGINE_SERVICE, keychain::DEFAULT_ACCOUNT)?;
    match entry.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete the engine token from the keychain. Idempotent.
///
/// # Errors
/// Only returns an error on keychain I/O; a missing entry is a
/// successful outcome.
pub fn keyring_clear_engine_token() -> Result<(), ConfigError> {
    let entry = keyring::Entry::new(keychain::ENGINE_SERVICE, keychain::DEFAULT_ACCOUNT)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Load the config from disk. Returns `Ok(None)` when the file does
/// not exist so the caller can decide whether to run onboarding.
///
/// # Errors
/// Returns `ConfigError::Io` on read errors other than not-found,
/// or `ConfigError::Parse` on invalid TOML.
pub fn load_config() -> Result<Option<Config>, ConfigError> {
    let path = config_path()?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(Some(toml::from_str(&s)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Write the config atomically (temp-file + rename). Creates
/// `~/.zero/` if missing.
///
/// # Errors
/// Returns `ConfigError::Io` on filesystem failures or
/// `ConfigError::Serialize` on serialization problems.
pub fn save_config(cfg: &Config) -> Result<PathBuf, ConfigError> {
    let dir = zero_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let body = toml::to_string_pretty(cfg)?;
    // Temp-file write + rename so a crash mid-write cannot leave a
    // torn config. We keep the temp in the same dir so `rename` is
    // atomic on the same filesystem.
    let tmp = dir.join(".config.toml.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Current config-schema version. Bump on breaking shape changes
/// and migrate via `Config::migrate_from_v<n>()` helpers.
pub const CONFIG_VERSION: u32 = 1;

impl Config {
    /// A sensible starter config for a fresh operator. Matches the
    /// spec's "default guardrails are defensive" principle — every
    /// limit errs on the side of friction.
    #[must_use]
    pub fn starter(handle: impl Into<String>) -> Self {
        Self {
            version: CONFIG_VERSION,
            identity: Identity {
                handle: handle.into(),
                email: None,
            },
            mode: Mode {
                default: "plan".to_string(),
                allow_auto: false,
                allow_headless: false,
            },
            guardrails: Guardrails {
                // 5% per position is high enough to be useful,
                // low enough that a single bad trade is not a
                // career-ender.
                max_position_pct: 5.0,
                max_concurrent: 3,
                daily_loss_pct: 3.0,
                drawdown_pct: 10.0,
                blocked_symbols: vec![],
                blocked_directions: vec![],
            },
            display: Display {
                theme: "phosphor".to_string(),
                live_stream_default: false,
                verbose_default: "normal".to_string(),
            },
            session: Session {
                auto_title: true,
                storage_path: None,
            },
        }
    }
}
