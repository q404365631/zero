//! Self-diagnostic per spec §18.
//!
//! Non-TTY. Completes in under 2 s against a healthy local engine.
//! Never lies about state — every row reports what the runner
//! actually observed, and a disabled check (no client, no ws URL,
//! no token) shows up as a named skip rather than silently passing.
//!
//! Checks (M1):
//!
//! | name               | verifies                                            |
//! |--------------------|-----------------------------------------------------|
//! | `runtime`          | build profile, target triple, version string        |
//! | `config_dir`       | `~/.zero` exists and is writable; `--fix` creates it |
//! | `config_parse`     | `~/.zero/config.toml` parses (if present)           |
//! | `engine_reachable` | `GET /` succeeds within the HTTP client's timeout   |
//! | `engine_healthy`   | `GET /health` returns `status = "ok"`               |
//! | `engine_components`| no components reported as `dead`                    |
//! | `auth`             | token present (or an explicit warn when absent)     |
//! | `auth_verified`    | if a token is set, an authed call does not 401/403  |
//! | `ws_reachable`     | `/ws` accepts a handshake within 1.5 s              |
//! | `rate_budget`      | local token-bucket fill + refill rate; `--fix` resets |
//!
//! The runner returns a [`Report`] with per-check status. Exit code
//! derives from the worst status: `Fail` → 2, everything else → 0.
//! That matches the existing test fixtures and spec §18.1's
//! "warnings are still exit 0 for script compatibility" note.

#![allow(clippy::module_name_repetitions)]

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use zero_engine_client::{HttpClient, HttpError};

/// Hard cap on the ws-handshake probe. Kept tight so a down engine
/// doesn't stall `zero doctor` past the 2 s total budget when
/// everything else was fast.
const WS_PROBE_TIMEOUT: Duration = Duration::from_millis(1_500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Repaired,
    Warn,
    Fail,
}

impl CheckStatus {
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Repaired => "fixed",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }

    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Ok | Self::Repaired | Self::Warn => 0,
            Self::Fail => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub note: String,
    /// Elapsed in milliseconds for this single check.
    pub elapsed_ms: u64,
}

impl CheckResult {
    fn ok(name: &str, note: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Ok,
            note: note.into(),
            elapsed_ms,
        }
    }

    fn warn(name: &str, note: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Warn,
            note: note.into(),
            elapsed_ms,
        }
    }

    fn fail(name: &str, note: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Fail,
            note: note.into(),
            elapsed_ms,
        }
    }

    fn repaired(name: &str, note: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Repaired,
            note: note.into(),
            elapsed_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub checks: Vec<CheckResult>,
    pub worst: CheckStatus,
    pub total_elapsed_ms: u64,
}

impl Report {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        self.worst.exit_code()
    }

    /// Plain-text renderer matching spec §18.1.
    #[must_use]
    pub fn render_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for c in &self.checks {
            let _ = writeln!(
                out,
                "  [{:>5}] {name:<22} {note} ({ms} ms)",
                c.status.symbol(),
                name = c.name,
                note = c.note,
                ms = c.elapsed_ms,
            );
        }
        let _ = writeln!(
            out,
            "\n  overall: {} in {} ms",
            self.worst.symbol(),
            self.total_elapsed_ms
        );
        out
    }
}

/// Doctor runner configuration. Built via [`Doctor::new`] or
/// [`Doctor::builder`]. Stored rather than re-computed per run so
/// a test harness can drive the runner multiple times with
/// deterministic inputs.
#[derive(Debug)]
pub struct Doctor {
    client: Option<HttpClient>,
    config_dir: PathBuf,
    has_token: bool,
    /// Explicit `config.toml` path. When `None` we fall back to
    /// `<config_dir>/config.toml`. Tests pass this to point at a
    /// fixture without walking the real `~/.zero`.
    config_path: Option<PathBuf>,
    /// WebSocket URL to probe. `None` disables the `ws_reachable`
    /// check (the row is omitted rather than reported as a skip;
    /// tests that want the negative case pass an unreachable URL).
    ws_url: Option<String>,
    /// Bearer token to attach to the `ws_reachable` handshake. Mirrors
    /// what `WsSubscriber` sends during real operation so the doctor
    /// probes the same auth path the TUI will use. `None` → the probe
    /// runs unauthenticated (the old behavior; still useful for local
    /// mock engines that don't gate `/ws`).
    ws_token: Option<String>,
    /// When true, safe-to-regenerate failures flip to `Repaired`
    /// instead of `Warn` — the runner actually performs the fix.
    /// Currently limited to `config_dir` (create on miss). Rate-
    /// budget + auth refresh land with their respective subsystems.
    fix: bool,
    /// When true, missing live custody controls are fatal. Default
    /// doctor runs only warn because paper/read-only operation is
    /// still valid without live custody.
    live_required: bool,
}

impl Doctor {
    /// Minimal constructor matching the pre-M1 signature. Equivalent
    /// to `builder().client(..).config_dir(..).build()`, with no
    /// WS probe and `fix = false`.
    #[must_use]
    pub fn new(client: Option<HttpClient>, config_dir: PathBuf) -> Self {
        Self::builder()
            .client(client)
            .config_dir(config_dir)
            .build()
    }

    #[must_use]
    pub fn builder() -> DoctorBuilder {
        DoctorBuilder::default()
    }

    /// Run every configured check once and build a [`Report`].
    ///
    /// Order is deterministic — the renderer leans on it for a
    /// consistent terminal display. The order is also meaningful:
    /// cheap local checks run first so a misconfigured laptop
    /// surfaces its problem before we spend a budget-heavy second
    /// talking to a network peer.
    pub async fn run(&self) -> Report {
        let started = std::time::Instant::now();
        let mut checks = Vec::with_capacity(10);

        // Local, instant.
        checks.push(check_runtime());
        checks.push(self.check_config_dir());
        checks.push(self.check_config_parse());
        checks.push(self.check_operator_partition());
        checks.push(self.check_credential_partition());

        // Engine-facing.
        if let Some(client) = &self.client {
            checks.push(check_engine_reachable(client).await);

            match client.health().await {
                Ok(h) => {
                    checks.push(engine_healthy_result(&h));
                    checks.push(engine_components_result(&h));
                }
                Err(e) => {
                    checks.push(CheckResult::fail("engine_healthy", e.to_string(), 0));
                }
            }

            // Auth presence is advisory; auth *verification* hits
            // the engine. Keep them distinct so a detached operator
            // (no token, read-only flow) gets a Warn rather than a
            // Fail — read-only doctor runs are still useful.
            checks.push(check_auth_presence(self.has_token));
            if self.has_token {
                checks.push(check_auth_verified(client).await);
            }

            // Rate-budget inspection sits with the engine-facing
            // checks because the budget's entire purpose is
            // pre-flighting HTTP. The row is omitted when the
            // client has no budget attached (narrow test paths);
            // production callers always wire one via
            // `HttpClient::with_rate_budget`.
            if let Some(row) = self.check_rate_budget(client) {
                checks.push(row);
            }
            checks.push(check_live_preflight(client, self.live_required).await);
        } else {
            checks.push(CheckResult::fail(
                "engine_reachable",
                "no api url configured",
                0,
            ));
        }

        // WebSocket probe — skipped when no URL is configured, so
        // non-TTY unit tests that only care about HTTP don't pay
        // the handshake cost.
        if let Some(ws) = self.ws_url.as_deref() {
            checks.push(check_ws_reachable(ws, self.ws_token.as_deref()).await);
        }

        let worst = checks
            .iter()
            .map(|c| c.status)
            .max()
            .unwrap_or(CheckStatus::Ok);
        let total_elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Report {
            checks,
            worst,
            total_elapsed_ms,
        }
    }

    fn check_config_dir(&self) -> CheckResult {
        let started = std::time::Instant::now();
        let dir = &self.config_dir;
        if !dir.exists() {
            // `--fix` path: create the directory and return
            // `Repaired`. The caller typically re-runs the doctor
            // after a fix, but surfacing it as Repaired here means
            // a single invocation is still honest about the state.
            if self.fix {
                match std::fs::create_dir_all(dir) {
                    Ok(()) => {
                        return CheckResult::repaired(
                            "config_dir",
                            format!("created {}", dir.display()),
                            elapsed(started),
                        );
                    }
                    Err(e) => {
                        return CheckResult::fail(
                            "config_dir",
                            format!("create {} failed: {e}", dir.display()),
                            elapsed(started),
                        );
                    }
                }
            }
            return CheckResult::warn(
                "config_dir",
                format!(
                    "{} does not exist — run `zero init` (or `zero doctor --fix`)",
                    dir.display()
                ),
                elapsed(started),
            );
        }
        let writable = std::fs::metadata(dir)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false);
        if writable {
            CheckResult::ok("config_dir", dir.display().to_string(), elapsed(started))
        } else {
            CheckResult::warn(
                "config_dir",
                format!("{} is read-only", dir.display()),
                elapsed(started),
            )
        }
    }

    /// Inspect the attached [`zero_engine_client::RateBudget`].
    /// Returns `None` when the client has no budget configured —
    /// the row is omitted rather than faked, in line with the
    /// rest of the doctor: silence on a missing probe target is
    /// more honest than a "not configured" line that looks like
    /// a real result.
    ///
    /// With `fix = true`, an empty bucket is **reset to full** and
    /// the row reports `Repaired`. This is the M2_PLAN §1
    /// "clear-counter" action. A non-empty bucket is never
    /// "repaired" — the fix path is not a no-op noop, it is a
    /// deliberate operator override of an exhausted state, and
    /// the doctor must not turn it into routine hygiene.
    fn check_rate_budget(&self, client: &HttpClient) -> Option<CheckResult> {
        let budget = client.rate_budget()?;
        let started = std::time::Instant::now();
        let snap = budget.snapshot();
        let headroom = snap.headroom();
        let refill = snap.refill_per_second;
        let note = format!(
            "{current}/{cap} tokens · refill {refill:.2}/s",
            current = snap.tokens,
            cap = snap.capacity,
        );

        // Exhausted → `--fix` refills, else warn with a visible
        // count. Near-empty (< 10 %) is always a Warn regardless
        // of `--fix` because auto-refilling a still-usable bucket
        // would mask a real "operator is pounding the keys" signal.
        if snap.tokens == 0 {
            if self.fix {
                budget.reset_to_full();
                let after = budget.snapshot();
                return Some(CheckResult::repaired(
                    "rate_budget",
                    format!(
                        "reset: {after_tokens}/{after_cap} tokens (was 0/{cap})",
                        after_tokens = after.tokens,
                        after_cap = after.capacity,
                        cap = snap.capacity,
                    ),
                    elapsed(started),
                ));
            }
            return Some(CheckResult::warn(
                "rate_budget",
                format!("exhausted — {note} (run `zero doctor --fix` to reset)"),
                elapsed(started),
            ));
        }
        if headroom < 0.10 {
            return Some(CheckResult::warn(
                "rate_budget",
                format!("near-empty — {note}"),
                elapsed(started),
            ));
        }
        Some(CheckResult::ok("rate_budget", note, elapsed(started)))
    }

    fn check_config_parse(&self) -> CheckResult {
        let started = std::time::Instant::now();
        let path = self
            .config_path
            .clone()
            .unwrap_or_else(|| self.config_dir.join("config.toml"));
        if !path.exists() {
            // Missing config.toml is the pre-`zero init` state.
            // Warn (not Fail) because read-only commands work
            // without a config file and many operators run `zero
            // doctor` before anything else.
            return CheckResult::warn(
                "config_parse",
                format!("{} not found — run `zero init`", path.display()),
                elapsed(started),
            );
        }
        match std::fs::read_to_string(&path) {
            Ok(body) => match toml::from_str::<toml::Value>(&body) {
                Ok(_) => CheckResult::ok(
                    "config_parse",
                    format!("{} parses", path.display()),
                    elapsed(started),
                ),
                Err(e) => CheckResult::fail(
                    "config_parse",
                    // `toml::de::Error::Display` already carries the
                    // line/column, so there's no need to synthesize
                    // one here.
                    format!("parse error: {e}"),
                    elapsed(started),
                ),
            },
            Err(e) => CheckResult::fail(
                "config_parse",
                format!("read {}: {e}", path.display()),
                elapsed(started),
            ),
        }
    }

    fn check_operator_partition(&self) -> CheckResult {
        let started = std::time::Instant::now();
        let path = self
            .config_path
            .clone()
            .unwrap_or_else(|| self.config_dir.join("config.toml"));
        let handle = std::fs::read_to_string(&path)
            .ok()
            .and_then(|body| toml::from_str::<zero_config::Config>(&body).ok())
            .map_or_else(|| "local-operator".to_string(), |cfg| cfg.identity.handle);
        let paths = zero_config::runtime_paths_in(self.config_dir.clone(), &handle);
        let expected_prefix = self.config_dir.join("operators").join(&paths.operator_slug);
        if !paths.operator_dir.starts_with(&expected_prefix) {
            return CheckResult::fail(
                "operator_partition",
                format!(
                    "state dir escaped operator partition: {}",
                    paths.operator_dir.display()
                ),
                elapsed(started),
            );
        }

        let legacy_paths = [
            self.config_dir.join("state.db"),
            self.config_dir.join("zero.log"),
            self.config_dir.join("sock"),
            self.config_dir.join("state").join("headless.json"),
        ];
        let stale: Vec<String> = legacy_paths
            .iter()
            .filter(|p| p.exists())
            .map(|p| p.display().to_string())
            .collect();
        if !stale.is_empty() {
            return CheckResult::warn(
                "operator_partition",
                format!(
                    "legacy shared artifacts present; migrate or archive: {}",
                    stale.join(", ")
                ),
                elapsed(started),
            );
        }

        CheckResult::ok(
            "operator_partition",
            format!(
                "{} -> {}",
                paths.operator_slug,
                paths.operator_dir.display()
            ),
            elapsed(started),
        )
    }

    fn check_credential_partition(&self) -> CheckResult {
        let started = std::time::Instant::now();
        let path = self
            .config_path
            .clone()
            .unwrap_or_else(|| self.config_dir.join("config.toml"));
        if !path.exists() {
            return CheckResult::warn(
                "credential_partition",
                "no config; keychain account will use legacy default until `zero init`",
                elapsed(started),
            );
        }
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|body| toml::from_str::<zero_config::Config>(&body).ok())
        {
            Some(cfg) => {
                let account = zero_config::keychain_account_for_handle(&cfg.identity.handle);
                CheckResult::ok(
                    "credential_partition",
                    format!(
                        "keychain account {account} for services dev.getzero.zero and dev.getzero.hyperliquid"
                    ),
                    elapsed(started),
                )
            }
            None => CheckResult::warn(
                "credential_partition",
                "config unreadable; keychain account cannot be derived",
                elapsed(started),
            ),
        }
    }
}

/// Fluent builder for [`Doctor`]. All fields default to "disabled";
/// the caller explicitly turns on each surface they want probed.
/// This keeps doctor runs honest: a check you didn't configure
/// doesn't appear in the report.
#[derive(Debug, Default)]
pub struct DoctorBuilder {
    client: Option<HttpClient>,
    config_dir: Option<PathBuf>,
    config_path: Option<PathBuf>,
    ws_url: Option<String>,
    ws_token: Option<String>,
    fix: bool,
    live_required: bool,
}

impl DoctorBuilder {
    #[must_use]
    pub fn client(mut self, client: Option<HttpClient>) -> Self {
        self.client = client;
        self
    }

    #[must_use]
    pub fn config_dir(mut self, dir: PathBuf) -> Self {
        self.config_dir = Some(dir);
        self
    }

    /// Override the config-file path that `config_parse` loads.
    /// Without this, the runner uses `<config_dir>/config.toml`.
    #[must_use]
    pub fn config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Enable the `ws_reachable` check against `url`. Missing →
    /// the check is omitted from the report.
    #[must_use]
    pub fn ws_url(mut self, url: impl Into<String>) -> Self {
        self.ws_url = Some(url.into());
        self
    }

    /// Attach a bearer token to the `ws_reachable` probe. Without
    /// this, the probe dials `/ws` unauthenticated — which fails
    /// `403` against any engine that requires a token, producing a
    /// false negative. Paired with [`Self::ws_url`] in production.
    #[must_use]
    pub fn ws_token(mut self, token: impl Into<String>) -> Self {
        self.ws_token = Some(token.into());
        self
    }

    /// Enable in-place repairs for checks that support them.
    #[must_use]
    pub const fn fix(mut self, enabled: bool) -> Self {
        self.fix = enabled;
        self
    }

    /// Require live custody preflight to pass. Used by future
    /// live-mode start paths; ordinary doctor runs keep this false
    /// so paper/read-only operators can still get a useful report.
    #[must_use]
    pub const fn live_required(mut self, enabled: bool) -> Self {
        self.live_required = enabled;
        self
    }

    /// Build the runner. Missing `config_dir` defaults to the
    /// system temp directory — a check against it will `Warn` at
    /// worst, which is the correct "misconfigured caller" signal.
    #[must_use]
    pub fn build(self) -> Doctor {
        let has_token = self.client.as_ref().is_some_and(HttpClient::has_token);
        Doctor {
            client: self.client,
            config_dir: self.config_dir.unwrap_or_else(std::env::temp_dir),
            has_token,
            config_path: self.config_path,
            ws_url: self.ws_url,
            ws_token: self.ws_token,
            fix: self.fix,
            live_required: self.live_required,
        }
    }
}

fn elapsed(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Runtime check — CLI version + target triple + build profile.
///
/// Always `Ok`; the value is the reader diagnostic ("I ran a debug
/// build of vX.Y on aarch64-apple-darwin"). Pinning this at the top
/// of the report means bug reports land with the operator's build
/// already stamped in.
fn check_runtime() -> CheckResult {
    // `CARGO_PKG_VERSION` is set by cargo itself. OS/arch come from
    // `std::env::consts` so we don't need a `build.rs` to generate
    // a target triple for a single diagnostic string.
    let version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    CheckResult::ok(
        "runtime",
        format!("zero-doctor v{version} · {os}/{arch} · {profile}"),
        0,
    )
}

fn check_auth_presence(has_token: bool) -> CheckResult {
    if has_token {
        CheckResult::ok("auth", "token present", 0)
    } else {
        CheckResult::warn("auth", "no token set — read-only endpoints only", 0)
    }
}

async fn check_auth_verified(client: &HttpClient) -> CheckResult {
    let started = std::time::Instant::now();
    // `/risk` is a typed authenticated endpoint. A 401/403 here
    // means the token exists but the engine rejects it — the
    // exact failure mode the presence-only check cannot catch.
    // An unreachable engine is not an auth problem; defer to
    // `engine_reachable` for that story and treat it as Warn
    // here to avoid double-failing the report.
    match client.risk().await {
        Ok(_) => CheckResult::ok("auth_verified", "token accepted", elapsed(started)),
        Err(HttpError::Unauthorized) => CheckResult::fail(
            "auth_verified",
            "engine rejected token — rotate via `zero init --force`",
            elapsed(started),
        ),
        Err(HttpError::NotFound { .. }) => CheckResult::warn(
            "auth_verified",
            "engine did not expose /risk; auth cannot be verified",
            elapsed(started),
        ),
        Err(e) => CheckResult::warn("auth_verified", format!("skipped: {e}"), elapsed(started)),
    }
}

async fn check_engine_reachable(client: &HttpClient) -> CheckResult {
    let started = std::time::Instant::now();
    match client.root().await {
        Ok(root) => CheckResult::ok(
            "engine_reachable",
            format!("{} v{} ({})", root.name, root.version, client.base_url()),
            elapsed(started),
        ),
        Err(HttpError::Unreachable(msg)) => CheckResult::fail(
            "engine_reachable",
            format!("unreachable: {msg}"),
            elapsed(started),
        ),
        Err(HttpError::Timeout(_)) => CheckResult::fail(
            "engine_reachable",
            format!("timeout after {:?}", zero_engine_client::http::timeout()),
            elapsed(started),
        ),
        Err(e) => CheckResult::fail("engine_reachable", e.to_string(), elapsed(started)),
    }
}

async fn check_live_preflight(client: &HttpClient, live_required: bool) -> CheckResult {
    let started = std::time::Instant::now();
    match client.live_preflight().await {
        Ok(preflight) if preflight.ready => CheckResult::ok(
            "live_preflight",
            "live custody and safety preflight ready",
            elapsed(started),
        ),
        Ok(preflight) if preflight.controls_ready => CheckResult::ok(
            "live_preflight",
            "custody controls pass; live executor is still refused",
            elapsed(started),
        ),
        Ok(preflight) => {
            let failed: Vec<&str> = preflight
                .checks
                .iter()
                .filter(|check| check.status != "ok")
                .map(|check| check.name.as_str())
                .collect();
            let note = if failed.is_empty() {
                "not ready".to_string()
            } else {
                format!("not ready: {}", failed.join(", "))
            };
            if live_required {
                CheckResult::fail("live_preflight", note, elapsed(started))
            } else {
                CheckResult::warn("live_preflight", note, elapsed(started))
            }
        }
        Err(HttpError::NotFound { .. }) => CheckResult::warn(
            "live_preflight",
            "engine does not expose /live/preflight",
            elapsed(started),
        ),
        Err(e) => CheckResult::warn("live_preflight", format!("skipped: {e}"), elapsed(started)),
    }
}

fn engine_healthy_result(h: &zero_engine_client::Health) -> CheckResult {
    let counts = h.component_counts();
    if h.is_ok() {
        CheckResult::ok(
            "engine_healthy",
            format!(
                "ok — {} healthy / {} stale / {} dead",
                counts.healthy, counts.stale, counts.dead
            ),
            0,
        )
    } else {
        CheckResult::warn(
            "engine_healthy",
            format!(
                "degraded — {} healthy / {} stale / {} dead",
                counts.healthy, counts.stale, counts.dead
            ),
            0,
        )
    }
}

fn engine_components_result(h: &zero_engine_client::Health) -> CheckResult {
    let counts = h.component_counts();
    if counts.dead > 0 {
        let dead: Vec<&str> = h
            .components
            .iter()
            .filter(|(_, c)| c.is_dead())
            .map(|(k, _)| k.as_str())
            .collect();
        CheckResult::fail("engine_components", format!("dead: {}", dead.join(", ")), 0)
    } else if counts.stale > 0 {
        CheckResult::warn(
            "engine_components",
            format!("{} components stale (>30s)", counts.stale),
            0,
        )
    } else {
        CheckResult::ok("engine_components", "all fresh", 0)
    }
}

/// WebSocket reachability probe. Dials `/ws` once, upgrades, and
/// drops the connection. Does *not* read any frames — a successful
/// upgrade is already proof the engine is serving the push surface,
/// and pulling a frame can take arbitrarily long depending on
/// engine cadence (the heartbeat is every ~5 s on the real server).
///
/// Returns `Fail` on any connect/upgrade error, `Warn` on timeout
/// (timeout ≠ "down"; it might just be slow), `Ok` on a clean
/// handshake.
async fn check_ws_reachable(url: &str, token: Option<&str>) -> CheckResult {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

    let started = std::time::Instant::now();

    // Build the handshake request explicitly so we can attach a
    // bearer token. An unauthenticated probe would always fail 403
    // against the real engine, producing a false negative.
    let request = match url.into_client_request() {
        Ok(mut req) => {
            if let Some(t) = token {
                match format!("Bearer {t}").parse() {
                    Ok(v) => {
                        req.headers_mut().insert("Authorization", v);
                    }
                    Err(e) => {
                        return CheckResult::fail(
                            "ws_reachable",
                            format!("{url}: invalid token for header: {e}"),
                            elapsed(started),
                        );
                    }
                }
            }
            req
        }
        Err(e) => {
            return CheckResult::fail(
                "ws_reachable",
                format!("{url}: invalid url: {e}"),
                elapsed(started),
            );
        }
    };

    let outcome =
        tokio::time::timeout(WS_PROBE_TIMEOUT, tokio_tungstenite::connect_async(request)).await;
    match outcome {
        Ok(Ok((ws, _resp))) => {
            // Close as politely as tungstenite allows; we don't need
            // frames, we just needed the upgrade to land.
            drop(ws);
            CheckResult::ok("ws_reachable", url.to_string(), elapsed(started))
        }
        Ok(Err(e)) => CheckResult::fail("ws_reachable", format!("{url}: {e}"), elapsed(started)),
        Err(_) => CheckResult::warn(
            "ws_reachable",
            format!("{url}: timeout after {WS_PROBE_TIMEOUT:?}"),
            elapsed(started),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_spec() {
        assert_eq!(CheckStatus::Ok.exit_code(), 0);
        assert_eq!(CheckStatus::Repaired.exit_code(), 0);
        // Warn is still 0 — spec §18.1 keeps warnings
        // script-compatible. Fail is the only non-zero exit.
        assert_eq!(CheckStatus::Warn.exit_code(), 0);
        assert_eq!(CheckStatus::Fail.exit_code(), 2);
    }

    #[test]
    fn worst_prefers_fail_then_warn_then_repaired() {
        // Derived Ord; pin it so a future enum-reorder can't
        // silently flip the worst-case math in `Doctor::run`.
        assert!(CheckStatus::Fail > CheckStatus::Warn);
        assert!(CheckStatus::Warn > CheckStatus::Repaired);
        assert!(CheckStatus::Repaired > CheckStatus::Ok);
    }

    #[test]
    fn render_text_includes_status_symbol_and_note() {
        let report = Report {
            checks: vec![
                CheckResult::ok("runtime", "v0 · test · debug", 0),
                CheckResult::repaired("config_dir", "created /tmp/zero-x", 3),
                CheckResult::warn("auth", "no token", 1),
                CheckResult::fail("engine_reachable", "down", 12),
            ],
            worst: CheckStatus::Fail,
            total_elapsed_ms: 42,
        };
        let out = report.render_text();
        assert!(out.contains("[   ok] runtime"), "{out}");
        assert!(out.contains("[fixed] config_dir"), "{out}");
        assert!(out.contains("[ warn] auth"), "{out}");
        assert!(out.contains("[ fail] engine_reachable"), "{out}");
        assert!(out.contains("overall: fail in 42 ms"), "{out}");
    }

    #[test]
    fn runtime_check_is_always_ok() {
        let c = check_runtime();
        assert_eq!(c.status, CheckStatus::Ok);
        assert!(c.note.starts_with("zero-doctor v"));
    }

    fn doctor_with_client(client: HttpClient, fix: bool) -> Doctor {
        Doctor::builder()
            .client(Some(client))
            .config_dir(std::env::temp_dir())
            .fix(fix)
            .build()
    }

    #[test]
    fn rate_budget_check_returns_ok_on_fresh_bucket() {
        let budget = zero_engine_client::RateBudget::default_system();
        let client = HttpClient::new("http://127.0.0.1:1", None)
            .expect("client")
            .with_rate_budget(budget);
        let doctor = doctor_with_client(client.clone(), false);
        let row = doctor
            .check_rate_budget(&client)
            .expect("row present when budget attached");
        assert_eq!(row.status, CheckStatus::Ok);
        // Fresh bucket reads as `60/60` today; pin the shape to
        // catch a future capacity change that forgets the row.
        assert!(row.note.contains("60/60"), "got {}", row.note);
    }

    #[test]
    fn rate_budget_check_warns_on_exhausted_bucket_without_fix() {
        let clock = zero_engine_client::ManualClock::new();
        let budget = zero_engine_client::RateBudget::with_clock(10, 0.0, clock);
        assert!(budget.try_consume(10).is_ok());
        let client = HttpClient::new("http://127.0.0.1:1", None)
            .expect("client")
            .with_rate_budget(budget);
        let doctor = doctor_with_client(client.clone(), false);
        let row = doctor.check_rate_budget(&client).expect("row");
        assert_eq!(row.status, CheckStatus::Warn);
        assert!(row.note.contains("exhausted"), "got {}", row.note);
        assert!(row.note.contains("`zero doctor --fix`"), "got {}", row.note);
    }

    #[test]
    fn rate_budget_check_repairs_exhausted_bucket_with_fix() {
        let clock = zero_engine_client::ManualClock::new();
        let budget = zero_engine_client::RateBudget::with_clock(10, 0.0, clock);
        assert!(budget.try_consume(10).is_ok());
        let client = HttpClient::new("http://127.0.0.1:1", None)
            .expect("client")
            .with_rate_budget(budget);
        let doctor = doctor_with_client(client.clone(), true);
        let row = doctor.check_rate_budget(&client).expect("row");
        assert_eq!(row.status, CheckStatus::Repaired);
        assert!(row.note.contains("reset:"), "got {}", row.note);
        // Bucket must actually be full post-fix.
        let after = client.rate_budget().unwrap().snapshot();
        assert_eq!(after.tokens, 10);
    }

    #[test]
    fn rate_budget_check_warns_when_near_empty_even_with_fix() {
        // Near-empty (<10%) is a **real signal** — auto-refilling
        // it would mask operator-typing-pressure. `--fix` must not
        // touch it; only a strictly exhausted bucket flips to
        // Repaired.
        let clock = zero_engine_client::ManualClock::new();
        let budget = zero_engine_client::RateBudget::with_clock(100, 0.0, clock);
        assert!(budget.try_consume(95).is_ok()); // 5/100 → 5 % headroom
        let client = HttpClient::new("http://127.0.0.1:1", None)
            .expect("client")
            .with_rate_budget(budget);
        let doctor = doctor_with_client(client.clone(), true);
        let row = doctor.check_rate_budget(&client).expect("row");
        assert_eq!(row.status, CheckStatus::Warn);
        assert!(row.note.contains("near-empty"), "got {}", row.note);
        // Bucket still at 5/100 — fix did *not* run.
        let after = client.rate_budget().unwrap().snapshot();
        assert_eq!(after.tokens, 5);
    }

    #[test]
    fn rate_budget_row_omitted_when_no_budget_attached() {
        let client = HttpClient::new("http://127.0.0.1:1", None).expect("client");
        let doctor = doctor_with_client(client.clone(), false);
        assert!(doctor.check_rate_budget(&client).is_none());
    }
}
