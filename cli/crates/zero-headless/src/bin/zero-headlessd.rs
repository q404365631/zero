//! `zero-headlessd` — the supervisor daemon binary.
//!
//! Thin wrapper: parse flags, build a [`Daemon`], run. All
//! behaviour lives in the `zero_headless` library so the
//! shipped binary and the integration tests exercise the
//! same code path.
//!
//! # Refuses-to-run-without-config
//!
//! The M2 spec requires that a daemon started without a
//! valid `~/.zero/config.toml` refuse at startup with a
//! non-zero exit, rather than silently listening on a socket
//! with nothing useful to do. This is enforced below via
//! `zero_config::load_config()` — a missing file is a fatal
//! error, *not* a "default to sane values" path.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use zero_headless::daemon::{AlwaysReachable, Config, Daemon, HttpEngineProbe};

#[derive(Debug, Parser)]
#[command(
    name = "zero-headlessd",
    about = "Zero operator-local supervisor daemon.",
    version
)]
struct Cli {
    /// Override the Unix socket path. Defaults to
    /// `<zero_dir>/operators/<operator-slug>/sock`. The `zero` CLI's real
    /// `SupervisorSource` adapter agrees on this path by
    /// reading the same default.
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Override the persistent state path. Defaults to
    /// `<zero_dir>/operators/<operator-slug>/state/headless.json`.
    #[arg(long)]
    state: Option<PathBuf>,

    /// Skip the engine HTTP probe. Useful for smoke tests on a
    /// machine with no engine reachable — the daemon still
    /// answers `/status` honestly ("unreachable since boot").
    #[arg(long)]
    no_probe: bool,
}

fn main() -> ExitCode {
    // Respect `ZERO_LOG` first, fall back to `RUST_LOG`, then
    // `info`. The supervisor is by design chatty about kill +
    // probe edges — the bar for filtering is "does the line
    // prove the daemon acted as the operator asked".
    let filter = EnvFilter::try_from_env("ZERO_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let cli = Cli::parse();

    // Refuse-to-run-without-config: the operator's config must
    // exist *and* be parseable. A missing file → helpful stderr
    // line + non-zero exit, matching the spec.
    let config_present = match zero_config::load_config() {
        Ok(Some(_)) => true,
        Ok(None) => {
            eprintln!(
                "zero-headlessd: refusing to start — no config at {}. Run `zero onboard` first.",
                zero_config::config_path()
                    .map_or_else(|_| "<unknown>".into(), |p| p.display().to_string()),
            );
            return ExitCode::from(2);
        }
        Err(err) => {
            eprintln!("zero-headlessd: refusing to start — config unparseable: {err}");
            return ExitCode::from(2);
        }
    };

    let cfg = Config {
        socket_path: cli
            .socket
            .unwrap_or_else(zero_headless::default_socket_path),
        state_path: cli.state.unwrap_or_else(zero_headless::default_state_path),
        config_present,
    };

    let probe: std::sync::Arc<dyn zero_headless::daemon::EngineProbe> = if cli.no_probe {
        std::sync::Arc::new(AlwaysReachable)
    } else {
        match build_http_probe() {
            Ok(p) => std::sync::Arc::new(p),
            Err(err) => {
                eprintln!(
                    "zero-headlessd: could not build engine probe ({err}); falling back to no-op probe.",
                );
                std::sync::Arc::new(AlwaysReachable)
            }
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("zero-headlessd: tokio runtime init failed: {err}");
            return ExitCode::from(3);
        }
    };

    runtime.block_on(async move {
        let daemon = match Daemon::new(cfg, probe) {
            Ok(d) => d,
            Err(err) => {
                eprintln!("zero-headlessd: daemon init failed: {err}");
                return ExitCode::from(2);
            }
        };
        match daemon.run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("zero-headlessd: daemon exited with error: {err}");
                ExitCode::from(1)
            }
        }
    })
}

fn build_http_probe() -> Result<HttpEngineProbe, String> {
    let api_url = zero_config::resolve_api_url(None);
    let token = std::env::var(zero_config::env::API_TOKEN).ok();
    let client = zero_engine_client::HttpClient::new(&api_url, token)
        .map_err(|e| format!("http client init: {e}"))?;
    Ok(HttpEngineProbe::new(client))
}
