//! Background pollers for engine endpoints that do not have a
//! dedicated push channel.
//!
//! At present, that's one surface: `GET /operator/state`. The
//! classifier runs on the engine host (ADR-016) and the endpoint is
//! the CLI's only window into the current behavioral label. The
//! poller runs at a deliberately relaxed cadence — operator state
//! changes on human time scales, and over-polling an engine we do
//! not own is rude.
//!
//! Transient errors are swallowed with a warn-log; the subscriber
//! task keeps trying. Fatal errors (bad base URL, unauth) surface
//! at construction time. The mirror's `Stat<Snapshot>` freshness
//! metadata is what the widget uses to distinguish "never polled"
//! from "stale" from "fresh" — see `statusbar.rs` for the render
//! side.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::RwLock;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::http::{HttpClient, HttpError};
use crate::stat::Source;
use crate::state::EngineState;

/// How often the poller fetches `/operator/state`. 5 s matches the
/// Addendum A §2 target classifier tick cadence — faster than the
/// operator can notice a mislabel, slower than a naive polling loop.
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// How long to wait after a failed request before retrying. We
/// intentionally do **not** escalate backoff here: if the engine is
/// down, the status bar already shows `engine:DOWN` via the WS
/// subscriber, and the operator-state segment degrading to `ops:?`
/// after 30 s is the honest rendering (see
/// `OPERATOR_STATE_STALE_AFTER` in the widget).
pub const POLL_BACKOFF: Duration = Duration::from_secs(5);

/// Handle to a running operator-state poller.
///
/// Dropping the handle does **not** stop the task; callers must
/// explicitly `.shutdown().await` for a clean exit. Matches the
/// `WsSubscriber` handle's semantics.
#[derive(Debug)]
pub struct OperatorStatePoller {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl OperatorStatePoller {
    /// Spawn a poller that writes into `state` every
    /// [`POLL_INTERVAL`]. Returns immediately — the first fetch
    /// happens in the background task on the first tick.
    #[must_use]
    pub fn spawn(http: HttpClient, state: Arc<RwLock<EngineState>>) -> Self {
        Self::spawn_with_interval(http, state, POLL_INTERVAL)
    }

    /// Like [`Self::spawn`] with a custom interval — used by tests
    /// that want to exercise multiple polls in a short wall-clock
    /// window.
    #[must_use]
    pub fn spawn_with_interval(
        http: HttpClient,
        state: Arc<RwLock<EngineState>>,
        interval: Duration,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(run_loop(http, state, interval, shutdown_rx));
        Self { shutdown_tx, task }
    }

    /// Signal the task to exit and wait for it.
    ///
    /// # Errors
    /// Returns the tokio join error verbatim on panic or cancel;
    /// clean shutdown always returns `Ok`.
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        let _ = self.shutdown_tx.send(true);
        self.task.await
    }
}

async fn run_loop(
    http: HttpClient,
    state: Arc<RwLock<EngineState>>,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = ticker.tick() => {
                match http.operator_state().await {
                    Ok(snap) => {
                        state.write().apply_operator_state(snap, Utc::now());
                    }
                    Err(e) => {
                        // 404 means the engine is older than ADR-016
                        // and does not expose the endpoint. Log once
                        // at info, then back off to a warn on repeat
                        // failures.
                        match &e {
                            HttpError::NotFound { .. } => {
                                tracing::debug!("operator-state endpoint not served; continuing");
                            }
                            _ => {
                                tracing::warn!(err = %e, "operator-state poll failed");
                            }
                        }
                        tokio::select! {
                            () = tokio::time::sleep(POLL_BACKOFF) => {}
                            _ = shutdown.changed() => break,
                        }
                    }
                }
            }
        }
    }

    tracing::debug!("operator-state poller exited");
}

// ─── HTTP backfill for the core mirror fields ──────────────────────
//
// The WS subscriber is the primary source for `status` / `positions`
// / `risk` / `regime`. The backfill poller is a defense-in-depth
// layer: when the WS is reconnecting (or the engine temporarily
// stops emitting an event type), this task keeps the mirror
// populated via cheap HTTP polls. Writes are tagged `Source::Http`
// so the rendered `Stat<T>` makes the provenance honest.
//
// The cadence is deliberately slow (30 s by default) because:
//
// * Push is the main path; this is backfill, not the source of truth.
// * The CLI should never flood an engine it does not own.
// * `last_heartbeat` is only bumped by WS updates (see
//   `EngineState::apply_*`), so this poller cannot paper over a
//   stalled feed — the status bar still goes amber → red if the bus
//   stops, even while backfill keeps writing.

/// How often the backfill poller pulls each endpoint. 30 s is slow
/// enough to be polite to the engine and fast enough that a WS
/// dropout doesn't leave the operator staring at a stale mirror
/// for long. Callers that want a different cadence (tests, mainly)
/// use [`EngineStatePoller::spawn_with_interval`].
pub const BACKFILL_INTERVAL: Duration = Duration::from_secs(30);

/// Backoff after a backfill failure. Matches [`POLL_BACKOFF`] —
/// if the engine is down the WS-side indicator already tells the
/// operator; doubling the visible cadence helps no one.
pub const BACKFILL_BACKOFF: Duration = Duration::from_secs(5);

/// Handle to a running HTTP-backfill poller for the core mirror
/// fields.
///
/// Same lifecycle semantics as [`OperatorStatePoller`]: dropping
/// the handle does **not** stop the task; call
/// [`EngineStatePoller::shutdown`] for a clean exit.
#[derive(Debug)]
pub struct EngineStatePoller {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl EngineStatePoller {
    /// Spawn a backfill poller for `status` / `positions` / `risk`
    /// / `regime`. Returns immediately — the first poll happens in
    /// the background task on the first tick.
    #[must_use]
    pub fn spawn(http: HttpClient, state: Arc<RwLock<EngineState>>) -> Self {
        Self::spawn_with_interval(http, state, BACKFILL_INTERVAL)
    }

    /// Like [`Self::spawn`] with a custom interval. Used by tests
    /// to exercise multiple polls in a short wall-clock window.
    #[must_use]
    pub fn spawn_with_interval(
        http: HttpClient,
        state: Arc<RwLock<EngineState>>,
        interval: Duration,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(backfill_loop(http, state, interval, shutdown_rx));
        Self { shutdown_tx, task }
    }

    /// Signal the task to exit and wait for it.
    ///
    /// # Errors
    /// Returns the tokio join error verbatim on panic or cancel;
    /// clean shutdown always returns `Ok`.
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        let _ = self.shutdown_tx.send(true);
        self.task.await
    }
}

async fn backfill_loop(
    http: HttpClient,
    state: Arc<RwLock<EngineState>>,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = ticker.tick() => {
                let failed = fetch_and_apply(&http, &state).await;
                if failed {
                    tokio::select! {
                        () = tokio::time::sleep(BACKFILL_BACKOFF) => {}
                        _ = shutdown.changed() => break,
                    }
                }
            }
        }
    }

    tracing::debug!("engine-state backfill poller exited");
}

/// Fetch each tracked endpoint in sequence, applying successes
/// and logging individual failures. Returns `true` if any
/// endpoint failed so the caller can apply a short backoff; a
/// per-call `false` would starve the mirror of the endpoints
/// that *did* work.
async fn fetch_and_apply(http: &HttpClient, state: &Arc<RwLock<EngineState>>) -> bool {
    let mut any_failed = false;
    let now = Utc::now();

    match http.v2_status().await {
        Ok(s) => state.write().apply_status(s, now, Source::Http),
        Err(e) => {
            log_backfill_error("v2_status", &e);
            any_failed = true;
        }
    }
    match http.positions().await {
        Ok(p) => state.write().apply_positions(p, now, Source::Http),
        Err(e) => {
            log_backfill_error("positions", &e);
            any_failed = true;
        }
    }
    match http.risk().await {
        Ok(r) => state.write().apply_risk(r, now, Source::Http),
        Err(e) => {
            log_backfill_error("risk", &e);
            any_failed = true;
        }
    }
    match http.regime(None).await {
        Ok(r) => state.write().apply_regime(r, now, Source::Http),
        Err(e) => {
            log_backfill_error("regime", &e);
            any_failed = true;
        }
    }

    any_failed
}

fn log_backfill_error(endpoint: &'static str, err: &HttpError) {
    match err {
        // 404 means the engine doesn't expose this endpoint (older
        // build, or locked-down surface). Log once at debug — repeat
        // log-spam is worse than the missing data.
        HttpError::NotFound { .. } => {
            tracing::debug!(endpoint, "backfill endpoint not served; continuing");
        }
        // 401 means auth isn't wired yet; the WS reconnect loop is
        // already shouting about it. Keep it quiet here.
        HttpError::Unauthorized => {
            tracing::debug!(endpoint, "backfill auth rejected; continuing");
        }
        _ => {
            tracing::warn!(endpoint, err = %err, "backfill poll failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn poller_writes_snapshot_on_first_tick() {
        let mock = zero_testkit::mock_engine::MockEngine::spawn()
            .await
            .expect("mock up");
        let http = HttpClient::new(mock.base_url(), None).expect("client");
        let state = EngineState::shared();

        let poller = OperatorStatePoller::spawn_with_interval(
            http,
            state.clone(),
            Duration::from_millis(10),
        );

        // Wait up to 1 s for the first snapshot to land.
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            if state.read().operator_state.is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() <= deadline,
                "snapshot never arrived"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        poller.shutdown().await.expect("clean shutdown");
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn poller_picks_up_label_changes() {
        let mock = zero_testkit::mock_engine::MockEngine::spawn()
            .await
            .expect("mock up");
        let http = HttpClient::new(mock.base_url(), None).expect("client");
        let state = EngineState::shared();

        let poller = OperatorStatePoller::spawn_with_interval(
            http,
            state.clone(),
            Duration::from_millis(10),
        );

        // Wait for STEADY (default).
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            if matches!(
                state.read().operator_state.as_ref().map(|s| s.value.label),
                Some(zero_operator_state::Label::Steady)
            ) {
                break;
            }
            assert!(
                std::time::Instant::now() <= deadline,
                "steady never arrived"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        // Flip to TILT.
        mock.with_overrides(|o| {
            o.operator_label = Some("tilt".to_string());
            o.operator_version += 1;
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            if matches!(
                state.read().operator_state.as_ref().map(|s| s.value.label),
                Some(zero_operator_state::Label::Tilt)
            ) {
                break;
            }
            assert!(
                std::time::Instant::now() <= deadline,
                "tilt never propagated"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        poller.shutdown().await.expect("clean shutdown");
        mock.shutdown().await;
    }

    // ── EngineStatePoller (HTTP backfill) ──────────────────────────

    #[tokio::test]
    async fn backfill_populates_all_tracked_fields() {
        let mock = zero_testkit::mock_engine::MockEngine::spawn()
            .await
            .expect("mock up");
        let http = HttpClient::new(mock.base_url(), None).expect("client");
        let state = EngineState::shared();

        let poller =
            EngineStatePoller::spawn_with_interval(http, state.clone(), Duration::from_millis(10));

        // Wait up to 2 s — four sequential HTTP calls per tick,
        // each ~1 ms on localhost, but CI schedulers are capricious.
        // The read-guard is scoped inside the block so it's dropped
        // before the next `.await` (clippy's await-holding-lock lint
        // forbids carrying parking_lot guards across yield points).
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let ready = {
                let s = state.read();
                s.status.is_some()
                    && s.positions.is_some()
                    && s.risk.is_some()
                    && s.regime.is_some()
            };
            if ready {
                break;
            }
            assert!(
                std::time::Instant::now() <= deadline,
                "not all fields backfilled in time"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        // All four must be tagged HTTP — backfill path, not push.
        // Snapshot the relevant bits inside a scoped read so the
        // guard is released before the shutdown awaits below.
        let (src_status, src_positions, src_risk, src_regime, heartbeat) = {
            let s = state.read();
            (
                s.status.as_ref().unwrap().source,
                s.positions.as_ref().unwrap().source,
                s.risk.as_ref().unwrap().source,
                s.regime.as_ref().unwrap().source,
                s.last_heartbeat,
            )
        };
        assert!(matches!(src_status, Source::Http));
        assert!(matches!(src_positions, Source::Http));
        assert!(matches!(src_risk, Source::Http));
        assert!(matches!(src_regime, Source::Http));
        assert!(
            heartbeat.is_none(),
            "HTTP backfill must not bump last_heartbeat — that's a WS-only signal",
        );

        poller.shutdown().await.expect("clean shutdown");
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn backfill_survives_transient_503_via_retry() {
        let mock = zero_testkit::mock_engine::MockEngine::spawn()
            .await
            .expect("mock up");
        // Single 503 on the next request; the retry-once policy
        // should absorb it and the poller should still populate
        // the mirror on its first tick.
        mock.with_overrides(|o| o.transient_fail_count = 1);

        let http = HttpClient::new(mock.base_url(), None).expect("client");
        let state = EngineState::shared();

        let poller =
            EngineStatePoller::spawn_with_interval(http, state.clone(), Duration::from_millis(10));

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            // Scoped read — guard dropped before the await below.
            let ready = state.read().status.is_some();
            if ready {
                break;
            }
            assert!(
                std::time::Instant::now() <= deadline,
                "status never backfilled (retry may have mis-behaved)"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        poller.shutdown().await.expect("clean shutdown");
        mock.shutdown().await;
    }
}
