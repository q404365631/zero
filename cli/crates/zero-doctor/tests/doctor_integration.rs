//! End-to-end `Doctor::run` behavior against `zero-testkit`'s
//! mock engine. These tests are the contract enforcement for
//! spec §18:
//!
//! * Every configured check shows up by name in the report.
//! * A healthy engine yields `worst = Ok` and exit code 0.
//! * `--fix` converts a missing config dir to `Repaired`.
//! * Token rejection (`HttpError::Unauthorized`) becomes a Fail
//!   on the `auth_verified` row — not a Warn.
//! * The runner finishes in under 2 s against a local mock,
//!   even with the WS probe and the authed round-trip wired in.
//!
//! We use unique temp dirs per test so `--fix` tests don't race
//! with the "directory-exists" happy path.

use std::path::PathBuf;

use zero_doctor::{CheckStatus, Doctor};
use zero_engine_client::HttpClient;
use zero_testkit::mock_engine::MockEngine;

fn unique_tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "zero-doctor-{tag}-{pid}-{nanos}",
        pid = std::process::id(),
        nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&d).expect("create tmp dir");
    d
}

fn write_config(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(&path, body).expect("write config.toml");
    path
}

#[tokio::test]
async fn healthy_engine_passes_all_rows() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), Some("tkn".into())).expect("client");
    let dir = unique_tmp("healthy");
    write_config(&dir, "# valid toml\n");

    let doctor = Doctor::builder()
        .client(Some(client))
        .config_dir(dir)
        .ws_url(mock.ws_url())
        .build();
    let report = doctor.run().await;

    assert_eq!(report.worst, CheckStatus::Ok, "{}", report.render_text());
    assert_eq!(report.exit_code(), 0);
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    for expected in [
        "runtime",
        "config_dir",
        "config_parse",
        "engine_reachable",
        "engine_healthy",
        "engine_components",
        "auth",
        "auth_verified",
        "ws_reachable",
    ] {
        assert!(names.contains(&expected), "missing row: {expected}");
    }

    mock.shutdown().await;
}

#[tokio::test]
async fn no_token_warns_but_does_not_fail() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let dir = unique_tmp("no-tok");

    let doctor = Doctor::builder()
        .client(Some(client))
        .config_dir(dir)
        .build();
    let report = doctor.run().await;

    let auth = report
        .checks
        .iter()
        .find(|c| c.name == "auth")
        .expect("auth check");
    assert_eq!(auth.status, CheckStatus::Warn);
    // `auth_verified` is skipped when there is no token — rather
    // than reporting "skipped" as Ok, we just omit the row.
    assert!(
        report.checks.iter().all(|c| c.name != "auth_verified"),
        "auth_verified should be absent when no token is set"
    );
    assert!(matches!(report.worst, CheckStatus::Ok | CheckStatus::Warn));

    mock.shutdown().await;
}

#[tokio::test]
async fn rejected_token_fails_auth_verified() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    // The mock's `/` and `/health` routes are outside the failure
    // injection layer, so only the typed authed endpoints get the
    // 401. That's the real-world shape: an engine that serves
    // `/health` openly but rejects a bad token on `/risk`.
    mock.with_overrides(|o| o.force_unauthorized = true);
    let client = HttpClient::new(mock.base_url(), Some("bogus".into())).expect("client");
    let dir = unique_tmp("bad-tok");

    let doctor = Doctor::builder()
        .client(Some(client))
        .config_dir(dir)
        .build();
    let report = doctor.run().await;

    let verified = report
        .checks
        .iter()
        .find(|c| c.name == "auth_verified")
        .expect("auth_verified row");
    assert_eq!(
        verified.status,
        CheckStatus::Fail,
        "{}",
        report.render_text()
    );
    assert_eq!(report.exit_code(), 2);

    mock.shutdown().await;
}

#[tokio::test]
async fn degraded_engine_reports_warn() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.degrade_health = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let dir = unique_tmp("degraded");

    let doctor = Doctor::builder()
        .client(Some(client))
        .config_dir(dir)
        .build();
    let report = doctor.run().await;

    let healthy = report
        .checks
        .iter()
        .find(|c| c.name == "engine_healthy")
        .expect("healthy check");
    assert_eq!(healthy.status, CheckStatus::Warn);
    assert_eq!(report.exit_code(), 0, "warn is still exit 0 per spec");

    mock.shutdown().await;
}

#[tokio::test]
async fn unreachable_engine_fails() {
    let client = HttpClient::new("http://127.0.0.1:1", None).expect("client");
    let dir = unique_tmp("unreach");
    let doctor = Doctor::builder()
        .client(Some(client))
        .config_dir(dir)
        .build();
    let report = doctor.run().await;

    assert_eq!(report.worst, CheckStatus::Fail, "{}", report.render_text());
    assert_ne!(report.exit_code(), 0);
}

#[tokio::test]
async fn no_client_fails_cleanly() {
    let dir = unique_tmp("no-client");
    let doctor = Doctor::builder().client(None).config_dir(dir).build();
    let report = doctor.run().await;
    let reachable = report
        .checks
        .iter()
        .find(|c| c.name == "engine_reachable")
        .expect("reachable check");
    assert_eq!(reachable.status, CheckStatus::Fail);
}

#[tokio::test]
async fn config_parse_error_is_fail() {
    let dir = unique_tmp("bad-cfg");
    // Unterminated string literal → TOML parse failure.
    write_config(&dir, "engine = \"unterminated\n");

    let doctor = Doctor::builder().client(None).config_dir(dir).build();
    let report = doctor.run().await;
    let parse = report
        .checks
        .iter()
        .find(|c| c.name == "config_parse")
        .expect("config_parse row");
    assert_eq!(parse.status, CheckStatus::Fail, "{}", parse.note);
}

#[tokio::test]
async fn missing_config_dir_warns_without_fix() {
    // Start from a unique dir then remove it so the check sees a
    // truly missing directory. `create_dir_all` is not reversed
    // by `remove_dir_all` if something in between already wrote —
    // assert liberally.
    let dir = unique_tmp("miss");
    std::fs::remove_dir_all(&dir).ok();
    assert!(!dir.exists(), "dir should be missing pre-check");

    let doctor = Doctor::builder()
        .client(None)
        .config_dir(dir.clone())
        .build();
    let report = doctor.run().await;

    let cfg = report
        .checks
        .iter()
        .find(|c| c.name == "config_dir")
        .expect("config_dir row");
    assert_eq!(cfg.status, CheckStatus::Warn);
    assert!(!dir.exists(), "without --fix the dir must stay missing");
}

#[tokio::test]
async fn fix_creates_missing_config_dir() {
    let dir = unique_tmp("fix");
    std::fs::remove_dir_all(&dir).ok();
    assert!(!dir.exists(), "dir should be missing pre-check");

    let doctor = Doctor::builder()
        .client(None)
        .config_dir(dir.clone())
        .fix(true)
        .build();
    let report = doctor.run().await;

    let cfg = report
        .checks
        .iter()
        .find(|c| c.name == "config_dir")
        .expect("config_dir row");
    assert_eq!(cfg.status, CheckStatus::Repaired, "{}", cfg.note);
    assert!(dir.exists(), "--fix must leave the directory present");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn ws_unreachable_fails() {
    // A TCP port with no listener — port 1 is reserved, so the
    // handshake dials into the void and returns a connect error
    // long before the 1.5 s probe timeout.
    let dir = unique_tmp("ws-down");
    let doctor = Doctor::builder()
        .client(None)
        .config_dir(dir)
        .ws_url("ws://127.0.0.1:1/ws")
        .build();
    let report = doctor.run().await;

    let ws = report
        .checks
        .iter()
        .find(|c| c.name == "ws_reachable")
        .expect("ws_reachable row");
    assert_eq!(ws.status, CheckStatus::Fail, "{}", ws.note);
}

#[tokio::test]
async fn finishes_under_two_seconds_against_healthy_mock() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), Some("tkn".into())).expect("client");
    let dir = unique_tmp("budget");
    write_config(&dir, "# ok\n");

    let doctor = Doctor::builder()
        .client(Some(client))
        .config_dir(dir)
        .ws_url(mock.ws_url())
        .build();

    let started = std::time::Instant::now();
    let _ = doctor.run().await;
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_millis() < 2000,
        "doctor took {}ms (budget is 2000ms)",
        elapsed.as_millis()
    );

    mock.shutdown().await;
}
