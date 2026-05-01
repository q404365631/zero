//! `/config show` + `/config doctor` trait + value types.
//!
//! `zero-commands` intentionally does not depend on `zero-config`:
//! the command crate is compiled for tests and non-TUI callers
//! where the real TOML + keychain layer is overkill. Instead a
//! tiny [`ConfigSource`] trait lives here and the production
//! adapter in `zero/src/main.rs` plugs in the real
//! implementation. Same pattern as [`crate::SessionSource`].
//!
//! Data is returned as plain Rust — no `toml::Value`, no
//! `keyring::Entry`. That keeps the command crate side-effect
//! free (no file or network access from inside tests) and means
//! adapter-side changes never ripple into dispatch.

/// One labelled `/config show` row.
///
/// Intentionally minimal: a human label + its current value.
/// The dispatcher renders `{label}: {value}` verbatim, so the
/// adapter decides formatting (e.g. "not set" vs an empty
/// string) — there is no shared default at this layer because
/// what counts as "unset" differs per field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigShowRow {
    pub label: String,
    pub value: String,
}

impl ConfigShowRow {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// Severity of a doctor finding. Drives the `OutputLine` kind
/// the dispatcher emits:
/// - `Ok` → `System` (informational)
/// - `Warn` → `Warn` (amber, advisory)
/// - `Error` → `Alert` (red + bold, operator must not miss)
///
/// The three levels are the same ones `zero doctor` prints in
/// its non-interactive form, so operators see consistent
/// wording whether they run `/config doctor` inside the TUI or
/// `zero doctor` from a shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorSeverity {
    Ok,
    Warn,
    Error,
}

/// One doctor finding. `message` is rendered verbatim; the
/// dispatcher does not prepend severity or reformat text, so
/// the adapter can choose its own phrasing ("token: set in
/// keychain", "config file missing — run `zero init`", etc.)
/// without the dispatcher needing to know the domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDoctorFinding {
    pub severity: DoctorSeverity,
    pub message: String,
}

impl ConfigDoctorFinding {
    #[must_use]
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            severity: DoctorSeverity::Ok,
            message: message.into(),
        }
    }
    #[must_use]
    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            severity: DoctorSeverity::Warn,
            message: message.into(),
        }
    }
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: DoctorSeverity::Error,
            message: message.into(),
        }
    }
}

/// Read-only trait over the operator's on-disk config + its
/// secret-resolution state. Kept read-only at this layer
/// because write paths (`zero init`, `zero pair`) already live
/// in dedicated non-interactive entrypoints; the TUI should
/// never silently rewrite `config.toml`.
///
/// Implementations:
/// - Production: `ConfigAdapter` in `zero/src/main.rs` wraps
///   `zero_config::load_config` + keychain lookups.
/// - Tests: [`MockConfig`] below is the in-memory double used
///   by `dispatch_integration.rs`.
pub trait ConfigSource: Send + Sync + 'static {
    /// Rows for `/config show`. Order is preserved verbatim in
    /// the rendered output — the adapter decides the column
    /// order so it can group identity → engine → guardrails →
    /// display without the dispatcher needing to know the
    /// schema.
    fn show(&self) -> Vec<ConfigShowRow>;

    /// Findings for `/config doctor`. Return order is
    /// preserved; the adapter is responsible for ordering
    /// (errors first is a reasonable default, but the trait
    /// does not mandate it because a "summary then detail"
    /// shape is sometimes clearer).
    fn doctor(&self) -> Vec<ConfigDoctorFinding>;
}

/// In-memory `ConfigSource` used by tests. Lets a test fix a
/// deterministic set of rows + findings without touching
/// `zero-config` or the filesystem.
#[derive(Debug, Clone, Default)]
pub struct MockConfig {
    pub rows: Vec<ConfigShowRow>,
    pub findings: Vec<ConfigDoctorFinding>,
}

impl MockConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_row(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.rows.push(ConfigShowRow::new(label, value));
        self
    }

    #[must_use]
    pub fn with_finding(mut self, f: ConfigDoctorFinding) -> Self {
        self.findings.push(f);
        self
    }
}

impl ConfigSource for MockConfig {
    fn show(&self) -> Vec<ConfigShowRow> {
        self.rows.clone()
    }
    fn doctor(&self) -> Vec<ConfigDoctorFinding> {
        self.findings.clone()
    }
}
