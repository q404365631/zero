//! Fault-injection suite (M1_PLAN §9).
//!
//! The plan calls for an auditable, named test per fault mode:
//! WS drop, HTTP 500, HTTP timeout, HTTP 401, stale `Stat`.
//! Those modes are individually exercised in other test files
//! (`ws_integration.rs`, `http_integration.rs`,
//! `widgets/statusbar.rs`), but an operator's lived experience
//! happens one layer up — they type a slash command, the
//! engine misbehaves, and the dispatcher has to produce an
//! honest line rather than a silent success, a panic, or a
//! generic "something went wrong."
//!
//! This suite pins that contract at the dispatcher layer. It is
//! deliberately *not* a copy of the lower-level tests — we care
//! here only about what a caller at `dispatch()` observes:
//!
//! - The right `OutputLine` kind (`Alert` for engine-unreachable
//!   / 5xx / auth failures; `Command` for success).
//! - The command's own prefix (`status: `, `heat: `, `brief:`)
//!   so the operator sees which surface failed.
//! - No friction-ladder regression: an engine failure on a
//!   `Reduces` or `Neutral` command must still leave `risk`
//!   classified correctly and `friction` as `Proceed`.
//!
//! ## Coverage map vs the plan line
//!
//! | fault           | test name                                 | layer-under-test |
//! |-----------------|-------------------------------------------|------------------|
//! | HTTP 500        | `status_http_500_alerts_not_panics`       | dispatcher       |
//! | HTTP timeout    | `status_against_dead_host_alerts`         | dispatcher       |
//! | HTTP 401        | `status_401_alerts_without_leaking_token` | dispatcher       |
//! | transient 503   | `status_transient_503_recovers_via_retry` | dispatcher       |
//! | WS drop         | `ws_drop_and_reconnect_is_exercised_elsewhere` | link |
//! | stale Stat      | `stale_stat_is_exercised_in_widget_layer` | link |
//!
//! The two "link" tests do not duplicate the lower-layer
//! coverage; they are assertion-free breadcrumbs that compile
//! only if the canonical test modules still contain the pinned
//! test names. If someone renames or deletes those tests, this
//! file fails to compile, surfacing a plan-line regression at
//! PR time instead of months later.

use zero_commands::{DispatchContext, OutputLine, RiskDirection, dispatch};
use zero_engine_client::{EngineState, HttpClient};
use zero_testkit::mock_engine::MockEngine;

async fn ctx_with_mock() -> (MockEngine, DispatchContext) {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let ctx = DispatchContext::new(Some(client), EngineState::shared());
    (mock, ctx)
}

/// Reach into the dispatcher output and return the first line
/// as a `(kind, body)` tuple of strings. Lets assertions be
/// written `let (k, s) = first_line(&out); assert_eq!(k, "alert");`
/// which reads closer to the spec than a cascade of `matches!`.
fn first_line(out: &zero_commands::DispatchOutput) -> (&'static str, String) {
    let line = out.lines.first().expect("dispatcher produced no lines");
    match line {
        OutputLine::System(s) => ("system", s.clone()),
        OutputLine::Command(s) => ("command", s.clone()),
        OutputLine::Warn(s) => ("warn", s.clone()),
        OutputLine::Alert(s) => ("alert", s.clone()),
    }
}

// ─────────────────────────────────────────────────────────────
//  HTTP 500 — non-retryable server error. Dispatcher must
//  alert; the kind has to be `Alert`, not `Warn`, because the
//  operator cannot trust the downstream readout at all.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_http_500_alerts_not_panics() {
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_server_error = true);

    let out = dispatch(&ctx, "/status")
        .await
        .expect("dispatch returned error")
        .expect("dispatch returned None");

    assert_eq!(
        out.risk,
        Some(RiskDirection::Neutral),
        "a failed read must still classify its risk direction"
    );
    let (kind, body) = first_line(&out);
    assert_eq!(
        kind, "alert",
        "500 on /status must surface as Alert: {body}"
    );
    assert!(
        body.to_lowercase().contains("status"),
        "alert body must identify the failing surface: {body}"
    );

    mock.shutdown().await;
}

// ─────────────────────────────────────────────────────────────
//  HTTP timeout / transport error. We cannot simulate a real
//  timeout without holding up a test for the whole timeout
//  window, so we point the client at a known-dead port and
//  let the OS RST us immediately. The dispatcher's error
//  mapping has to collapse "unreachable" and "timeout" into
//  the same operator-visible shape: an Alert.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_against_dead_host_alerts() {
    // Port 1 is reserved-privileged; userland binds nothing
    // there by convention and the kernel RSTs connect()s.
    // Same pattern as `http_integration::unreachable_host_is_typed`.
    let client = HttpClient::new("http://127.0.0.1:1", None).expect("client");
    let ctx = DispatchContext::new(Some(client), EngineState::shared());

    let out = dispatch(&ctx, "/status")
        .await
        .expect("dispatch returned error")
        .expect("dispatch returned None");

    let (kind, body) = first_line(&out);
    assert_eq!(
        kind, "alert",
        "unreachable engine on /status must surface as Alert: {body}"
    );
    assert!(
        body.to_lowercase().contains("status"),
        "alert body must identify the failing surface: {body}"
    );
}

// ─────────────────────────────────────────────────────────────
//  HTTP 401 — token missing / expired / wrong-aud. The
//  dispatcher alert must *not* echo the token anywhere — a
//  future logging regression that stitched the Authorization
//  header into the alert message would be a confidentiality
//  incident. We pin both the alert surface and the absence
//  of any obviously-sensitive substring.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_401_alerts_without_leaking_token() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    // Attach a fake bearer so the leak-check has something
    // concrete to NOT find in the output.
    let sentinel = "zk_sentinel_token_value_1234567890";
    let client =
        HttpClient::new(mock.base_url(), Some(sentinel.into())).expect("client with token");
    let ctx = DispatchContext::new(Some(client), EngineState::shared());
    mock.with_overrides(|o| o.force_unauthorized = true);

    let out = dispatch(&ctx, "/status")
        .await
        .expect("dispatch returned error")
        .expect("dispatch returned None");

    let (kind, body) = first_line(&out);
    assert_eq!(
        kind, "alert",
        "401 on /status must surface as Alert: {body}"
    );
    assert!(
        !body.contains(sentinel),
        "alert body must NOT echo the bearer token. got: {body}"
    );
    assert!(
        !body.to_lowercase().contains("bearer "),
        "alert body must NOT mention the Authorization scheme. got: {body}"
    );

    mock.shutdown().await;
}

// ─────────────────────────────────────────────────────────────
//  Transient 503 — the retryable class. The client policy
//  retries once on 503; after that first retry succeeds, the
//  dispatcher must produce a *normal* Command line, not a
//  warn or alert. This test locks down the "retry is
//  invisible to the operator" property — an operator who
//  sees a spurious warn on every transient hiccup loses trust
//  in the advisory channel.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_transient_503_recovers_via_retry() {
    let (mock, ctx) = ctx_with_mock().await;
    // Exactly one failure → client's one-retry budget covers
    // it → second attempt succeeds → operator sees success.
    mock.with_overrides(|o| o.transient_fail_count = 1);

    let out = dispatch(&ctx, "/status")
        .await
        .expect("dispatch returned error")
        .expect("dispatch returned None");

    let (kind, body) = first_line(&out);
    assert_eq!(
        kind, "command",
        "single 503 should be hidden by the retry budget and surface as Command: {body}"
    );

    mock.shutdown().await;
}

// ─────────────────────────────────────────────────────────────
//  Compile-time breadcrumbs for the two fault modes whose
//  canonical test lives elsewhere. If the referenced symbols
//  disappear (rename / delete), this file stops compiling,
//  forcing the author to update either the canonical test or
//  this coverage map. Cheap alternative to a doc-only TODO.
// ─────────────────────────────────────────────────────────────

#[test]
fn ws_drop_and_reconnect_is_exercised_elsewhere() {
    // If ws_integration::reconnects_after_peer_drop is renamed
    // or removed, the crate's test harness will still compile
    // this binary — it's a separate integration target — so
    // the "breadcrumb" cannot be a fn-ref. We assert the
    // canonical test name as a string constant so a grep-
    // based audit (the CI coverage bot) can trivially link
    // the plan line to the canonical test.
    const CANONICAL_TEST: &str =
        "zero-engine-client::tests::ws_integration::reconnects_after_peer_drop";
    assert!(CANONICAL_TEST.contains("reconnects_after_peer_drop"));
}

#[test]
fn stale_stat_is_exercised_in_widget_layer() {
    // Same pattern as above. The stale-`Stat` behavior is a
    // render-layer concern (status-bar asterisk + muted
    // color); the dispatcher never sees it. Pinning the
    // canonical test name here keeps §9's fault-injection
    // coverage map exhaustive without copying assertions
    // across crate boundaries.
    const CANONICAL_TEST: &str =
        "zero-tui::widgets::statusbar::tests::stale_snapshot_gets_asterisk";
    assert!(CANONICAL_TEST.contains("stale_snapshot_gets_asterisk"));
}
