//! `zero` — operator terminal entrypoint.
//!
//! Subcommands exit on completion. No-argument invocation launches
//! the interactive TUI (M1). This binary is the dispatcher; all
//! behavior lives in the supporting crates.

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use zero_commands::DispatchContext;

// Real `SupervisorSource` adapter that bridges the sync
// dispatcher surface to the async `zero-headless` client. Kept
// in its own module to keep `main.rs` focused on argv plumbing.
mod supervisor_adapter;
use zero_engine_client::{
    EngineState, EngineStatePoller, HttpClient, OperatorStatePoller, WsSubscriber,
};

/// Exit-code taxonomy.
///
/// Centralised here so every `ExitCode::from(n)` in this binary
/// goes through a named reason rather than a magic integer. This
/// is the contract §9's CI gate check against, the one the
/// operator's shell-scripts branch on, and the one
/// `docs/honesty.md` references when it says "the exit code is
/// not a vibe."
///
/// The numbers are load-bearing: scripts written against this
/// binary branch on them, and changing a value after ship is an
/// honesty regression (the operator's `if $? -eq 2` stops
/// meaning what it used to). The `#[repr(u8)]` + match-proof
/// below make drift hard by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ExitKind {
    /// Command succeeded.
    Ok = 0,
    /// Invalid arguments, missing required flag, refusing to
    /// overwrite config without `--force`, refusing a risk-
    /// increasing command in non-interactive mode. Anything the
    /// operator can fix by editing the invocation.
    Usage = 1,
    /// Engine reachable check failed: DNS, TCP, 5xx, timeout.
    /// The CLI is healthy; the server is not.
    EngineUnreachable = 2,
    /// Authentication failed: 401, missing/expired token, bad
    /// keychain entry. Surfaced separately from `Usage` because
    /// the operator's fix is different (run `zero init`, not
    /// re-type the command).
    ///
    /// Reserved: no call site emits this today. The dispatcher
    /// collapses all engine errors into `OutputLine::Alert`
    /// without surfacing the HTTP status, so a 401 is
    /// currently indistinguishable from a 500 at this layer
    /// and both map to `EngineUnreachable`. Wiring the status
    /// through is an M2 task; until then `AuthInvalid` stays
    /// reserved so scripts already branching on exit code 3
    /// keep working once the wiring lands.
    #[allow(dead_code)]
    AuthInvalid = 3,
    /// Something the CLI did went wrong that is neither the
    /// operator's fault nor the engine's: disk I/O, JSON
    /// serialization, a panic we caught. Always worth a
    /// bug report.
    Internal = 4,
}

impl ExitKind {
    fn code(self) -> ExitCode {
        ExitCode::from(self as u8)
    }
}

impl From<ExitKind> for ExitCode {
    fn from(k: ExitKind) -> Self {
        k.code()
    }
}

/// The operator's terminal — intelligence. not automation.
#[derive(Debug, Parser)]
#[command(
    name = "zero",
    version,
    about = "ZERO operator terminal",
    long_about = "The CLI-native surface for ZERO self-custodial onchain operations. \
                  Plan → Auto → Headless, with Shadow → Paper → Live for every \
                  composition change. Engine is source of truth; CLI is a \
                  renderer + dispatcher."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Engine API endpoint (default: <https://api.getzero.dev>).
    #[arg(long, env = "ZERO_API_URL", global = true)]
    api: Option<String>,

    /// Operator token (or set `ZERO_API_TOKEN`).
    #[arg(long, env = "ZERO_API_TOKEN", global = true)]
    token: Option<String>,

    /// Start in paper mode.
    #[arg(long, global = true)]
    paper: bool,

    /// Disable session persistence for this invocation. The TUI
    /// runs from a fresh in-memory log and nothing is written to
    /// `~/.zero/state.db`.
    #[arg(long, global = true)]
    no_persist: bool,

    /// Emit machine-readable output (where supported). Implies
    /// no color and no widgets.
    #[arg(long, global = true)]
    json: bool,

    /// Increase log verbosity (`-v` info, `-vv` debug, `-vvv` trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// First-run setup wizard.
    ///
    /// Interactive by default. Supply flags and `--yes` for
    /// non-interactive use (CI, scripts).
    Init {
        /// Operator handle (any non-whitespace string). In
        /// non-interactive mode, required unless `--yes`.
        #[arg(long)]
        handle: Option<String>,

        /// Accept defaults and skip confirmation prompts.
        /// Required for non-interactive runs without a handle.
        #[arg(long)]
        yes: bool,

        /// Run non-interactively — never prompt, even for
        /// missing pieces. Useful in CI.
        #[arg(long)]
        non_interactive: bool,

        /// Produce the plan but do not write it. Pairs well
        /// with `--non-interactive` for rehearsing a config
        /// before committing.
        #[arg(long)]
        dry_run: bool,

        /// Overwrite existing config without prompting.
        #[arg(long)]
        force: bool,
    },

    /// Local diagnostic. Exits non-zero on failed checks.
    Doctor {
        /// Attempt auto-repair of repairable failures. **M1 stub.**
        #[arg(long)]
        fix: bool,

        /// Override output format.
        #[arg(long, value_enum)]
        format: Option<Format>,
    },

    /// Print CLI + engine version info.
    Version,

    /// Run a single slash-command non-interactively and exit.
    ///
    /// Example: `zero run status`, `zero run risk`,
    /// `zero run regime BTC`, `zero run pulse 20`.
    ///
    /// The exact input (minus the leading slash) is forwarded
    /// to the same dispatcher the TUI uses, so every
    /// `Neutral` / `Reduces` command that makes sense without
    /// a conversation pane is available. Risk-*Increasing*
    /// commands (`/execute`, `/state-override`,
    /// `/disclosure-override`) are refused: they require a
    /// typed-confirm overlay that cannot exist without a TTY,
    /// and running them through a scripted pipe would bypass
    /// the friction ladder on purpose — a safety regression we
    /// will not ship. The refusal is always `Usage` (exit 1),
    /// never silent.
    Run {
        /// The slash-command to run. Leading `/` is optional;
        /// arguments follow as separate tokens.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        input: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Tracing target depends on the subcommand. The TUI owns the
    // terminal — writing WARN lines to stderr there overlays the
    // status bar with raw log text (first observed in the wild as
    // "r=HTTP error: 403 Forbidden retry:6 …" bleeding across
    // `ops:?` after the engine WS repeatedly 403'd). Non-TUI
    // commands (doctor, version, run, init) keep the stderr
    // writer — that's what scripts, CI, and piped invocations
    // expect, and stdout stays clean for `--json`.
    let target = tracing_target_for(cli.command.as_ref());
    init_tracing(cli.verbose, &target);

    // M2 §5: `--paper` is now a live per-invocation override.
    // `build_client` attaches [`zero_engine_client::Mode::Paper`]
    // to the HTTP client; every request thereafter carries
    // `X-Zero-Mode: paper`. We emit a single stderr breadcrumb
    // so operators know the flag took effect — silence here
    // would be honest (the header is enough) but an explicit
    // acknowledgement matches the flag's "I meant it" shape.
    // Held out of the TUI path because the TUI renders the mode
    // in the status bar; stderr noise during a live session
    // would land on top of the frame.
    if cli.paper && cli.command.is_some() {
        eprintln!("zero: --paper active — requests carry X-Zero-Mode: paper.");
    }

    match &cli.command {
        Some(Command::Init {
            handle,
            yes,
            non_interactive,
            dry_run,
            force,
        }) => {
            run_init(
                &cli,
                handle.clone(),
                InitOptions {
                    yes: *yes,
                    non_interactive: *non_interactive,
                    dry_run: *dry_run,
                    force: *force,
                },
            )
            .await
        }
        Some(Command::Doctor { fix, format }) => run_doctor(&cli, *fix, *format).await,
        Some(Command::Version) => run_version(&cli).await,
        Some(Command::Run { input }) => run_oneshot(&cli, input).await,
        None => {
            // No subcommand + no TTY on stdout → print help and
            // exit 0. The spec's rationale is that a bare
            // invocation in a script (CI, cron, `sh -c`) must
            // not hang waiting for terminal input; `zero` is
            // not `cat` and should not stream raw TUI bytes to
            // a pipe. Exit 0 rather than 1 because printing
            // help is a *successful* response to "no TTY".
            if !std::io::stdout().is_terminal() {
                let mut cmd = Cli::command();
                let _ = cmd.print_help();
                println!();
                return ExitKind::Ok.into();
            }
            run_tui(&cli).await
        }
    }
}

/// Where tracing should write its formatted records. The TUI
/// path routes to a log file so WARN lines from the WS poller
/// et al. do not interleave with the rendered frame; every
/// other entrypoint keeps the stderr writer so scripts see
/// diagnostics on the expected stream.
#[derive(Debug, Clone)]
enum TracingTarget {
    /// Default: emit to stderr with ANSI formatting. Used by
    /// `doctor`, `version`, `run`, `init`, and the no-TTY help
    /// path.
    Stderr,
    /// TUI path: emit to `<zero_dir>/zero.log`, append-only,
    /// no ANSI. If the file cannot be opened we silently fall
    /// back to discarding — the alternative is corrupting the
    /// frame, which is the exact bug this enum exists to fix.
    TuiLogFile,
}

/// Decide the tracing writer from the parsed subcommand. Kept
/// free so the choice is testable without spinning up a full
/// `Cli`.
fn tracing_target_for(cmd: Option<&Command>) -> TracingTarget {
    match cmd {
        // Only the bare-`zero` path launches the TUI. Every
        // other subcommand is non-interactive and stdout/stderr
        // are both valid diagnostic channels.
        None => TracingTarget::TuiLogFile,
        Some(_) => TracingTarget::Stderr,
    }
}

fn init_tracing(verbose: u8, target: &TracingTarget) {
    let filter = match verbose {
        0 => "warn",
        1 => "zero=info,zero_engine_client=info,zero_doctor=info",
        2 => "debug",
        _ => "trace",
    };
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter));

    match target {
        TracingTarget::Stderr => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_writer(std::io::stderr)
                .try_init();
        }
        TracingTarget::TuiLogFile => {
            // Best-effort: resolve `<zero_dir>/zero.log`, create
            // the dir, open for append. On any failure we fall
            // through to a no-op subscriber — the alternative is
            // falling back to stderr, which is exactly the bug
            // this branch exists to avoid (WARN records from the
            // WS poller overlay the status bar).
            //
            // `tracing_subscriber` requires a `MakeWriter`; the
            // blanket impl covers `Arc<Mutex<W: Write>>`, which
            // is what we construct here so the writer is both
            // thread-safe and has no interior dynamic dispatch.
            // `Arc<File>` implements `MakeWriter` because
            // `&File` implements `Write` on every platform
            // tracing-subscriber targets. Append-only O_APPEND
            // opens give us the write-atomicity we need without
            // an extra `Mutex` wrap.
            //
            // If the file cannot be opened we silently discard
            // — there is no honest recovery, since stderr is
            // the one place we must NOT write when the TUI is
            // up. A future `/log` overlay inside the TUI can
            // surface init failures to the operator without
            // risking the frame.
            if let Ok(file) = open_tui_log_sink() {
                let writer = std::sync::Arc::new(file);
                let _ = tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .with_ansi(false)
                    .with_writer(writer)
                    .try_init();
            }
        }
    }
}

/// Open `<zero_dir>/zero.log` for append, creating the parent
/// dir if needed. Factored out so the happy-path logic in
/// `init_tracing` stays readable.
fn open_tui_log_sink() -> std::io::Result<std::fs::File> {
    let dir = zero_config::zero_dir().map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("zero.log"))
}

/// Mirror of the four independent `zero init` flags. Grouped as
/// a struct so `run_init` stays under the
/// `fn_params_excessive_bools` limit; each field is genuinely
/// orthogonal (user-facing CLI flags), so no enum reduction
/// helps.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
struct InitOptions {
    yes: bool,
    non_interactive: bool,
    dry_run: bool,
    force: bool,
}

async fn run_init(cli: &Cli, handle: Option<String>, opts: InitOptions) -> ExitCode {
    use zero_onboarding::{Flags, StdioPrompt, run_interactive, run_non_interactive};

    // If a config already exists and we weren't asked to force,
    // show what we already have and bail with a hint. Re-running
    // `zero init --force` is the escape hatch; a minimal-diff
    // reconfigure command (provisional name `zero pair`) is
    // deferred to a later milestone. Do **not** reference `zero
    // pair` in operator-facing strings until it exists — the
    // doctor remediation hint at `resolved_token_source()` was
    // changed in lock-step with this note.
    if !opts.force
        && let Ok(Some(existing)) = zero_config::load_config()
    {
        let path = zero_config::config_path()
            .map_or_else(|_| "<unknown>".into(), |p| p.display().to_string());
        eprintln!("zero: config already exists.");
        eprintln!("  handle:  {}", existing.identity.handle);
        eprintln!("  mode:    {}", existing.mode.default);
        eprintln!("  path:    {path}");
        eprintln!();
        eprintln!("Pass --force to overwrite, or run `zero doctor` to verify.");
        // Refusing an overwrite is a usage issue — the operator
        // can fix it by adding `--force`. Not an engine /
        // auth / internal problem.
        return ExitKind::Usage.into();
    }

    let api_override = cli.api.clone();
    let token_override = cli.token.clone();

    // Flags feed both interactive defaults and non-interactive
    // answers. The wizard treats missing pieces differently in
    // each mode (prompt vs. error).
    let flags = Flags {
        handle,
        api: api_override,
        token: token_override,
        accept_defaults: opts.yes,
    };

    let plan = if opts.non_interactive {
        match run_non_interactive(&flags).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("zero: init failed: {e}");
                // `run_non_interactive` errors on missing pieces
                // (no handle, no `--yes`) — that is a usage
                // failure the operator can fix by adding flags.
                return ExitKind::Usage.into();
            }
        }
    } else {
        let mut prompt = StdioPrompt::stdio();
        match run_interactive(&mut prompt).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("zero: init failed: {e}");
                // Interactive-prompt I/O error — closest bucket
                // is `Internal` (unexpected std{in,out} failure).
                return ExitKind::Internal.into();
            }
        }
    };

    if opts.dry_run {
        println!("# dry-run plan (not written)");
        println!("{}", plan.summary());
        return ExitKind::Ok.into();
    }

    match plan.apply() {
        Ok(receipt) => {
            println!("zero: configured.");
            println!("  config:   {}", receipt.config_path.display());
            println!(
                "  token:    {}",
                if receipt.token_in_keychain {
                    "stored in OS keychain"
                } else {
                    "not stored (engine may not require auth)"
                }
            );
            println!(
                "  welcome:  {}",
                if receipt.welcome_recorded {
                    "milestone recorded"
                } else {
                    "milestone skipped (session store unavailable)"
                }
            );
            if !plan.engine_reachable {
                println!();
                println!("warning: engine was not reachable during setup.");
                println!("run `zero doctor` once the engine is up to confirm.");
            }
            ExitKind::Ok.into()
        }
        Err(e) => {
            eprintln!("zero: apply failed: {e}");
            // `plan.apply` does filesystem + keychain writes.
            // A failure here is neither the operator's fault
            // (arguments were fine — we got this far) nor an
            // auth issue against the engine; it is internal
            // CLI I/O and worth a bug report.
            ExitKind::Internal.into()
        }
    }
}

fn build_client(cli: &Cli) -> Option<HttpClient> {
    let api_url = zero_config::resolve_api_url(cli.api.as_deref());
    let token = zero_config::resolve_token(cli.token.as_deref());
    match HttpClient::new(&api_url, token) {
        Ok(c) => {
            // Every production client gets the default-size rate
            // budget attached. Operators who hit it read a typed
            // refusal line ("rate: exhausted — retry in Ns") instead
            // of watching the CLI freeze on a retry loop, and the
            // status bar's `rate:N/M` segment reads from the same
            // bucket via `HttpClient::rate_budget`.
            let mut client = c.with_rate_budget(zero_engine_client::RateBudget::default_system());
            // M2 §5: `--paper` is no longer a stderr advisory.
            // Attach the mode override so every outbound request
            // carries `X-Zero-Mode: paper`. Live is the default
            // (no header), matching the engine's launch-time
            // posture; no need to emit `live` explicitly.
            if cli.paper {
                client = client.with_mode(zero_engine_client::Mode::Paper);
            }
            if let Ok(Some(cfg)) = zero_config::load_config()
                && !cfg.identity.handle.trim().is_empty()
            {
                client = client.with_operator_context(
                    zero_engine_client::OperatorRequestContext::local(cfg.identity.handle),
                );
            }
            Some(client)
        }
        Err(e) => {
            eprintln!("zero: invalid api url {api_url:?}: {e}");
            None
        }
    }
}

async fn run_doctor(cli: &Cli, fix: bool, format: Option<Format>) -> ExitCode {
    let format = format.unwrap_or(if cli.json { Format::Json } else { Format::Text });

    let client = build_client(cli);
    let config_dir = zero_config::zero_dir().unwrap_or_else(|_| std::env::temp_dir());

    // Resolve the WS URL from the same HTTP base the client uses.
    // Falling back to the default API URL here — not the client's
    // configured URL — keeps the doctor useful even when
    // `build_client` returned None (e.g. no token and the user
    // wanted to check raw engine reachability pre-`zero init`).
    let http_url_str = zero_config::resolve_api_url(cli.api.as_deref());
    let ws_url = url::Url::parse(&http_url_str)
        .ok()
        .and_then(|u| ws_url_from_http(&u));

    let mut builder = zero_doctor::Doctor::builder()
        .client(client)
        .config_dir(config_dir)
        .fix(fix);
    if let Some(ws) = ws_url {
        builder = builder.ws_url(ws);
        // Attach the resolved token so the probe hits `/ws` the same
        // way the TUI does. Without this, a token-gated engine
        // rejects the handshake with 403 and the doctor misreports
        // the WS surface as broken.
        if let Some(token) = zero_config::resolve_token(cli.token.as_deref())
            && !token.is_empty()
        {
            builder = builder.ws_token(token);
        }
    }
    let doctor = builder.build();

    let report = doctor.run().await;

    match format {
        Format::Json => match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("zero: serialize report: {e}");
                return ExitKind::Internal.into();
            }
        },
        Format::Text => {
            println!("{}", report.render_text());
        }
    }

    // `zero-doctor` emits 0 (all-ok/warn/repaired) or 2 (fail).
    // Both map cleanly onto our taxonomy — 0 is `Ok`, 2 is
    // `EngineUnreachable` because every doctor fail today is an
    // engine-reachability or config-sanity check. Any drift in
    // `zero-doctor`'s exit-code set will fall through to
    // `Internal`, which is exactly what should happen if a new
    // status shows up the binary does not yet understand.
    match report.exit_code() {
        0 => ExitKind::Ok.into(),
        2 => ExitKind::EngineUnreachable.into(),
        _ => ExitKind::Internal.into(),
    }
}

/// Derive a WebSocket URL from an HTTP base URL. `http` →  `ws`,
/// `https` → `wss`, path set to `/ws`. Returns `None` when the
/// scheme is not http(s); the caller skips WS in that case.
fn ws_url_from_http(http: &url::Url) -> Option<String> {
    let scheme = match http.scheme() {
        "http" => "ws",
        "https" => "wss",
        _ => return None,
    };
    let host = http.host_str()?;
    let port = http
        .port_or_known_default()
        .map_or_else(String::new, |p| format!(":{p}"));
    Some(format!("{scheme}://{host}{port}/ws"))
}

async fn run_tui(cli: &Cli) -> ExitCode {
    use zero_tui::App;
    use zero_tui::app::log::{EntryKind, LogEntry};
    use zero_tui::app::session::SessionSink;

    let Some(client) = build_client(cli) else {
        // Invalid API URL — `build_client` already printed the
        // parse error. This is a usage failure (the operator's
        // `--api` flag / `ZERO_API_URL` / config is malformed).
        return ExitKind::Usage.into();
    };

    let engine_state = EngineState::shared();
    let token = zero_config::resolve_token(cli.token.as_deref());

    // Spawn the WS subscriber if we can derive a ws:// URL. If the
    // engine is unreachable, the TUI still launches; the status bar
    // shows DOWN, and the subscriber keeps trying in the background.
    let subscriber =
        ws_url_from_http(client.base_url()).and_then(|ws_url| {
            match WsSubscriber::spawn(&ws_url, token.clone(), engine_state.clone()) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!(err = %e, "ws subscriber failed to spawn");
                    None
                }
            }
        });

    // Operator-state poller — the CLI's only window into the
    // engine-hosted behavioral classifier (ADR-016). Uses the same
    // HTTP client as the dispatcher so auth + retry stays uniform.
    let operator_poller = OperatorStatePoller::spawn(client.clone(), engine_state.clone());

    // HTTP backfill poller — defense-in-depth for the core mirror
    // fields (status / positions / risk / regime). Runs on a slow
    // 30 s cadence; the WS subscriber remains primary. See
    // `zero_engine_client::poll` for the full contract: writes are
    // tagged `Source::Http` and do **not** bump `last_heartbeat`.
    let state_poller = EngineStatePoller::spawn(client.clone(), engine_state.clone());

    let base_url_string = client.base_url().as_str().to_string();
    let mut ctx = DispatchContext::new(Some(client), engine_state.clone());

    // Open the session store and start a new session, unless the
    // operator passed `--no-persist`. A failed open degrades to
    // "in-memory only" plus an advisory line in the log.
    let mut prelude: Vec<LogEntry> = Vec::new();
    let sink: Option<SessionSink> = if cli.no_persist {
        prelude.push(LogEntry::new(
            EntryKind::System,
            "persistence disabled (--no-persist) — this session will not be recorded",
        ));
        None
    } else {
        match open_session_store(&base_url_string, &mut prelude) {
            Ok(s) => Some(s),
            Err(msg) => {
                prelude.push(LogEntry::new(EntryKind::Warn, msg));
                None
            }
        }
    };
    // Attach the session adapter to the dispatch context *before*
    // it is cloned into `App`; the session cohort commands
    // (`/sessions`, `/resume`, `/fork`, `/save`) consult the
    // adapter at dispatch time. Adapter shares the `ActiveSession`
    // handle with the sink, so `/fork` atomically rotates the
    // write target without re-plumbing the context.
    if let Some(s) = sink.as_ref() {
        ctx = ctx.with_sessions(std::sync::Arc::new(s.adapter()));
    }
    // Attach the config introspection adapter. This is always
    // wired (unlike the session store, which respects
    // --no-persist) because config reads do not touch the
    // database — they consult the on-disk TOML and the OS
    // keychain, both of which are idempotent and safe to
    // query from a `--no-persist` run.
    ctx = ctx.with_config(std::sync::Arc::new(ConfigAdapter::new(
        cli.api.clone(),
        cli.token.clone(),
    )));
    // M2 §5: attach the `/auto` + `/headless` adapter surfaces.
    //
    // The CLI ships **stub** implementations at this layer —
    // `zero-commands::MockAutoSource` flips an in-memory mode
    // and `MockSupervisorSource` tracks daemon running-state
    // without actually spawning anything. The real HTTP-backed
    // `AutoSource` lands with the engine's `POST
    // /operator/auto` endpoint; the real `SupervisorSource`
    // lands with the `zero-headless` crate (M2 §6). Wiring
    // them through the trait here means both landings are
    // one-line swaps at this site, not a new plumbing pass.
    //
    // `/auto` uses the CLI process's current `--paper` flag as
    // the initial mode — an operator launching `zero --paper`
    // and then typing `/auto status` reads `mode=off` initially
    // (paper mode is orthogonal to auto mode), which is the
    // honest answer.
    ctx = ctx.with_auto(std::sync::Arc::new(zero_commands::MockAutoSource::new(
        zero_commands::AutoMode::Off,
    )));
    // M2 §6: the real `SupervisorSource` adapter talks to
    // `zero-headlessd` over `~/.zero/sock`. The dispatcher
    // surface is sync (so `/kill` can't deadlock), but the
    // client is async — the adapter bridges the two via
    // `block_in_place` on the ambient multi-threaded tokio
    // runtime (`#[tokio::main]` default). If the daemon is
    // not running, `/headless status` answers "stopped" and
    // `/headless start` surfaces an `Unavailable` alert
    // pointing the operator at `zero headless install`
    // (landed in §8) — no silent pretending.
    let socket_path = zero_headless::default_socket_path();
    let runtime_handle = tokio::runtime::Handle::current();
    ctx = ctx.with_supervisor(std::sync::Arc::new(
        supervisor_adapter::HeadlessSupervisorAdapter::new(socket_path, runtime_handle),
    ));
    // Keep a clone of the sink for post-run bookkeeping (the
    // daily wrap generator reads through it). `Clone` on
    // `SessionSink` shares the underlying `Arc<Store>` and the
    // `Arc<Mutex<ActiveSession>>`, so the post-run clone sees
    // the same session row the event loop was writing to —
    // even if a `/fork` reassigned mid-run, the wrap runs
    // against the currently-live row, which is the honest
    // answer: "what was the operator actually doing when they
    // quit?"
    let wrap_sink: Option<zero_tui::app::session::SessionSink> = sink.clone();

    let mut app = if let Some(s) = sink {
        App::new_with_sink(engine_state, ctx, s)
    } else {
        App::new(engine_state, ctx)
    };

    // Attach the WS broadcast receiver to the app so the
    // live-stream pane can render engine events in near-real
    // time. Missing subscriber (engine unreachable at launch)
    // is fine — the pane renders its honest empty state.
    if let Some(sub) = subscriber.as_ref() {
        app = app.with_events(sub.events());
    }

    // Seed replay + any prelude notes *silently* so they are not
    // double-persisted on this session's first commit.
    let state = app.state_mut();
    for entry in prelude {
        state.append_silent(entry);
    }

    let result = app.run().await;

    if let Some(s) = subscriber {
        let _ = s.shutdown().await;
    }
    let _ = operator_poller.shutdown().await;
    let _ = state_poller.shutdown().await;

    match result {
        Ok(exit) => {
            // Post-session daily-wrap pass. Runs only on a
            // clean shutdown (an error leaves the session in
            // an unknown state and a wrap of a half-captured
            // session would be dishonest); gated on
            // `!wrap_off` per Addendum A §9.1 + gated on a
            // session store being present (a `--no-persist`
            // run has nothing to wrap).
            if !exit.wrap_off
                && let Some(sink) = wrap_sink
            {
                run_daily_wrap(&sink);
            }
            ExitKind::Ok.into()
        }
        Err(e) => {
            eprintln!("zero: tui error: {e}");
            // `app.run()` returns errors from crossterm /
            // ratatui / tokio — every failure mode here is
            // CLI-internal (terminal reset, event-loop crash,
            // render backend). Not `AuthInvalid`: the event
            // loop never round-trips auth, so auth failures
            // would have bubbled up earlier via `build_client`
            // or the engine-state mirror's advisory lines.
            ExitKind::Internal.into()
        }
    }
}

/// Run the daily-wrap pass against a freshly-ended session.
///
/// This is strictly best-effort: every failure mode here is
/// advisory, never fatal. The session has already committed
/// the moment `SessionSink::end()` returned; the wrap is a
/// post-hoc artifact, and an operator who just quit should
/// never see a stack trace because their wraps dir ran out
/// of space. Every step logs on failure and returns quietly.
fn run_daily_wrap(sink: &zero_tui::app::session::SessionSink) {
    let Some(session_id) = sink.session_id() else {
        return;
    };
    let Some(ulid) = sink.ulid() else {
        return;
    };
    let store = sink.store();

    // Fetch the session row so we know the true `started_at`
    // (the event-loop does not track it). `get_session_by_ulid`
    // returns `Ok(None)` only if someone deleted the row under
    // us — treat that as "nothing to wrap."
    let row = match store.get_session_by_ulid(&ulid) {
        Ok(Some(r)) => r,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(err = %e, "wrap: session row fetch failed");
            return;
        }
    };

    // Cap events read at 10k — a runaway session that emitted
    // 100k lines would make the wrap generator allocate
    // heavily; for the top-N + counts we only need a
    // representative sample. 10k is far larger than any
    // reasonable operator session and far smaller than a
    // runaway log. A future `/brief`-style summary that needs
    // full fidelity can take the slow path through the store
    // directly.
    let events = match store.list_events(session_id, 10_000) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(err = %e, "wrap: event list failed");
            return;
        }
    };

    let now = chrono::Utc::now();
    if !zero_session::wrap::should_wrap(&row, &events, now) {
        return;
    }

    let report = zero_session::wrap::generate(&row, &events, now);

    // Write under `~/.zero/state/wraps/` — a subdirectory of
    // the config dir, *not* alongside `state.db`. Keeping
    // wraps in their own subdir means a future `/wraps`
    // command listing them does not have to filter the DB
    // file out.
    let wraps_dir = match zero_config::zero_dir() {
        Ok(d) => d.join("state").join("wraps"),
        Err(e) => {
            tracing::warn!(err = %e, "wrap: zero_dir failed");
            return;
        }
    };

    match zero_session::wrap::write_wrap(&wraps_dir, &report) {
        Ok(path) => {
            // A single advisory line on stderr so the operator
            // sees where the wrap landed without the TUI
            // stealing their terminal back. stderr (not stdout)
            // because a script that pipes `zero` is reading
            // stdout for structured output; the wrap notice is
            // log-channel content.
            eprintln!("zero: wrap saved — {}", path.display());
        }
        Err(e) => {
            tracing::warn!(err = %e, "wrap: write failed");
        }
    }
}

/// The honest welcome — shown exactly once per persistent
/// store, gated by the [`milestones::WELCOME_SHOWN`] flag. The
/// copy is deliberately medically honest (Addendum A §15's
/// "no cute, no gamified, no playful" rule): it tells the
/// operator what this CLI is, what it is not, and the two
/// commands that matter on day one. It does not congratulate
/// them for installing the binary.
///
/// Each element is a single pre-formatted line; the TUI's log
/// renderer reflows by character count but we keep the lines
/// short enough to render cleanly at 80 columns. Trailing
/// colons on label-style lines mirror the rest of the
/// conversation pane's convention.
///
/// The first element is always an `EntryKind::System` with the
/// thin horizontal marker — it lets the operator see at a
/// glance that what follows is the welcome block and not a
/// prior-session replay. The last element is a blank so the
/// next line in the log has breathing room.
const WELCOME_LINES: &[(&str, &str)] = &[
    (
        "system",
        "── welcome ──────────────────────────────────────",
    ),
    ("system", "zero is your operator terminal."),
    ("system", "intelligence, not automation."),
    ("system", ""),
    (
        "system",
        "the engine is the source of truth; this CLI is a renderer + dispatcher.",
    ),
    (
        "system",
        "the operator-state segment on the status bar is always visible — never hidden.",
    ),
    (
        "system",
        "risk-reducing commands (/kill, /flatten-all, /close, /pause-entries, /break) are friction-exempt. always.",
    ),
    ("system", ""),
    ("system", "two commands to get started:"),
    (
        "system",
        "  /help  — the full surface, grouped by risk direction.",
    ),
    ("system", "  /status — what the engine sees right now."),
    ("system", ""),
    (
        "system",
        "this welcome shows once. re-read it any time with /help.",
    ),
    (
        "system",
        "─────────────────────────────────────────────────",
    ),
    ("system", ""),
];

/// Push the welcome lines into `prelude` when the
/// `WELCOME_SHOWN` milestone is unset. The milestone is set to
/// a timestamp so a future re-tool (a "what-changed-since"
/// command, an analytics rollup) can tell how long ago the
/// welcome was shown without the binary having to parse
/// booleans as timestamps.
///
/// Idempotent on the read side: a session with the milestone
/// already set inserts nothing. Best-effort on the write side:
/// if the milestone-set call fails (disk full, readonly mount),
/// we log and return `Ok(())` rather than failing the launch —
/// seeing the welcome twice is worth less than losing the
/// session entirely. The next successful launch rewrites the
/// milestone.
fn maybe_push_welcome(
    store: &zero_session::Store,
    prelude: &mut Vec<zero_tui::app::log::LogEntry>,
) {
    use zero_session::milestones::WELCOME_SHOWN;
    use zero_tui::app::log::{EntryKind, LogEntry};

    match store.get_milestone(WELCOME_SHOWN) {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(e) => {
            // A read failure is surprising (the table exists;
            // the column is selected verbatim). Surface it on
            // stderr and show the welcome anyway — the "wrong"
            // side here is not showing it.
            eprintln!("zero: milestones read failed ({e}); showing welcome defensively.");
        }
    }

    for (kind, text) in WELCOME_LINES {
        let ek = match *kind {
            "system" => EntryKind::System,
            // The array is a static compile-time constant so
            // any other value here is a programmer error.
            // Fall back to System so we never panic on a
            // typo, but debug_assert so tests catch it.
            other => {
                debug_assert!(false, "unknown welcome-line kind: {other}");
                EntryKind::System
            }
        };
        prelude.push(LogEntry::new(ek, *text));
    }

    // Persist the milestone after the prelude lines are
    // queued. `now()` as RFC3339 is what the other milestones
    // use (`FIRST_LIVE_TRADE_AT` is documented as ISO-8601);
    // keep them consistent.
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = store.set_milestone(WELCOME_SHOWN, &now) {
        eprintln!("zero: could not persist welcome milestone ({e}); it will re-show next launch.");
    }
}

/// Open the on-disk session store, replay the last session into
/// `prelude`, and start a new row. Returns a ready-to-use sink.
fn open_session_store(
    base_url: &str,
    prelude: &mut Vec<zero_tui::app::log::LogEntry>,
) -> Result<zero_tui::app::session::SessionSink, String> {
    use std::sync::Arc;
    use zero_tui::app::log::{EntryKind, LogEntry};
    use zero_tui::app::session::{SessionSink, replay, summarize};

    let dir = zero_config::zero_dir().map_err(|e| format!("session: {e}"))?;
    let db_path = dir.join("state.db");
    let store = Arc::new(zero_session::Store::open(&db_path).map_err(|e| format!("session: {e}"))?);

    // Welcome comes *before* replay so a first-ever session
    // reads welcome → (no replay, nothing prior exists) →
    // prompt. A resuming session reads welcome (if unset — a
    // `--no-persist` operator finally persisting for the first
    // time gets the welcome here) → replay → prompt. The
    // milestone is persistent so the next launch skips this.
    maybe_push_welcome(&store, prelude);

    // Replay the most recent prior session into the log before we
    // open a new one, so the operator lands in context.
    if let Ok(Some(prev)) = store.last_session()
        && let Ok(events) = store.list_events(prev.id, 200)
        && !events.is_empty()
    {
        prelude.push(LogEntry::new(
            EntryKind::System,
            summarize(&prev, events.len()),
        ));
        prelude.extend(replay(&events));
    }

    let ulid = new_ulid();
    let id = store
        .start_session(&ulid, Some(base_url), env!("CARGO_PKG_VERSION"), None)
        .map_err(|e| format!("session: {e}"))?;
    Ok(SessionSink::new(store, id, ulid))
}

/// Production `ConfigSource`.
///
/// Reads the on-disk TOML (if any), layers the per-invocation
/// overrides (`--api`, `--token`) on top of env-var + keychain
/// lookups so `/config show` reports what the rest of the
/// process actually sees — not a raw file dump that would
/// drift from runtime behavior.
///
/// `/config doctor` performs the same checks as the
/// non-interactive `zero doctor` subcommand plus a few TUI-
/// specific ones (session-store path writable, theme string
/// parseable). The work is intentionally read-only — no config
/// is rewritten from a slash command.
struct ConfigAdapter {
    api_override: Option<String>,
    token_override: Option<String>,
}

impl ConfigAdapter {
    fn new(api_override: Option<String>, token_override: Option<String>) -> Self {
        Self {
            api_override,
            token_override,
        }
    }

    fn resolved_token_source(&self) -> &'static str {
        // Replicates the precedence ladder in
        // `zero_config::resolve_token` so /config show can tell
        // the operator *where* their token came from — a much
        // more useful answer than "set / unset."
        if self
            .token_override
            .as_deref()
            .is_some_and(|s| !s.is_empty())
        {
            "flag"
        } else if std::env::var(zero_config::env::API_TOKEN)
            .ok()
            .is_some_and(|s| !s.is_empty())
        {
            "env"
        } else if matches!(zero_config::keyring_read_engine_token(), Ok(Some(_))) {
            "keychain"
        } else {
            "unset"
        }
    }
}

impl zero_commands::ConfigSource for ConfigAdapter {
    fn show(&self) -> Vec<zero_commands::ConfigShowRow> {
        use zero_commands::ConfigShowRow as R;
        let mut rows: Vec<R> = Vec::new();

        let api_url = zero_config::resolve_api_url(self.api_override.as_deref());
        rows.push(R::new("api_url", api_url));
        rows.push(R::new(
            "api_url source",
            api_url_source(self.api_override.as_ref()),
        ));
        rows.push(R::new("token", self.resolved_token_source()));

        // Config file path + whether it exists. Showing the
        // path even when absent gives the operator a concrete
        // place to create the file — more actionable than a
        // bare "(no config)" line.
        match zero_config::config_path() {
            Ok(p) => {
                let exists = p.exists();
                rows.push(R::new("config file", p.display().to_string()));
                rows.push(R::new(
                    "config file exists",
                    if exists { "yes" } else { "no" },
                ));
            }
            Err(e) => rows.push(R::new("config file", format!("(unavailable: {e})"))),
        }

        match zero_config::load_config() {
            Ok(Some(cfg)) => {
                rows.push(R::new("handle", cfg.identity.handle.clone()));
                rows.push(R::new(
                    "email",
                    cfg.identity.email.clone().unwrap_or_else(|| "—".into()),
                ));
                rows.push(R::new("default mode", cfg.mode.default.clone()));
                rows.push(R::new("theme", cfg.display.theme.clone()));
                rows.push(R::new(
                    "live-stream default",
                    if cfg.display.live_stream_default {
                        "on"
                    } else {
                        "off"
                    },
                ));
                rows.push(R::new(
                    "verbose default",
                    cfg.display.verbose_default.clone(),
                ));
                rows.push(R::new(
                    "max position %",
                    format!("{:.1}", cfg.guardrails.max_position_pct),
                ));
                rows.push(R::new(
                    "max concurrent",
                    cfg.guardrails.max_concurrent.to_string(),
                ));
                rows.push(R::new(
                    "daily loss %",
                    format!("{:.1}", cfg.guardrails.daily_loss_pct),
                ));
                rows.push(R::new(
                    "drawdown %",
                    format!("{:.1}", cfg.guardrails.drawdown_pct),
                ));
            }
            Ok(None) => {
                rows.push(R::new("config file state", "absent (run `zero init`)"));
            }
            Err(e) => {
                rows.push(R::new("config file state", format!("(parse error: {e})")));
            }
        }
        rows
    }

    fn doctor(&self) -> Vec<zero_commands::ConfigDoctorFinding> {
        use zero_commands::ConfigDoctorFinding as F;
        let mut findings: Vec<F> = Vec::new();

        match zero_config::config_path() {
            Ok(p) if p.exists() => {
                findings.push(F::ok(format!("config file found at {}", p.display())));
            }
            Ok(p) => findings.push(F::warn(format!(
                "config file missing at {} — run `zero init`",
                p.display()
            ))),
            Err(e) => findings.push(F::error(format!("cannot resolve config dir: {e}"))),
        }

        match zero_config::load_config() {
            Ok(Some(cfg)) => {
                if cfg.version == zero_config::CONFIG_VERSION {
                    findings.push(F::ok(format!("config schema v{} current", cfg.version)));
                } else {
                    findings.push(F::warn(format!(
                        "config schema v{} older than v{}",
                        cfg.version,
                        zero_config::CONFIG_VERSION
                    )));
                }
                if cfg.identity.handle.is_empty() {
                    findings.push(F::warn("identity.handle empty — run `zero init`"));
                } else {
                    findings.push(F::ok(format!(
                        "identity.handle = '{}'",
                        cfg.identity.handle
                    )));
                }
            }
            Ok(None) => findings.push(F::warn("no config loaded")),
            Err(e) => findings.push(F::error(format!("config parse error: {e}"))),
        }

        match self.resolved_token_source() {
            // Honesty bar: earlier copy said "or run `zero pair`."
            // `zero pair` does not exist in M1 (see the comment in
            // `run_init` re: "later milestone"). Telling an
            // operator in a broken-auth state to run a
            // non-existent command is exactly the kind of paper
            // cut a trading-terminal CLI cannot afford — the
            // operator types it, gets "unknown command", and now
            // the tool looks broken on top of the actual problem.
            // Until `zero pair` ships, the remediation path is
            // either env/flag (fastest) or `zero init --force`
            // (persistent). Both are reachable today.
            "unset" => findings.push(F::error(
                "engine token unset — pass --token, set ZERO_API_TOKEN, or run `zero init --force`",
            )),
            src => findings.push(F::ok(format!("engine token resolved via {src}"))),
        }

        // Keychain reachability probe. A raw keychain error
        // (secret service down on Linux, locked login keychain
        // on macOS) is actionable on its own, separate from
        // "no token stored."
        match zero_config::keyring_read_engine_token() {
            Ok(_) => findings.push(F::ok("keychain reachable")),
            Err(e) => findings.push(F::warn(format!(
                "keychain not reachable ({e}) — env + flag fallback still works"
            ))),
        }

        // Session-store directory writability. Failures here
        // land as warnings because the TUI still runs
        // (in-memory only) but session persistence is lost.
        match zero_config::zero_dir() {
            Ok(dir) => match std::fs::create_dir_all(&dir) {
                Ok(()) => findings.push(F::ok(format!(
                    "session-store dir writable: {}",
                    dir.display()
                ))),
                Err(e) => findings.push(F::warn(format!(
                    "session-store dir not writable ({e}) — sessions will not persist"
                ))),
            },
            Err(e) => findings.push(F::warn(format!("session-store dir unavailable: {e}"))),
        }

        findings
    }
}

fn api_url_source(override_: Option<&String>) -> &'static str {
    if override_.map(String::as_str).is_some_and(|s| !s.is_empty()) {
        "flag"
    } else if std::env::var(zero_config::env::API_URL)
        .ok()
        .is_some_and(|s| !s.is_empty())
    {
        "env"
    } else {
        "default"
    }
}

/// Minimal ULID-ish id — time-sortable base32 string suitable for
/// session tagging. We avoid pulling in the `ulid` crate just for
/// this one call site.
fn new_ulid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Pad to 13 hex digits (covers ms into year 2286) + 6 random
    // base36 chars for uniqueness across concurrent starts.
    let rand = fastrand_hex(6);
    format!("{ms:013x}{rand}")
}

fn fastrand_hex(n: usize) -> String {
    // Tiny LCG — good enough to disambiguate simultaneous starts
    // within a single millisecond. Not used for anything security-
    // sensitive.
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut state: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x9E37_79B9_7F4A_7C15, |d| {
            u64::try_from(d.as_nanos()).unwrap_or(0x9E37_79B9_7F4A_7C15)
        });
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let c = ((state >> 33) & 0x1F) as u8;
            if c < 10 {
                (b'0' + c) as char
            } else {
                (b'a' + c - 10) as char
            }
        })
        .collect()
}

/// Run a single slash-command non-interactively, render the
/// result to stdout, and exit.
///
/// Contract:
/// - Leading `/` is optional in the input tokens; we prepend
///   one so `zero run status` and `zero run /status` are
///   interchangeable. This keeps shell-quoting honest for
///   operators who forget the slash.
/// - Risk-*Increasing* commands are refused with a usage
///   message + `ExitKind::Usage`. The refusal is deliberate:
///   the friction ladder's typed-confirm surface cannot exist
///   in a non-TTY pipe, and accepting `Increases` here would
///   be a silent friction bypass (§6.3 asymmetry invariant).
/// - `Reduces` commands *run*: `/kill`, `/flatten-all`,
///   `/close`, `/pause-entries`, `/break`. An operator
///   scripting a kill-switch cron is a valid use case.
/// - The session store is skipped (`--no-persist` semantics
///   implicit) — a one-shot invocation must not rotate the
///   active session row. Config introspection (`/config
///   show/doctor`) is still wired via `ConfigAdapter`.
/// - `--json` emits a structured array so scripts get machine-
///   parseable output; text mode uses the same OutputLine
///   rendering the TUI does, minus the color/modifier ANSI
///   codes that would confuse a pipe.
async fn run_oneshot(cli: &Cli, input: &[String]) -> ExitCode {
    use zero_commands::{OutputLine, RiskDirection, command::resolve, parse::parse_line};

    // Reconstruct the command line the dispatcher expects. The
    // tokens come in already split by the shell, so we re-join
    // with single spaces and prepend `/` if the operator
    // omitted it. A bare `zero run` hits clap's `required = true`
    // earlier, so `input` is always non-empty here.
    let mut joined = input.join(" ");
    if !joined.starts_with('/') {
        joined.insert(0, '/');
    }

    // Resolve the command up front so we can refuse `Increases`
    // *before* hitting the engine. Skipping the HTTP round-trip
    // on a refused command is the difference between "fail fast
    // with a clear message" and "fail with a confusing network
    // error because the engine was down anyway."
    let parsed = parse_line(&joined);
    let Some(cmd) = resolve(&parsed) else {
        // Empty input. Clap's `required = true` should have
        // caught this; kept here for the parse-to-None case
        // (e.g. whitespace-only tokens).
        eprintln!("zero run: empty command");
        return ExitKind::Usage.into();
    };

    if matches!(cmd.risk(), RiskDirection::Increases) {
        // Honest refusal. The taxonomy is `Usage` because the
        // operator can fix the invocation (pick a different
        // command, or run interactively).
        eprintln!(
            "zero run: refusing to run risk-increasing command '{name}' non-interactively.",
            name = cmd.name(),
        );
        eprintln!(
            "  Increasing-risk commands require the friction ladder's typed-confirm surface,"
        );
        eprintln!("  which needs a TTY. Run `zero` (no args) to use the interactive terminal.");
        return ExitKind::Usage.into();
    }

    // Build a minimal dispatch context. No session sink (one-
    // shot runs must not rotate the session row), no WS/poller
    // background tasks (we exit after one command), but we do
    // wire the HTTP client + the config adapter so engine-
    // backed commands and `/config show` both work.
    let Some(client) = build_client(cli) else {
        return ExitKind::Usage.into();
    };
    let engine_state = EngineState::shared();
    let mut ctx = DispatchContext::new(Some(client), engine_state);
    ctx = ctx.with_config(std::sync::Arc::new(ConfigAdapter::new(
        cli.api.clone(),
        cli.token.clone(),
    )));

    // `dispatch` returns `Ok(None)` only when `resolve` would
    // also have returned `None` — we already ruled that out
    // above, so this is an honest safety net, not an expected
    // path. Using `let...else` keeps the happy path flat.
    let Ok(Some(out)) = zero_commands::dispatch(&ctx, &joined).await else {
        eprintln!("zero run: empty command");
        return ExitKind::Usage.into();
    };

    // Render every `OutputLine` to stdout. Text mode prepends
    // a one-char prefix mirroring the TUI's color roles so a
    // piped log is still scannable; JSON mode emits a single
    // array with `{kind, text}` objects.
    let mut sticky_exit: Option<ExitKind> = None;
    if cli.json {
        let arr: Vec<serde_json::Value> = out
            .lines
            .iter()
            .map(|l| {
                let (kind, text) = match l {
                    OutputLine::System(s) => ("system", s.as_str()),
                    OutputLine::Command(s) => ("command", s.as_str()),
                    OutputLine::Warn(s) => ("warn", s.as_str()),
                    OutputLine::Alert(s) => ("alert", s.as_str()),
                };
                serde_json::json!({ "kind": kind, "text": text })
            })
            .collect();
        match serde_json::to_string_pretty(&arr) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("zero run: serialize output: {e}");
                return ExitKind::Internal.into();
            }
        }
    } else {
        // Text rendering. `System` and `Command` share the
        // `  ` prefix because the pipe-reader has no color and
        // the structural distinction (informational vs. engine
        // output) is not worth a prefix that would be confused
        // for a shell comment. `Warn` and `Alert` get
        // progressively louder prefixes so `grep '^!'` catches
        // both in scripts.
        for l in &out.lines {
            match l {
                OutputLine::System(s) | OutputLine::Command(s) => println!("  {s}"),
                OutputLine::Warn(s) => println!("! {s}"),
                OutputLine::Alert(s) => println!("!! {s}"),
            }
        }
    }

    // An `Alert` in the output is how the dispatcher reports a
    // remote error (HTTP 500, timeout, engine-down). Propagate
    // it to the exit code so scripts can branch — silent
    // success under `!! engine unreachable` would be the exact
    // dishonesty §9 rules out.
    for l in &out.lines {
        if matches!(l, OutputLine::Alert(_)) {
            sticky_exit = Some(ExitKind::EngineUnreachable);
            break;
        }
    }

    sticky_exit.unwrap_or(ExitKind::Ok).into()
}

async fn run_version(cli: &Cli) -> ExitCode {
    // Three shapes of output (text-reachable, text-unreachable,
    // text-badurl) and their JSON mirrors. We fan out early so
    // the JSON path is not an afterthought bolt-on: each exit
    // has an explicit `return` so a future contributor cannot
    // accidentally print a text line under --json.
    let cli_version = env!("CARGO_PKG_VERSION");

    let Some(client) = build_client(cli) else {
        // `build_client` already printed the parse error on
        // stderr in the text path; repeat a minimal marker on
        // the json path so scripts still see a parseable
        // object.
        if cli.json {
            let obj = serde_json::json!({
                "cli_version": cli_version,
                "engine_version": null,
                "engine_status": null,
                "engine_url": null,
                "engine_reachable": false,
                "error": "invalid api url",
            });
            match serde_json::to_string_pretty(&obj) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    eprintln!("zero: serialize version: {e}");
                    return ExitKind::Internal.into();
                }
            }
        } else {
            println!("zero {cli_version} (cli)");
        }
        return ExitKind::Usage.into();
    };

    let base_url = client.base_url().as_str().to_owned();
    match client.root().await {
        Ok(root) => {
            if cli.json {
                let obj = serde_json::json!({
                    "cli_version": cli_version,
                    "engine_version": root.version,
                    "engine_status": root.status,
                    "engine_url": base_url,
                    "engine_reachable": true,
                });
                match serde_json::to_string_pretty(&obj) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("zero: serialize version: {e}");
                        return ExitKind::Internal.into();
                    }
                }
            } else {
                println!("zero {cli_version} (cli)");
                println!("engine {} ({}) — {base_url}", root.version, root.status);
            }
            ExitKind::Ok.into()
        }
        Err(e) => {
            // Engine root probe failed — DNS / TCP / 5xx /
            // timeout. `AuthInvalid` would be a stretch
            // because `GET /` does not require a token on our
            // engine. Stays `EngineUnreachable`.
            if cli.json {
                let obj = serde_json::json!({
                    "cli_version": cli_version,
                    "engine_version": null,
                    "engine_status": null,
                    "engine_url": base_url,
                    "engine_reachable": false,
                    "error": e.to_string(),
                });
                match serde_json::to_string_pretty(&obj) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("zero: serialize version: {e}");
                        return ExitKind::Internal.into();
                    }
                }
            } else {
                println!("zero {cli_version} (cli)");
                println!("engine unreachable at {base_url}: {e}");
            }
            ExitKind::EngineUnreachable.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_session::{Store, milestones::WELCOME_SHOWN};
    use zero_tui::app::log::{EntryKind, LogEntry};

    #[test]
    fn welcome_appears_exactly_once_on_first_session() {
        let store = Store::open_in_memory().expect("open in-memory store");
        assert_eq!(
            store.get_milestone(WELCOME_SHOWN).unwrap(),
            None,
            "fresh store has no welcome milestone"
        );

        let mut prelude: Vec<LogEntry> = Vec::new();
        maybe_push_welcome(&store, &mut prelude);

        assert!(
            !prelude.is_empty(),
            "first-ever call must push at least one welcome line"
        );
        assert_eq!(
            prelude.len(),
            WELCOME_LINES.len(),
            "welcome push must match the WELCOME_LINES constant (len={})",
            WELCOME_LINES.len()
        );
        // Every line is a System entry.
        for e in &prelude {
            assert_eq!(e.kind, EntryKind::System);
        }
        // Milestone got persisted as RFC-3339.
        let stored = store
            .get_milestone(WELCOME_SHOWN)
            .unwrap()
            .expect("milestone persisted");
        assert!(
            chrono::DateTime::parse_from_rfc3339(&stored).is_ok(),
            "welcome milestone should serialize as RFC-3339 (got {stored:?})"
        );
    }

    #[test]
    fn welcome_is_suppressed_on_subsequent_sessions() {
        let store = Store::open_in_memory().expect("open in-memory store");

        // First session — welcome appears.
        let mut first: Vec<LogEntry> = Vec::new();
        maybe_push_welcome(&store, &mut first);
        assert!(!first.is_empty());
        let first_count = first.len();

        // Second session — welcome suppressed.
        let mut second: Vec<LogEntry> = Vec::new();
        maybe_push_welcome(&store, &mut second);
        assert!(
            second.is_empty(),
            "welcome must not re-appear once the milestone is set (got {} lines)",
            second.len()
        );

        // Third, fourth, etc. — remain suppressed.
        for _ in 0..5 {
            let mut nth: Vec<LogEntry> = Vec::new();
            maybe_push_welcome(&store, &mut nth);
            assert!(nth.is_empty());
        }

        // Sanity: first-session still has its exact count.
        assert_eq!(first.len(), first_count);
    }

    #[test]
    fn welcome_is_not_empty_enough_to_be_a_noop() {
        // Guard against a future contributor accidentally
        // dropping WELCOME_LINES to `&[]` — the "exactly
        // once" guarantee becomes vacuous if the welcome is
        // empty. Lock in a minimum line count so a shrink
        // fires this test.
        assert!(
            WELCOME_LINES.len() >= 3,
            "the welcome must carry enough lines to orient a new operator"
        );
        // At least one line mentions /help so the operator can
        // find the rest of the surface.
        assert!(
            WELCOME_LINES.iter().any(|(_, text)| text.contains("/help")),
            "welcome must surface the /help affordance"
        );
    }

    /// Regression guard for the "WARN bleeds into the status bar"
    /// bug. The TUI owns the terminal; any tracing writer that
    /// targets stderr (or stdout) while the TUI is up will
    /// overlay the frame. The fix is a subcommand-based target
    /// decision — when the subcommand is `None` (bare `zero`,
    /// the TUI entrypoint), tracing MUST go to the log file.
    ///
    /// This test pins the mapping without touching the global
    /// subscriber (doing so from a test would leak into the rest
    /// of the suite via `try_init`).
    #[test]
    fn tracing_target_is_log_file_for_tui_entrypoint() {
        let t = tracing_target_for(None);
        assert!(
            matches!(t, TracingTarget::TuiLogFile),
            "bare `zero` launches the TUI and MUST route tracing to the log file"
        );
    }

    #[test]
    fn tracing_target_is_stderr_for_non_tui_subcommands() {
        // Spot-check each non-TUI variant. A future contributor
        // who adds a new TUI-ish subcommand that needs the file
        // writer has to revisit this list, which is the point.
        let doctor = Some(Command::Doctor {
            fix: false,
            format: None,
        });
        let version = Some(Command::Version);
        let run = Some(Command::Run { input: vec![] });
        let init = Some(Command::Init {
            handle: None,
            yes: false,
            non_interactive: false,
            dry_run: false,
            force: false,
        });
        for cmd in [doctor, version, run, init] {
            assert!(
                matches!(tracing_target_for(cmd.as_ref()), TracingTarget::Stderr),
                "non-TUI subcommand {cmd:?} must keep stderr — scripts expect diagnostics there"
            );
        }
    }
}
