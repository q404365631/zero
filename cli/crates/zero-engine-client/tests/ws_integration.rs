//! End-to-end tests for `WsSubscriber` against the `zero-testkit`
//! mock. Covers: happy-path connect, event dispatch into
//! `EngineState`, reconnect after drop, and clean shutdown.

use std::time::Duration;

use zero_engine_client::{EngineEvent, EngineState, JitterMode, ReconnectConfig, WsSubscriber};
use zero_testkit::mock_engine::MockEngine;

/// Poll `predicate` up to `timeout`, sleeping 10 ms between
/// attempts. Returns `true` if the predicate returned `true`
/// within the window.
async fn wait_for<F>(timeout: Duration, mut predicate: F) -> bool
where
    F: FnMut() -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if predicate() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    predicate()
}

#[tokio::test]
async fn connects_and_marks_state() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let state = EngineState::shared();
    let sub = WsSubscriber::spawn(&mock.ws_url(), None, state.clone()).expect("subscribe");

    let connected = wait_for(Duration::from_secs(2), || {
        state.read().connection.ws_connected
    })
    .await;
    assert!(connected, "subscriber did not mark ws_connected within 2s");

    sub.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn applies_positions_and_risk_from_ws() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let state = EngineState::shared();
    let sub = WsSubscriber::spawn(&mock.ws_url(), None, state.clone()).expect("subscribe");

    let got_positions = wait_for(Duration::from_secs(2), || state.read().positions.is_some()).await;
    assert!(got_positions, "positions were not applied to EngineState");

    let got_risk = wait_for(Duration::from_secs(2), || state.read().risk.is_some()).await;
    assert!(got_risk, "risk was not applied to EngineState");

    {
        let s = state.read();
        let positions = s.positions.as_ref().unwrap();
        assert_eq!(positions.value.items.len(), 1);
        assert_eq!(positions.value.items[0].symbol, "BTC");
        let risk = s.risk.as_ref().unwrap();
        assert!(!risk.value.is_halted());
    }

    sub.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn broadcast_receives_typed_events() {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let state = EngineState::shared();
    let sub = WsSubscriber::spawn(&mock.ws_url(), None, state.clone()).expect("subscribe");
    let mut rx = sub.events();

    let mut saw_heartbeat = false;
    let mut saw_positions = false;
    let mut saw_risk = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline && !(saw_heartbeat && saw_positions && saw_risk) {
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Ok(EngineEvent::Heartbeat(_))) => saw_heartbeat = true,
            Ok(Ok(EngineEvent::Positions(_))) => saw_positions = true,
            Ok(Ok(EngineEvent::Risk(_))) => saw_risk = true,
            Ok(Ok(_) | Err(_)) | Err(_) => {}
        }
    }

    assert!(saw_heartbeat, "no heartbeat event");
    assert!(saw_positions, "no positions event");
    assert!(saw_risk, "no risk event");

    sub.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn reconnects_after_peer_drop() {
    let mock = MockEngine::spawn().await.expect("spawn mock");

    // Ask the mock to slam the first connection shut on accept.
    mock.with_overrides(|o| o.ws_drop_once = true);

    // Tight backoff so the test doesn't waste seconds.
    let state = EngineState::shared();
    let sub = WsSubscriber::spawn_with_config(
        &mock.ws_url(),
        None,
        state.clone(),
        ReconnectConfig {
            initial_backoff: Duration::from_millis(20),
            max_backoff: Duration::from_millis(200),
            multiplier: 2,
            // Deterministic timing for the drop-recovery assertion —
            // `JitterMode::Full` would still work (cap is only 200 ms)
            // but `None` keeps the test's wall-clock envelope tight.
            jitter: JitterMode::None,
        },
    )
    .expect("subscribe");

    // Second attempt succeeds (the override auto-unsets after one drop).
    let reconnected = wait_for(Duration::from_secs(2), || {
        state.read().connection.ws_connected && state.read().positions.is_some()
    })
    .await;
    assert!(reconnected, "subscriber did not recover after peer drop");

    // Lifetime counter must record both the failed attempt and the
    // successful one. `reconnect_count` is allowed to be 0 by now
    // because the recovery resets it.
    assert!(
        state.read().connection.total_attempts >= 2,
        "total_attempts should be >= 2 (first drop + successful reconnect); got {}",
        state.read().connection.total_attempts
    );
    assert_eq!(
        state.read().connection.reconnect_count,
        0,
        "reconnect_count must reset to 0 on success"
    );

    sub.shutdown().await.expect("shutdown");
    mock.shutdown().await;
}

#[tokio::test]
async fn shutdown_is_clean_when_peer_is_down() {
    // Spawn against a port nothing is listening on. The subscriber
    // should be retrying in the background; shutdown must still
    // return promptly.
    let state = EngineState::shared();
    let sub = WsSubscriber::spawn_with_config(
        "ws://127.0.0.1:1/ws",
        None,
        state,
        ReconnectConfig {
            initial_backoff: Duration::from_millis(20),
            max_backoff: Duration::from_millis(100),
            multiplier: 2,
            jitter: JitterMode::None,
        },
    )
    .expect("subscribe");

    // Give it a few failed attempts.
    tokio::time::sleep(Duration::from_millis(80)).await;

    let result = tokio::time::timeout(Duration::from_secs(2), sub.shutdown()).await;
    assert!(result.is_ok(), "shutdown did not return within 2s");
    result.unwrap().expect("shutdown ok");
}
