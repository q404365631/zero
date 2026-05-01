//! End-to-end tests for `HttpClient` against the `zero-testkit`
//! mock. The real engine's FastAPI shapes are frozen in the mock;
//! drift here means either the mock is stale or the client got it
//! wrong. Either way, CI fails loud.

use zero_engine_client::HttpClient;
use zero_testkit::mock_engine::MockEngine;

#[tokio::test]
async fn root_decodes_version_probe() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let root = client.root().await.expect("root");
    assert_eq!(root.name, "ZERO OS");
    assert_eq!(root.version, "1.2.3-mock");
    assert_eq!(root.status, "running");
    mock.shutdown().await;
}

#[tokio::test]
async fn health_decodes_components() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let health = client.health().await.expect("health");
    assert!(health.is_ok());
    let counts = health.component_counts();
    assert_eq!(counts.healthy, 2);
    assert_eq!(counts.dead, 0);
    mock.shutdown().await;
}

#[tokio::test]
async fn health_degraded_surfaces() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.degrade_health = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let health = client.health().await.expect("health");
    assert!(!health.is_ok());
    mock.shutdown().await;
}

#[tokio::test]
async fn live_preflight_decodes_readiness_gate() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let preflight = client.live_preflight().await.expect("live preflight");

    assert_eq!(preflight.schema_version, "zero.live_preflight.v1");
    assert_eq!(preflight.exchange, "hyperliquid");
    assert!(!preflight.ready);
    assert!(preflight.controls_ready);
    assert!(
        preflight
            .checks
            .iter()
            .any(|check| check.name == "live_executor" && check.status == "fail")
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn unreachable_host_is_typed() {
    // port 1 is virtually guaranteed to reject.
    let client = HttpClient::new("http://127.0.0.1:1", None).expect("client");
    let err = client.root().await.expect_err("should fail");
    assert!(
        matches!(
            err,
            zero_engine_client::HttpError::Unreachable(_)
                | zero_engine_client::HttpError::Timeout(_)
        ),
        "unexpected error variant: {err:?}",
    );
}

// ── Retry policy + error-mapping matrix ────────────────────────────
//
// These tests live here rather than in `http_breadth.rs` because
// they exercise the transport layer, not payload decoding. Together
// they lock down the contract documented at the top of
// `http.rs`:
//
//   * retry-once on 502 / 503 / 504 / timeout / transport error
//     (500 ms fixed backoff)
//   * 401 / 403  → `HttpError::Unauthorized`
//   * 404       → `HttpError::NotFound`
//   * other 4xx / 5xx → `HttpError::Status { .. }`, no retry
//
// The mock's `inject_failures` middleware is the failure surface.
// Each test flips one override, issues one typed call, and asserts
// the typed variant the client produces.

#[tokio::test]
async fn retry_recovers_after_one_transient_failure() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    // One 503 then success — the client's retry-once policy must
    // swallow the first failure and return the decoded body.
    mock.with_overrides(|o| o.transient_fail_count = 1);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let status = client.v2_status().await.expect("retry should recover");
    assert_eq!(status.regime(), Some("TREND_LONG confirmed across majors."));
    mock.shutdown().await;
}

#[tokio::test]
async fn retry_gives_up_after_second_transient_failure() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    // Two 503s — first attempt + single retry, both fail.
    // The client surfaces the final status as `HttpError::Status`
    // (retry budget was exhausted).
    mock.with_overrides(|o| o.transient_fail_count = 2);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let err = client.v2_status().await.expect_err("both attempts fail");
    match err {
        zero_engine_client::HttpError::Status { status, .. } => {
            assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
        }
        other => panic!("expected Status(503), got {other:?}"),
    }
    mock.shutdown().await;
}

#[tokio::test]
async fn unauthorized_status_maps_to_typed_error() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.force_unauthorized = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let err = client.positions().await.expect_err("401");
    assert!(
        matches!(err, zero_engine_client::HttpError::Unauthorized),
        "expected Unauthorized, got {err:?}",
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn not_found_maps_to_typed_error_with_path() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.force_not_found = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let err = client.risk().await.expect_err("404");
    match err {
        zero_engine_client::HttpError::NotFound { path } => {
            assert_eq!(path, "/risk", "path must be preserved for diagnostics");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
    mock.shutdown().await;
}

#[tokio::test]
async fn non_retryable_500_fails_without_second_attempt() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    // 500 is not in the retry set (only 502/503/504 are). The
    // client must surface it immediately as `HttpError::Status`.
    mock.with_overrides(|o| o.force_server_error = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let err = client.regime(None).await.expect_err("500");
    match err {
        zero_engine_client::HttpError::Status { status, .. } => {
            assert_eq!(status, reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        }
        other => panic!("expected Status(500), got {other:?}"),
    }
    mock.shutdown().await;
}
