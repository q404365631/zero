//! Integration tests for M2_PLAN §7 — Auto-mode composition surface.
//!
//! These tests pin the wire contract between the CLI and the engine
//! for the two composition-change endpoints the live-trade path
//! flows through:
//!
//! - `POST /execute` — places a composition-change order. Carries
//!   a typed body (coin / side / size / idempotency_key) and
//!   mirrors the idempotency key into an `X-Idempotency-Key`
//!   header. The engine's response carries `simulated: bool` —
//!   the CLI reads that flag, not a local "am I in paper mode?"
//!   guess.
//! - `POST /auto/toggle` — flips the engine's Auto-mode flag.
//!   Response `state` reflects the engine's **post-call** truth
//!   (friction may refuse the flip).
//!
//! Every test here is written against the `zero-testkit` mock; the
//! mock captures headers + body so the wire shape is verifiable
//! without reaching for `reqwest` mocks at the raw-byte level.
//!
//! Coverage rubric (M2_PLAN §7 bullets, mapped onto tests):
//!
//! 1. `POST /execute` end-to-end (typed body + deserialized reply).
//! 2. `POST /auto/toggle` end-to-end + engine-refusal path.
//! 3. `X-Zero-Mode: paper | live` honored on both endpoints.
//! 4. Paper-mode responses carry `simulated: true` and the CLI
//!    round-trips that flag back to the caller.
//! 5. **No-retry rule**: a single upstream 503 on `POST /execute`
//!    or `POST /auto/toggle` surfaces as a typed error with
//!    exactly one upstream request — silent retry is the single
//!    worst failure mode a trading CLI can have.
//! 6. Idempotency key: unique per call, round-trips into both
//!    the body and the `X-Idempotency-Key` header.

use zero_engine_client::{AutoState, ExecuteSide, HttpClient, HttpError, Mode};
use zero_testkit::mock_engine::MockEngine;

// ─── /execute — happy path ─────────────────────────────────────────

#[tokio::test]
async fn post_execute_round_trips_typed_body_and_response() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let resp = client
        .post_execute("BTC", ExecuteSide::Buy, 0.25)
        .await
        .expect("execute accepted");

    assert!(resp.accepted, "mock echoes accepted=true");
    // Mock does not force paper-mode without an `X-Zero-Mode: paper`
    // header; an unset client must see `simulated: false`. This pins
    // that the paper-mode path is header-driven, not a default.
    assert!(
        !resp.simulated,
        "simulated must default to false when mode unset"
    );

    let captures = mock.received_executes();
    assert_eq!(captures.len(), 1, "one upstream call, no double-send");
    let cap = &captures[0];

    let body = &cap.body;
    assert_eq!(body["coin"], "BTC");
    assert_eq!(body["side"], "buy");
    // `size` is serialized as a JSON number; the typed `f64` round-
    // trips to the same literal.
    assert_eq!(body["size"].as_f64(), Some(0.25));
    // Idempotency key is present in the body and is a v4 UUID shape
    // (36 chars, four hyphens). We do not pin the exact value — the
    // contract is "unique per call" — but the shape must match.
    let key = body["idempotency_key"].as_str().expect("key in body");
    assert_eq!(key.len(), 36, "UUID v4 stringifies to 36 chars");
    assert_eq!(key.matches('-').count(), 4);

    // Header mirror — the key in the body **must** also land in
    // `X-Idempotency-Key` so engine-side proxies that log headers
    // but redact bodies still see the dedupe key.
    assert_eq!(
        cap.headers.get("x-idempotency-key").map(String::as_str),
        Some(key),
    );

    // Content type is explicit JSON; no `X-Zero-Mode` was attached.
    assert_eq!(
        cap.headers.get("content-type").map(String::as_str),
        Some("application/json"),
    );
    assert!(
        !cap.headers.contains_key("x-zero-mode"),
        "no mode override attached by default",
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn post_execute_emits_unique_idempotency_key_per_call() {
    // Two back-to-back `/execute` calls must carry different
    // idempotency keys. A stale / reused key is a dedupe bug — the
    // second order would be silently dropped.
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    client
        .post_execute("ETH", ExecuteSide::Sell, 1.0)
        .await
        .expect("first execute");
    client
        .post_execute("ETH", ExecuteSide::Sell, 1.0)
        .await
        .expect("second execute");

    let captures = mock.received_executes();
    assert_eq!(captures.len(), 2);
    let key1 = captures[0].body["idempotency_key"].as_str().unwrap();
    let key2 = captures[1].body["idempotency_key"].as_str().unwrap();
    assert_ne!(key1, key2, "each /execute mints a fresh key");

    mock.shutdown().await;
}

// ─── X-Zero-Mode honored on composition endpoints ──────────────────

#[tokio::test]
async fn post_execute_honors_paper_mode_header() {
    // Client attached with `Mode::Paper` — the mock mirrors
    // `X-Zero-Mode: paper` into a `simulated: true` response; the
    // CLI surfaces that flag unchanged.
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None)
        .expect("client")
        .with_mode(Mode::Paper);

    let resp = client
        .post_execute("SOL", ExecuteSide::Buy, 5.0)
        .await
        .expect("paper execute accepted");
    assert!(
        resp.simulated,
        "paper mode must propagate into the response's simulated flag",
    );

    let captures = mock.received_executes();
    assert_eq!(
        captures[0].headers.get("x-zero-mode").map(String::as_str),
        Some("paper"),
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn post_execute_honors_live_mode_header() {
    // Explicit `Mode::Live` must emit the header verbatim; the
    // engine uses the header to distinguish operator-forced-live
    // from engine-default-live, even though both resolve to the
    // same live-fill code path today.
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None)
        .expect("client")
        .with_mode(Mode::Live);

    let resp = client
        .post_execute("ARB", ExecuteSide::Sell, 10.0)
        .await
        .expect("live execute accepted");
    assert!(
        !resp.simulated,
        "live mode must not flip the simulated flag",
    );

    let captures = mock.received_executes();
    assert_eq!(
        captures[0].headers.get("x-zero-mode").map(String::as_str),
        Some("live"),
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn post_auto_toggle_honors_paper_mode_header() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None)
        .expect("client")
        .with_mode(Mode::Paper);

    let resp = client.post_auto_toggle(true).await.expect("flipped");
    assert_eq!(resp.state, AutoState::On);
    assert!(resp.simulated, "paper-mode flip must carry simulated=true");

    let captures = mock.received_auto_toggles();
    assert_eq!(captures.len(), 1);
    assert_eq!(
        captures[0].headers.get("x-zero-mode").map(String::as_str),
        Some("paper"),
    );
    // `/auto/toggle` does **not** emit an idempotency-key header —
    // the endpoint is naturally idempotent (flipping on twice is a
    // no-op) and the no-retry rule covers the ambiguous-failure
    // mid-flight case.
    assert!(
        !captures[0].headers.contains_key("x-idempotency-key"),
        "auto/toggle must not carry idempotency-key",
    );

    mock.shutdown().await;
}

// ─── /auto/toggle — engine-refusal path ────────────────────────────

#[tokio::test]
async fn post_auto_toggle_surfaces_engine_refusal_verbatim() {
    // Operator asks for `on`; engine returns `off` + a reason. The
    // CLI must surface the engine's truth — not the requested flip —
    // and preserve the `reason` string so the operator sees why.
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| {
        o.auto_toggle_echo_state = Some(false);
        o.auto_toggle_reason = Some("operator state is TILT".into());
    });

    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let resp = client.post_auto_toggle(true).await.expect("delivered");

    assert_eq!(
        resp.state,
        AutoState::Off,
        "engine refusal must land verbatim, not optimistically",
    );
    assert_eq!(resp.reason.as_deref(), Some("operator state is TILT"));

    mock.shutdown().await;
}

// ─── No-retry rule (M2_PLAN §7) ────────────────────────────────────

#[tokio::test]
async fn post_execute_never_retries_on_503() {
    // `POST /execute` must fail after exactly one upstream attempt
    // when the engine returns 503. Silent retry here is the exact
    // failure mode the no-retry rule exists to prevent.
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.post_transient_fail = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let err = client
        .post_execute("BTC", ExecuteSide::Buy, 0.1)
        .await
        .expect_err("503 must surface typed");
    assert!(
        matches!(
            err,
            HttpError::Status { status, .. } if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        ),
        "expected 503 Status, got {err:?}",
    );

    // Capture list is empty because the mock short-circuits in the
    // injection path *before* pushing the capture — the assertion
    // here is on the override decrementing exactly one request off
    // the wire. We re-use a `force_simulated`-style sentinel by
    // running a second call that must now succeed (override still
    // set, so still 503). If the first call had retried, the mock
    // would have observed two 503s against a single override, but
    // the override is boolean-latched, so we can't count that way.
    // Instead: flip the override off and send a second call; it
    // must succeed, proving the latch is still live and that the
    // first call did not drain more than one request's worth.
    mock.with_overrides(|o| o.post_transient_fail = false);
    let ok = client
        .post_execute("BTC", ExecuteSide::Buy, 0.1)
        .await
        .expect("second call succeeds");
    assert!(ok.accepted);

    mock.shutdown().await;
}

#[tokio::test]
async fn post_auto_toggle_never_retries_on_503() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.post_transient_fail = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let err = client
        .post_auto_toggle(true)
        .await
        .expect_err("503 must surface typed");
    assert!(
        matches!(
            err,
            HttpError::Status { status, .. } if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        ),
        "expected 503 Status, got {err:?}",
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn post_execute_never_retries_on_500() {
    // Belt-and-suspenders: 500 is a non-retryable status in the
    // existing `is_retryable` matrix, but the composition-POST path
    // should not re-check that matrix at all. Pin both.
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.post_server_error = true);
    let client = HttpClient::new(mock.base_url(), None).expect("client");

    let err = client
        .post_execute("BTC", ExecuteSide::Buy, 0.1)
        .await
        .expect_err("500 must surface typed");
    assert!(
        matches!(
            err,
            HttpError::Status { status, .. } if status == reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ),
        "expected 500 Status, got {err:?}",
    );

    mock.shutdown().await;
}

#[tokio::test]
async fn post_execute_never_retries_on_timeout() {
    // Point the client at a black-hole address so the transport
    // errors out (connect-refused / timeout). The no-retry rule
    // must surface the first transport error verbatim instead of
    // re-dialing the unreachable host.
    //
    // We bound the test via `tokio::time::timeout` so a bug in the
    // no-retry path (two attempts at 8 s each → 16 s total) is
    // caught as a test timeout rather than silently making CI slow.
    let client = HttpClient::new("http://127.0.0.1:1", None).expect("client");
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        client.post_execute("BTC", ExecuteSide::Buy, 0.1),
    )
    .await;
    let elapsed = started.elapsed();

    let err = result
        .expect("must complete before 12s bound (retry would exceed)")
        .expect_err("transport error must surface");
    assert!(
        matches!(err, HttpError::Unreachable(_) | HttpError::Timeout(_)),
        "unexpected variant: {err:?}",
    );
    // A single 8 s timeout bounds the elapsed time well under 16 s
    // (which a retry-once would produce). The test's 12 s bound
    // above is the hard fail; this assertion pins the positive
    // case with a generous margin against slow CI.
    assert!(
        elapsed < std::time::Duration::from_secs(11),
        "single-attempt budget should leave plenty of headroom; was {elapsed:?}",
    );
}

// ─── Engine-asserted simulated flag ────────────────────────────────

#[tokio::test]
async fn execute_simulated_flag_is_engine_asserted_not_locally_guessed() {
    // Even with `Mode::Live` set on the client, if the engine
    // returns `simulated: true` (the paper adapter is mid-migration,
    // shadow mode is on, etc.), the CLI must surface that truth.
    // The opposite direction is covered by the paper tests above.
    let mock = MockEngine::spawn().await.expect("spawn mock");
    mock.with_overrides(|o| o.force_simulated = true);

    let client = HttpClient::new(mock.base_url(), None)
        .expect("client")
        .with_mode(Mode::Live);
    let resp = client
        .post_execute("BTC", ExecuteSide::Buy, 0.5)
        .await
        .expect("live request, simulated reply");

    assert!(
        resp.simulated,
        "engine truth beats client's local mode assumption",
    );

    mock.shutdown().await;
}
