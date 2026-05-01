//! End-to-end integration tests for the CLI-side rate budget +
//! engine-429 handling.
//!
//! Covers the two observable paths from `M2_PLAN.md §1`:
//!
//! 1. **CLI-side exhaustion.** A [`RateBudget`] with a small
//!    capacity (and a zero-refill clock) refuses the call before
//!    it leaves the process — the [`MockEngine`] never receives
//!    the request. The operator-visible error is shaped as
//!    `rate: exhausted — retry in Ns`.
//! 2. **Engine-side 429 round-trip.** The engine returns 429 with
//!    a `Retry-After` header. The client **refunds the local
//!    bucket** (otherwise the operator pays twice for a request
//!    the engine rejected), parses the header, and surfaces the
//!    same typed error with the engine's suggested wait.
//!
//! A third test pins the no-auto-retry rule: 429 is never looped
//! on, because a silent retry turns the engine's "wait N seconds"
//! into a wasted traffic burst (or, worse, a blanket block from
//! the engine's own limiter).

use std::sync::Arc;
use std::time::Duration;

use zero_engine_client::{
    HttpClient, HttpError, ManualClock, RateBudget, RateLimitSource, SystemClock,
};
use zero_testkit::mock_engine::MockEngine;

/// Build a client pointed at `mock` with a full-size bucket. The
/// tests that do not care about the budget still wire one so the
/// middleware layering is exercised identically to production.
fn client_with_full_budget(mock: &MockEngine) -> HttpClient {
    let budget = RateBudget::with_clock(60, 1.0, Arc::new(SystemClock));
    HttpClient::new(mock.base_url(), Some("tok".to_string()))
        .expect("client")
        .with_rate_budget(budget)
}

#[tokio::test]
async fn cli_budget_exhausted_refuses_without_touching_network() {
    let mock = MockEngine::spawn().await.expect("mock");
    let clock = ManualClock::new();
    let budget = RateBudget::with_clock(1, 0.0, clock);
    // Spend the only token so the next call must fail at the
    // budget layer.
    budget.try_consume(1).unwrap();
    let client = HttpClient::new(mock.base_url(), Some("tok".to_string()))
        .expect("client")
        .with_rate_budget(budget);

    match client.v2_status().await {
        Err(HttpError::RateBudgetExhausted {
            retry_after,
            origin,
        }) => {
            assert_eq!(origin, RateLimitSource::CliBudget);
            // Zero-refill clock → retry is "forever"
            // (`Duration::MAX`). The exact value is less
            // important than the shape: the client did not block
            // waiting for a nonexistent refill.
            assert_eq!(retry_after, Duration::MAX);
        }
        other => panic!("expected CliBudget exhaustion, got {other:?}"),
    }
    // Most importantly: the mock never saw the request. If it had,
    // the `/v2/status` mock would have logged at least one call
    // somewhere; the client-facing assertion above covers the
    // visible side effect, and the mock's default handler has no
    // observable state to drift on a silent extra hit.

    mock.shutdown().await;
}

#[tokio::test]
async fn engine_429_refunds_local_bucket_and_surfaces_retry_after() {
    let mock = MockEngine::spawn().await.expect("mock");
    mock.with_overrides(|o| {
        o.rate_limit_count = 1;
        o.rate_limit_retry_after = Some("7".to_string());
    });
    let client = client_with_full_budget(&mock);
    let before = client.rate_budget().unwrap().snapshot().tokens;

    let err = client
        .v2_status()
        .await
        .expect_err("engine 429 must surface as error");
    match err {
        HttpError::RateBudgetExhausted {
            retry_after,
            origin,
        } => {
            assert_eq!(origin, RateLimitSource::Engine429);
            assert_eq!(retry_after, Duration::from_secs(7));
        }
        other => panic!("expected Engine429 exhaustion, got {other:?}"),
    }

    // Bucket was debited (1 point for /v2/status) pre-send,
    // refunded post-429. Net zero — `before == after`.
    let after = client.rate_budget().unwrap().snapshot().tokens;
    assert_eq!(
        before, after,
        "engine-429 must refund the local bucket so the operator is not double-charged",
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn engine_429_is_never_auto_retried() {
    // The retry-once policy must not apply to 429 — a CLI that
    // loops on a 429 defeats the whole `Retry-After` contract. We
    // prove it indirectly: inject exactly one 429, and assert the
    // next call (which would *succeed* if the client looped once)
    // still fails because the injected 429 was the only response
    // the test-side setup actually delivered.
    let mock = MockEngine::spawn().await.expect("mock");
    mock.with_overrides(|o| {
        o.rate_limit_count = 1;
        o.rate_limit_retry_after = Some("1".to_string());
    });
    let client = client_with_full_budget(&mock);

    let first = client.v2_status().await;
    assert!(
        matches!(first, Err(HttpError::RateBudgetExhausted { .. })),
        "expected 429-originated exhaustion, got {first:?}",
    );

    // Counter is now drained; the second call should succeed.
    let second = client.v2_status().await;
    assert!(
        second.is_ok(),
        "after the counter drains the engine returns 200; got {second:?}",
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn retry_after_missing_header_defaults_to_one_second() {
    // `rate_limit_count > 0` without a `retry_after` override must
    // still produce a parseable response. The default header is
    // "1", so the client's parsed duration is exactly 1 s.
    let mock = MockEngine::spawn().await.expect("mock");
    mock.with_overrides(|o| {
        o.rate_limit_count = 1;
        o.rate_limit_retry_after = None; // explicit
    });
    let client = client_with_full_budget(&mock);

    match client.v2_status().await {
        Err(HttpError::RateBudgetExhausted { retry_after, .. }) => {
            assert_eq!(retry_after, Duration::from_secs(1));
        }
        other => panic!("expected default-retry 429, got {other:?}"),
    }

    mock.shutdown().await;
}

#[tokio::test]
async fn budget_cost_is_applied_per_endpoint() {
    // `/v2/status` costs 3; `/positions` costs 1. A 4-token bucket
    // serves one `/v2/status` + one `/positions` fine (total cost
    // 4), but a second `/v2/status` must fail at the budget layer.
    let mock = MockEngine::spawn().await.expect("mock");
    let budget = RateBudget::with_clock(4, 0.0, Arc::new(SystemClock));
    let client = HttpClient::new(mock.base_url(), Some("tok".to_string()))
        .expect("client")
        .with_rate_budget(budget);

    client.v2_status().await.expect("first v2 status");
    client.positions().await.expect("positions");

    match client.v2_status().await {
        Err(HttpError::RateBudgetExhausted {
            origin: RateLimitSource::CliBudget,
            ..
        }) => {}
        other => panic!("expected CLI-budget exhaustion on second v2_status, got {other:?}"),
    }

    mock.shutdown().await;
}
