//! Integration tests for the supervisor daemon.
//!
//! Each test stands up a real `Daemon` bound to a temp-dir
//! socket, drives it through the `Client`, then shuts it down.
//! The tests exercise the same code path as production (the
//! binary is a thin wrapper around `Daemon::run`) so a green
//! suite here is strong evidence the `zero-headlessd` binary
//! behaves the same way — no mocking of the socket listener.
//!
//! The 8 tests map 1:1 to M2 §6's required integration-test
//! list. "install + spawn" and "kill via SIGTERM" are the
//! specimen cases we can exercise without invoking launchd /
//! systemd or sending POSIX signals to our own test runner —
//! the `Shutdown::shutdown` handle is the same sink SIGTERM
//! writes into, so simulating it is faithful.

use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use tokio::time::timeout;
use zero_headless::daemon::{AlwaysReachable, AlwaysUnreachable, Config, Daemon};
use zero_headless::{
    Client, EngineHealth, PersistError, Request, Response, State, SupervisorState,
};

fn make_config(dir: &TempDir) -> Config {
    Config::for_test(dir.path())
}

async fn spawn_daemon(cfg: Config) -> zero_headless::daemon::Shutdown {
    let daemon = Daemon::new(cfg, Arc::new(AlwaysReachable)).expect("daemon init should succeed");
    let mut handle = daemon.spawn().expect("daemon spawn");
    wait_for_socket(handle.socket_path()).await;
    // Give the accept loop a moment to enter its `select!`
    // — otherwise an immediate dial may race the listener's
    // first poll on slower CI runners.
    tokio::time::sleep(Duration::from_millis(20)).await;
    // `handle` is returned by value so the caller can call
    // `.shutdown()` / `.join()`. The forgotten mut here is
    // intentional: tests own the handle's lifecycle.
    let _ = &mut handle;
    handle
}

async fn wait_for_socket(path: &std::path::Path) {
    for _ in 0..50 {
        if path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("socket never appeared at {}", path.display());
}

/// Test 1 — **install + spawn.** A fresh daemon stands up,
/// creates its socket, and answers a `Status` request. We
/// can't invoke `launchctl`/`systemctl` from a unit test, but
/// the library-spawn path below is what both supervisors call
/// into, so a green test here is proof the spawn path itself
/// works.
#[tokio::test(flavor = "multi_thread")]
async fn install_and_spawn() {
    let dir = TempDir::new().unwrap();
    let mut handle = spawn_daemon(make_config(&dir)).await;

    let client = Client::new(handle.socket_path().to_path_buf());
    let reply = client.send(&Request::Status).await.expect("status");
    match reply {
        Response::Status(s) => {
            assert_eq!(s.state, SupervisorState::Off); // default intent
            assert_eq!(s.protocol_version, zero_headless::PROTOCOL_VERSION);
        }
        other => panic!("expected Status, got {other:?}"),
    }

    handle.shutdown();
    handle.join().await.unwrap();
}

/// Test 2 — **graceful stop.** The shutdown path closes the
/// accept loop, drains in-flight connections, and exits with
/// `Ok(())`. No panic, no leaked socket file.
#[tokio::test(flavor = "multi_thread")]
async fn graceful_stop_cleans_up() {
    let dir = TempDir::new().unwrap();
    let handle = spawn_daemon(make_config(&dir)).await;
    let socket_path = handle.socket_path().to_path_buf();

    assert!(
        socket_path.exists(),
        "socket should exist while daemon runs"
    );
    handle.join().await.expect("daemon exit clean");
    assert!(
        !socket_path.exists(),
        "graceful stop must remove socket file: {socket_path:?}",
    );
}

/// Test 3 — **socket round-trip.** Start → Stop → Status
/// flips the intent flag and the status reply reflects every
/// edge. The client and the daemon agree on the wire format
/// by construction.
#[tokio::test(flavor = "multi_thread")]
async fn socket_round_trip_tracks_state() {
    let dir = TempDir::new().unwrap();
    let mut handle = spawn_daemon(make_config(&dir)).await;
    let client = Client::new(handle.socket_path().to_path_buf());

    let r = client.send(&Request::Start).await.unwrap();
    assert!(
        matches!(
            r,
            Response::Accepted {
                state: SupervisorState::On,
                ..
            }
        ),
        "Start → Accepted(on), got {r:?}",
    );

    let r = client.send(&Request::Status).await.unwrap();
    match r {
        Response::Status(s) => assert_eq!(s.state, SupervisorState::On),
        other => panic!("status after start = {other:?}"),
    }

    let r = client.send(&Request::Stop).await.unwrap();
    assert!(
        matches!(
            r,
            Response::Accepted {
                state: SupervisorState::Off,
                ..
            }
        ),
        "Stop → Accepted(off), got {r:?}",
    );

    handle.shutdown();
    handle.join().await.unwrap();
}

/// Test 4 — **recovery on SIGTERM.** The daemon's SIGTERM
/// handler drops into the same `shutdown` oneshot the
/// test-side `Shutdown::shutdown()` does. We exercise that
/// path end-to-end: spawn, dial, shutdown, await — no leaks,
/// no dangling tasks.
#[tokio::test(flavor = "multi_thread")]
async fn simulated_sigterm_drains_cleanly() {
    let dir = TempDir::new().unwrap();
    let mut handle = spawn_daemon(make_config(&dir)).await;
    let client = Client::new(handle.socket_path().to_path_buf());

    // Send a Start just so there's state worth preserving.
    let _ = client.send(&Request::Start).await.unwrap();

    // Fire the shutdown sink — the same one a real SIGTERM
    // would push onto via the daemon's signal handler.
    handle.shutdown();

    // Must exit within a sane deadline; a hang here is the
    // whole reason we wrote the drain-barrier mpsc.
    timeout(Duration::from_secs(3), handle.join())
        .await
        .expect("graceful drain within 3s")
        .expect("daemon exit ok");
}

/// Test 5 — **state persistence across restart.** The
/// operator's intent survives a daemon bounce. Without this,
/// a crash-and-restart would silently revert the supervisor
/// to "off" — exactly the 2 AM failure mode §6 rejects.
#[tokio::test(flavor = "multi_thread")]
async fn intent_survives_restart() {
    let dir = TempDir::new().unwrap();
    let cfg = make_config(&dir);

    // First boot: arm.
    {
        let mut handle = spawn_daemon(cfg.clone()).await;
        let client = Client::new(handle.socket_path().to_path_buf());
        let _ = client.send(&Request::Start).await.unwrap();
        handle.shutdown();
        handle.join().await.unwrap();
    }

    // Verify the state file on disk agrees with us.
    let state = State::load(&cfg.state_path).unwrap();
    assert_eq!(state.intent, SupervisorState::On);

    // Second boot: without any further Start, the daemon must
    // load the intent as On and answer Status accordingly.
    {
        let mut handle = spawn_daemon(cfg.clone()).await;
        let client = Client::new(handle.socket_path().to_path_buf());
        let r = client.send(&Request::Status).await.unwrap();
        match r {
            Response::Status(s) => assert_eq!(
                s.state,
                SupervisorState::On,
                "intent flag should survive restart",
            ),
            other => panic!("expected Status, got {other:?}"),
        }
        handle.shutdown();
        handle.join().await.unwrap();
    }
}

/// Test 6 — **kill-switch via CLI.** The `Request::Kill` path
/// records a `Killed` action durably and accepts the request.
/// A real CLI's `/kill` then also removes the socket file —
/// that's the `SupervisorSource::tear_down_socket` half, not
/// the daemon's concern here.
#[tokio::test(flavor = "multi_thread")]
async fn kill_switch_over_socket_records_action() {
    let dir = TempDir::new().unwrap();
    let cfg = make_config(&dir);
    let mut handle = spawn_daemon(cfg.clone()).await;
    let client = Client::new(handle.socket_path().to_path_buf());

    let r = client.send(&Request::Kill).await.unwrap();
    assert!(
        matches!(
            r,
            Response::Accepted {
                state: SupervisorState::Off,
                ..
            }
        ),
        "Kill → Accepted(off), got {r:?}",
    );

    // State file records the kill.
    let s = State::load(&cfg.state_path).unwrap();
    let last = s
        .recent_actions
        .first()
        .expect("at least one action recorded");
    assert_eq!(last.kind, zero_headless::ActionKind::Killed);
    assert!(!last.note.is_empty(), "note must not be empty");

    handle.shutdown();
    handle.join().await.unwrap();
}

/// Test 7 — **kill-switch via SIGTERM.** Equivalent to test
/// 4 in mechanism but named for the spec's checklist item.
/// We assert the daemon's state-file was not corrupted by a
/// SIGTERM-path shutdown, even if in-flight work was
/// underway.
#[tokio::test(flavor = "multi_thread")]
async fn sigterm_does_not_corrupt_state() {
    let dir = TempDir::new().unwrap();
    let cfg = make_config(&dir);
    let mut handle = spawn_daemon(cfg.clone()).await;
    let client = Client::new(handle.socket_path().to_path_buf());

    let _ = client.send(&Request::Start).await.unwrap();

    // Simulate a SIGTERM mid-operation. Because `Start`
    // returned `Accepted` before we shutdown, the state file
    // was already written atomically; we assert that's what
    // a fresh load sees.
    handle.shutdown();
    handle.join().await.unwrap();

    let back = State::load(&cfg.state_path).unwrap();
    assert_eq!(back.intent, SupervisorState::On);
    match State::load(&cfg.state_path) {
        Ok(_) => {}
        Err(PersistError::Parse { .. }) => panic!("SIGTERM corrupted state file"),
        Err(PersistError::Io { .. }) => panic!("state file disappeared on SIGTERM"),
    }
}

/// Test 8 — **refuses to run without config.** The daemon
/// init path returns `MissingConfig` when `Config.config_present
/// = false`, so the binary's `ExitCode::from(2)` path fires.
/// No socket is ever created.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_run_without_config() {
    let dir = TempDir::new().unwrap();
    let mut cfg = make_config(&dir);
    cfg.config_present = false;

    let err = Daemon::new(cfg.clone(), Arc::new(AlwaysReachable)).unwrap_err();
    assert!(
        matches!(err, zero_headless::daemon::DaemonError::MissingConfig(_)),
        "expected MissingConfig, got {err:?}",
    );

    // And no socket file should have been created.
    assert!(!cfg.socket_path.exists());
}

/// Bonus coverage — honest engine unreachable path. Not one
/// of the eight spec-required tests but close enough in
/// spirit that regressing it would be a silent lie, so we
/// pin it here.
#[tokio::test(flavor = "multi_thread")]
async fn status_reports_unreachable_engine_honestly() {
    let dir = TempDir::new().unwrap();
    let cfg = make_config(&dir);
    let daemon = Daemon::new(cfg, Arc::new(AlwaysUnreachable)).unwrap();
    let mut handle = daemon.spawn().unwrap();
    wait_for_socket(handle.socket_path()).await;

    let client = Client::new(handle.socket_path().to_path_buf());
    let r = client.send(&Request::Status).await.unwrap();
    match r {
        Response::Status(s) => assert!(
            matches!(s.engine, EngineHealth::Unreachable { .. }),
            "engine should be honestly unreachable, got {:?}",
            s.engine,
        ),
        other => panic!("expected Status, got {other:?}"),
    }

    handle.shutdown();
    handle.join().await.unwrap();
}
