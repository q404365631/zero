//! End-to-end tests for the `zero` binary.
//!
//! These tests invoke the actual compiled binary (via Cargo's
//! `CARGO_BIN_EXE_zero` env var) so the exit-code taxonomy, the
//! no-TTY help path, the `zero run` subcommand, and the `--json`
//! renderers are all verified *together* — every layer that a
//! script depends on, in the same process that the operator's
//! script would spawn.
//!
//! Why not use `assert_cmd`? We could — the ergonomics are
//! nice — but the binary's public contract is small enough that
//! the stdlib gives us every assertion we need without a new
//! dev-dep. Keeping the dependency graph boring pays off when a
//! security-scanner run flags the transitive tree.
//!
//! # Convention
//!
//! Each test spawns the binary with a pinned `ZERO_API_URL` env
//! var (either the real default or a local `MockEngine`). Tests
//! that touch the engine spin up a mock; tests that only check
//! argument parsing / help / refusal paths point at an
//! intentionally-unreachable URL to avoid cross-test flakiness.
//!
//! Every assertion names the exact exit code (not the abstract
//! `ExitKind`) because the contract with shell-scripts is the
//! integer, not the Rust enum.

use std::process::{Command, Stdio};

use zero_testkit::mock_engine::MockEngine;

/// The pinned unreachable URL used for "parsing-only" tests.
/// Not localhost:0 because some platforms bind to a random port
/// there; not 127.0.0.1:1 because some CI runners dislike port
/// 1 in particular. `198.51.100.1` is TEST-NET-2 (RFC 5737) so
/// the packet is guaranteed to black-hole quickly.
const UNREACHABLE: &str = "http://198.51.100.1:1";

/// Path to the compiled binary. Cargo sets this env var at
/// compile time of the integration test, so we never have to
/// guess at `target/` layout or profile.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_zero")
}

/// Build a `Command` with `--api` pointing at the given URL.
/// We pass `--api` rather than `ZERO_API_URL` so a stray env
/// var on the host machine cannot influence the test.
fn cmd(api: &str) -> Command {
    let mut c = Command::new(bin());
    c.arg("--api").arg(api);
    // Defense-in-depth: clear `ZERO_API_TOKEN` so a host
    // keychain or env var cannot affect refusal paths.
    c.env_remove("ZERO_API_TOKEN");
    // And: never let the test process pop a macOS keychain
    // prompt. The CLI's TTY-based short-circuit covers the
    // non-interactive case, but a developer running
    // `cargo test` from a terminal has stdin attached, which
    // would otherwise hit the keyring. `ZERO_NO_KEYCHAIN=1`
    // forces the honest "no token" fallthrough.
    c.env("ZERO_NO_KEYCHAIN", "1");
    c
}

// Every test that spawns the binary against a `MockEngine` runs
// under `tokio::test(flavor = "multi_thread")`. The default
// current-thread runtime would block the mock's accept loop for
// the duration of `Command::output()`, so the child process
// would hit the 8 s HTTP retry ceiling before the mock ever
// served a byte. Multi-thread runs the mock on a separate
// worker while the test thread blocks on the child — no extra
// latency, no hung listener.

// ── exit code taxonomy ──────────────────────────────────────────

#[test]
fn bare_zero_with_non_tty_prints_help_and_exits_zero() {
    // A bare invocation whose stdout is a pipe (non-TTY) must
    // print help and exit 0 — never stream raw TUI bytes into
    // a script. `Stdio::piped()` is the mechanism that makes
    // stdout non-tty from the binary's perspective.
    let out = cmd(UNREACHABLE)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(0),
        "bare zero w/ non-tty stdout must exit 0, got {:?}. stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage: zero"),
        "help must include Usage line, got:\n{stdout}"
    );
    assert!(
        stdout.contains("run"),
        "help must list the `run` subcommand, got:\n{stdout}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn version_against_mock_is_ok_and_reports_engine_version() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let url = mock.base_url();
    let out = cmd(&url).arg("version").output().expect("spawn");
    mock.shutdown().await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "version against mock must exit 0, got {:?}\nurl={url}\nstdout={}\nstderr={}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("zero "));
    assert!(stdout.contains("engine "));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn version_json_emits_parseable_object_with_every_field() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let out = cmd(&mock.base_url())
        .arg("--json")
        .arg("version")
        .output()
        .expect("spawn");
    mock.shutdown().await;
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("must be parseable JSON");
    // Every documented field must be present. Missing any is an
    // honesty regression — scripts reading this object rely on
    // the shape.
    for key in [
        "cli_version",
        "engine_version",
        "engine_status",
        "engine_url",
        "engine_reachable",
    ] {
        assert!(v.get(key).is_some(), "field {key} missing in {stdout}");
    }
    assert_eq!(v["engine_reachable"], serde_json::Value::Bool(true));
}

#[test]
fn version_against_unreachable_exits_engine_unreachable() {
    // DNS + TCP to an unroutable address should surface as
    // exit 2 (`EngineUnreachable`), not 1 (`Usage`). The
    // connect attempt can take a few seconds on some
    // platforms before the syscall gives up.
    let out = cmd(UNREACHABLE).arg("version").output().expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(2),
        "unreachable engine must exit 2, got {:?}. stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("engine unreachable"));
}

#[test]
fn version_with_malformed_url_exits_usage() {
    // An obviously-broken URL is a usage failure — the
    // operator typed something wrong. `build_client` rejects
    // it before any network I/O.
    let out = cmd("not a url at all")
        .arg("version")
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(1),
        "malformed URL must exit 1 (Usage), got {:?}. stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── run subcommand ──────────────────────────────────────────────

#[test]
fn run_without_args_exits_usage_via_clap() {
    // Clap enforces `required = true` on the trailing var-arg.
    // A bare `zero run` must exit non-zero from clap itself
    // (conventionally 2 in clap 4; we accept anything non-zero
    // since it is clap's contract, not ours, and a future clap
    // bump could reshape it — what matters is we do not
    // silently succeed).
    let out = cmd(UNREACHABLE).arg("run").output().expect("spawn");
    assert!(
        !out.status.success(),
        "zero run with no args must fail, got {:?}",
        out.status
    );
}

#[test]
fn run_increases_risk_command_is_refused_as_usage() {
    // Four increasing-risk commands today: /execute,
    // /state-override, /disclosure-override, and each of those
    // under their parse aliases. The refusal is the single
    // most important safety property of the non-interactive
    // path — silent acceptance here would let a scripted
    // pipeline bypass the friction ladder wholesale. Test
    // three of the four shapes so a regression in any parse
    // arm surfaces loudly.
    for invocation in [
        vec!["run", "execute"],
        vec!["run", "state-override", "STEADY"],
        vec!["run", "disclosure-override", "--i-know-what-i-am-doing"],
    ] {
        let out = cmd(UNREACHABLE).args(&invocation).output().expect("spawn");
        assert_eq!(
            out.status.code(),
            Some(1),
            "Increases command {invocation:?} must exit 1 (Usage), got {:?}. stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("refusing"),
            "stderr must explain refusal for {invocation:?}, got:\n{stderr}"
        );
        // And must tell the operator the escape hatch.
        assert!(
            stderr.contains("interactive"),
            "stderr must point to interactive path for {invocation:?}, got:\n{stderr}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_status_against_mock_succeeds_and_renders_text() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let out = cmd(&mock.base_url())
        .args(["run", "status"])
        .output()
        .expect("spawn");
    mock.shutdown().await;
    assert_eq!(
        out.status.code(),
        Some(0),
        "status must exit 0, got {:?}. stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // MockEngine returns a populated /status row — check for
    // the engine-summary tokens the dispatcher emits.
    assert!(
        stdout.contains("regime=") || stdout.contains("equity="),
        "status line missing engine summary tokens, got:\n{stdout}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_status_json_emits_array_of_output_lines() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let out = cmd(&mock.base_url())
        .args(["--json", "run", "status"])
        .output()
        .expect("spawn");
    mock.shutdown().await;
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("must be parseable JSON");
    let arr = v.as_array().expect("array shape");
    assert!(!arr.is_empty(), "at least one output line expected");
    // Every element must have {kind, text}.
    for item in arr {
        assert!(item.get("kind").is_some(), "element missing kind: {item}");
        assert!(item.get("text").is_some(), "element missing text: {item}");
    }
}

#[test]
fn run_unknown_command_exits_ok_with_warn() {
    // Unknown commands resolve to `Command::Unknown` in the
    // dispatcher and emit an `OutputLine::Warn` ("unknown
    // command: /xyz (try /help)"). A Warn alone is *not* an
    // error — it is the dispatcher saying "I did not do
    // anything, but the operator made a typo." Exit 0 is the
    // honest answer: no engine was touched. If an operator
    // script wants to be strict they can grep for `^!`.
    let out = cmd(UNREACHABLE)
        .args(["run", "xyzzy-not-real"])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(0),
        "unknown cmd must exit 0, got {:?}. stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("unknown command"));
}

#[test]
fn run_slash_prefix_is_optional() {
    // `zero run status` and `zero run /status` must behave
    // identically. The dispatcher is the single source of
    // truth and already accepts both forms; this test pins
    // the binary-level contract so a future argv-rewriting
    // change cannot desync them.
    let out_a = cmd(UNREACHABLE)
        .args(["run", "help"])
        .output()
        .expect("spawn");
    let out_b = cmd(UNREACHABLE)
        .args(["run", "/help"])
        .output()
        .expect("spawn");
    assert_eq!(out_a.status.code(), out_b.status.code());
    assert_eq!(out_a.stdout, out_b.stdout);
}

// ── --paper advisory ────────────────────────────────────────────

#[test]
fn paper_flag_emits_honest_advisory() {
    // `--paper` is parsed but has no effect in M1. The binary
    // must emit a honest advisory on stderr so an operator
    // passing it does not assume they toggled modes. Silent
    // acceptance would be the opposite of what this CLI
    // promises in §15's refusal list.
    let out = cmd(UNREACHABLE)
        .args(["--paper", "version"])
        .output()
        .expect("spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--paper"),
        "stderr must mention --paper, got:\n{stderr}"
    );
}
