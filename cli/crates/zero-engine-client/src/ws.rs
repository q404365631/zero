//! WebSocket subscriber for the engine's `/ws` push surface.
//!
//! Subscribes to the engine's broadcast channel, decodes typed
//! events, and folds them into an `EngineState` mirror. Handles
//! reconnection with exponential backoff; the TUI status bar reads
//! `EngineState::connection` to render a DEGRADED banner during
//! partition.
//!
//! Mirrors `ConnectionManager.broadcast()` in the engine's FastAPI
//! server — event shape is `{event: string, ts: iso8601, data: object}`.
//! Unknown event kinds are preserved in [`EngineEvent::Unknown`] so
//! the engine can evolve its push surface without breaking the
//! subscriber.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::StreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite;

use crate::models::{Positions, Regime, Risk, V2Status};
use crate::stat::Source;
use crate::state::EngineState;

/// Raw event shape pushed by the engine's bus poller.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawEvent {
    event: String,
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    data: serde_json::Value,
}

/// Typed event the subscriber emits to consumers. Known events are
/// decoded into strong types; anything else lands in
/// [`EngineEvent::Unknown`] with the raw payload.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Heartbeat(DateTime<Utc>),
    Status(Box<V2Status>),
    Positions(Box<Positions>),
    Risk(Box<Risk>),
    Regime(Box<Regime>),
    Unknown {
        event: String,
        ts: DateTime<Utc>,
        data: serde_json::Value,
    },
}

/// Errors the subscriber can surface to its caller. Reconnectable
/// errors are handled internally via backoff and never bubble out;
/// only construction-time and shutdown errors reach the caller.
#[derive(Debug, thiserror::Error)]
pub enum WsError {
    #[error("invalid websocket url: {0}")]
    InvalidUrl(String),
    #[error("subscriber shutdown failed: {0}")]
    Shutdown(String),
}

/// How to jitter an exponential backoff delay before sleeping.
///
/// The industry-standard approach for reconnect loops (Marc Brooker,
/// AWS Architecture Blog) is "full jitter": sleep for a uniformly
/// random duration in `[0, exp_backoff]` rather than sleeping for
/// exactly `exp_backoff`. This breaks synchronized reconnect waves
/// across many clients and keeps the cluster's recovery time tight
/// even when a partition heals for everyone at once.
///
/// Zero ships with one CLI per operator, so the "thundering herd"
/// is thin, but the cost of jitter is zero and the story stays
/// consistent across hosted deployments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JitterMode {
    /// Sleep for exactly `exp_backoff`. Deterministic; intended for
    /// tests that need to assert exact reconnect timing.
    None,
    /// Sleep for `rand_uniform(0, exp_backoff)` — the "full jitter"
    /// variant. Default for production.
    #[default]
    Full,
}

/// Configuration for the subscriber's reconnect behavior.
///
/// The backoff sequence is `min(initial * multiplier^attempt, max)`,
/// then passed through [`JitterMode`] to produce the actual sleep
/// duration. On a successful read (see `ReadOutcome::Connected`),
/// the attempt counter resets to zero.
#[derive(Debug, Clone, Copy)]
pub struct ReconnectConfig {
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub multiplier: u32,
    pub jitter: JitterMode,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            multiplier: 2,
            jitter: JitterMode::default(),
        }
    }
}

/// Compute the exponential-backoff *cap* for a given attempt count
/// (0-based: attempt 0 uses `initial`, attempt 1 uses
/// `initial * multiplier`, etc.), clamped to `max`.
///
/// Pure, const-friendly, and branch-free on overflow: a runaway
/// attempt count saturates at `max_backoff` rather than wrapping.
/// Split out as a free function so tests can exercise the full
/// sequence without spinning up a subscriber.
#[must_use]
pub fn exp_backoff_cap(
    initial: Duration,
    max: Duration,
    multiplier: u32,
    attempt: u32,
) -> Duration {
    // Compute `initial * multiplier^attempt` in `u128` to avoid
    // premature overflow; cap at `max` on the way out. `multiplier`
    // of 0 or 1 still behaves sanely (stays at `initial`).
    let base_ms = u128::from(u64::try_from(initial.as_millis()).unwrap_or(u64::MAX));
    let mul = u128::from(multiplier.max(1));
    let mut factor: u128 = 1;
    for _ in 0..attempt {
        factor = factor.saturating_mul(mul);
        // Once factor * base would exceed max, we've saturated; stop
        // multiplying to avoid wasteful work on high attempt counts.
        if factor.saturating_mul(base_ms) >= max.as_millis() {
            break;
        }
    }
    let scaled_ms = factor.saturating_mul(base_ms);
    let capped_ms = scaled_ms.min(max.as_millis());
    // `max_millis` can't exceed `u64::MAX` — `Duration::as_millis`
    // returns `u128` but any practical `max_backoff` fits in `u64`.
    Duration::from_millis(u64::try_from(capped_ms).unwrap_or(u64::MAX))
}

/// Apply `mode` to a computed backoff cap.
///
/// Pure + seedable for tests: `rng` produces the next random `u64`
/// used to scale the cap when `mode` is [`JitterMode::Full`]. The
/// result is always `<= cap` so the `max_backoff` invariant holds.
#[must_use]
pub fn apply_jitter(cap: Duration, mode: JitterMode, rng: &mut dyn FnMut() -> u64) -> Duration {
    match mode {
        JitterMode::None => cap,
        JitterMode::Full => {
            let ms = u64::try_from(cap.as_millis()).unwrap_or(u64::MAX);
            if ms == 0 {
                return Duration::ZERO;
            }
            // `rng() % (ms + 1)` so the range is [0, ms] inclusive;
            // saturating the +1 keeps the upper bound at u64::MAX.
            let modulus = ms.saturating_add(1);
            Duration::from_millis(rng() % modulus)
        }
    }
}

/// Tiny xorshift64 RNG used for jitter. Cryptographic quality is
/// not required — we just want to decorrelate reconnect waves.
/// Kept inline to avoid pulling the `rand` crate (and its six
/// transitive deps) into the release-small binary-size budget.
#[derive(Debug, Clone, Copy)]
struct XorshiftRng {
    state: u64,
}

impl XorshiftRng {
    fn seeded_from_now() -> Self {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        // Mix in a thread-id-like byte so two subscribers spawned
        // in the same tick don't march in lockstep.
        let seed = u64::try_from(ns & u128::from(u64::MAX)).unwrap_or(1);
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64* — Marsaglia, 2003.
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

/// Handle to a running WS subscriber task.
///
/// Dropping the handle does not stop the task — use
/// [`WsSubscriber::shutdown`] for a clean exit. This is deliberate:
/// the TUI passes the handle around via `Arc` and the subscriber
/// outlives widgets that only need to subscribe to events.
#[derive(Debug)]
pub struct WsSubscriber {
    state: Arc<RwLock<EngineState>>,
    events: broadcast::Sender<EngineEvent>,
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl WsSubscriber {
    /// Spawn a subscriber against `url`, authenticating with
    /// `token` if provided.
    ///
    /// Returns immediately; the connect attempt happens in the
    /// background task. Consumers poll [`EngineState::connection`]
    /// on the shared state to learn whether the first connection
    /// has landed.
    ///
    /// # Errors
    /// Returns [`WsError::InvalidUrl`] if the url cannot be parsed.
    pub fn spawn(
        url: &str,
        token: Option<String>,
        state: Arc<RwLock<EngineState>>,
    ) -> Result<Self, WsError> {
        Self::spawn_with_config(url, token, state, ReconnectConfig::default())
    }

    /// Like [`Self::spawn`] with a custom reconnect policy; used by
    /// tests that want to exercise backoff without real wall-clock
    /// delay.
    ///
    /// # Errors
    /// Returns [`WsError::InvalidUrl`] if the url cannot be parsed.
    pub fn spawn_with_config(
        url: &str,
        token: Option<String>,
        state: Arc<RwLock<EngineState>>,
        reconnect: ReconnectConfig,
    ) -> Result<Self, WsError> {
        let url = url::Url::parse(url).map_err(|e| WsError::InvalidUrl(e.to_string()))?;
        if !matches!(url.scheme(), "ws" | "wss") {
            return Err(WsError::InvalidUrl(format!(
                "unexpected scheme: {}",
                url.scheme()
            )));
        }

        let (events, _) = broadcast::channel(128);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let task = tokio::spawn(run_loop(
            url,
            token,
            state.clone(),
            events.clone(),
            shutdown_rx,
            reconnect,
        ));

        Ok(Self {
            state,
            events,
            shutdown_tx,
            task,
        })
    }

    /// Shared handle to the engine-state mirror. Widgets clone this
    /// and acquire `.read()` for the duration of a render pass.
    #[must_use]
    pub fn state(&self) -> Arc<RwLock<EngineState>> {
        self.state.clone()
    }

    /// Subscribe to the raw typed event stream. Each subscriber
    /// gets its own receiver; slow consumers are dropped by tokio's
    /// broadcast channel, which is appropriate for a push firehose.
    #[must_use]
    pub fn events(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    /// Signal the task to exit and wait for it.
    ///
    /// # Errors
    /// Returns [`WsError::Shutdown`] only when the task panicked or
    /// was cancelled externally — a clean exit always returns `Ok`.
    pub async fn shutdown(self) -> Result<(), WsError> {
        let _ = self.shutdown_tx.send(true);
        self.task
            .await
            .map_err(|e| WsError::Shutdown(e.to_string()))
    }
}

async fn run_loop(
    url: url::Url,
    token: Option<String>,
    state: Arc<RwLock<EngineState>>,
    events: broadcast::Sender<EngineEvent>,
    mut shutdown: watch::Receiver<bool>,
    reconnect: ReconnectConfig,
) {
    // Attempt counter drives the exponential cap. Reset on a clean
    // connection (`ReadOutcome::Connected`) so a one-off disconnect
    // doesn't leave us sleeping 30 s after it heals.
    let mut attempt: u32 = 0;
    let mut rng = XorshiftRng::seeded_from_now();

    loop {
        if *shutdown.borrow() {
            break;
        }

        state.write().on_reconnect_attempt(Utc::now());

        match connect_and_read(&url, token.as_deref(), &state, &events, &mut shutdown).await {
            ReadOutcome::Shutdown => break,
            ReadOutcome::Disconnected => {
                state.write().on_ws_disconnected();

                let cap = exp_backoff_cap(
                    reconnect.initial_backoff,
                    reconnect.max_backoff,
                    reconnect.multiplier,
                    attempt,
                );
                let sleep = apply_jitter(cap, reconnect.jitter, &mut || rng.next_u64());
                let sleep_ms = u64::try_from(sleep.as_millis()).unwrap_or(u64::MAX);
                let cap_ms = u64::try_from(cap.as_millis()).unwrap_or(u64::MAX);
                tracing::warn!(
                    attempt,
                    cap_ms,
                    sleep_ms,
                    "ws disconnected, retrying with jittered backoff"
                );

                tokio::select! {
                    () = tokio::time::sleep(sleep) => {}
                    _ = shutdown.changed() => break,
                }

                attempt = attempt.saturating_add(1);
            }
            ReadOutcome::Connected => {
                // Only reset after a frame actually landed — a hang
                // that ended before any read wouldn't count as
                // successful recovery.
                attempt = 0;
            }
        }
    }

    tracing::debug!("ws subscriber task exited");
}

enum ReadOutcome {
    /// Reached after a full handshake + at least one frame.
    Connected,
    /// Connection failed or was lost; reconnect loop should retry.
    Disconnected,
    /// Shutdown channel fired; reconnect loop should exit.
    Shutdown,
}

async fn connect_and_read(
    url: &url::Url,
    token: Option<&str>,
    state: &Arc<RwLock<EngineState>>,
    events: &broadcast::Sender<EngineEvent>,
    shutdown: &mut watch::Receiver<bool>,
) -> ReadOutcome {
    let request = match build_request(url, token) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(err = %e, "invalid ws request");
            return ReadOutcome::Disconnected;
        }
    };

    let (ws, _resp) = match tokio_tungstenite::connect_async(request).await {
        Ok(pair) => pair,
        Err(e) => {
            tracing::warn!(err = %e, "ws connect failed");
            return ReadOutcome::Disconnected;
        }
    };

    state.write().on_ws_connected();
    tracing::info!(url = %url, "ws connected");

    let (_sink, mut stream) = ws.split();
    let mut any_frame = false;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::debug!("shutdown requested during read");
                    return ReadOutcome::Shutdown;
                }
            }
            frame = stream.next() => {
                match frame {
                    Some(Ok(tungstenite::Message::Text(text))) => {
                        any_frame = true;
                        dispatch_frame(&text, state, events);
                    }
                    Some(Ok(tungstenite::Message::Binary(bin))) => {
                        any_frame = true;
                        if let Ok(text) = std::str::from_utf8(&bin) {
                            dispatch_frame(text, state, events);
                        }
                    }
                    Some(Ok(tungstenite::Message::Ping(_) | tungstenite::Message::Pong(_))) => {
                        // tungstenite autoresponds to pings; pongs
                        // still bump freshness.
                        any_frame = true;
                        state.write().apply_heartbeat(Utc::now());
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => {
                        tracing::info!("ws closed by peer");
                        state.write().on_ws_disconnected();
                        return if any_frame {
                            ReadOutcome::Connected
                        } else {
                            ReadOutcome::Disconnected
                        };
                    }
                    Some(Ok(tungstenite::Message::Frame(_))) => {
                        // Raw frames are not emitted by the default
                        // tungstenite reader config.
                    }
                    Some(Err(e)) => {
                        tracing::warn!(err = %e, "ws read error");
                        state.write().on_ws_disconnected();
                        return ReadOutcome::Disconnected;
                    }
                }
            }
        }
    }
}

fn build_request(
    url: &url::Url,
    token: Option<&str>,
) -> Result<tungstenite::handshake::client::Request, String> {
    use tungstenite::client::IntoClientRequest as _;

    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| e.to_string())?;

    if let Some(t) = token {
        let value = format!("Bearer {t}")
            .parse::<tungstenite::http::HeaderValue>()
            .map_err(|e| e.to_string())?;
        request.headers_mut().insert("Authorization", value);
    }

    Ok(request)
}

fn dispatch_frame(
    text: &str,
    state: &Arc<RwLock<EngineState>>,
    events: &broadcast::Sender<EngineEvent>,
) {
    let raw: RawEvent = match serde_json::from_str(text) {
        Ok(raw) => raw,
        Err(e) => {
            tracing::debug!(err = %e, preview = %text.chars().take(80).collect::<String>(), "ws decode error");
            return;
        }
    };

    let ts = raw
        .ts
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc));

    let evt = match raw.event.as_str() {
        "heartbeat" => {
            state.write().apply_heartbeat(ts);
            EngineEvent::Heartbeat(ts)
        }
        "status" | "v2_status" => match serde_json::from_value::<V2Status>(raw.data.clone()) {
            Ok(s) => {
                state.write().apply_status(s.clone(), ts, Source::Ws);
                EngineEvent::Status(Box::new(s))
            }
            Err(e) => {
                tracing::debug!(err = %e, "status decode error");
                EngineEvent::Unknown {
                    event: raw.event,
                    ts,
                    data: raw.data,
                }
            }
        },
        "positions" | "positions_update" => {
            match serde_json::from_value::<Positions>(raw.data.clone()) {
                Ok(p) => {
                    state.write().apply_positions(p.clone(), ts, Source::Ws);
                    EngineEvent::Positions(Box::new(p))
                }
                Err(e) => {
                    tracing::debug!(err = %e, "positions decode error");
                    EngineEvent::Unknown {
                        event: raw.event,
                        ts,
                        data: raw.data,
                    }
                }
            }
        }
        "risk" | "risk_update" => match serde_json::from_value::<Risk>(raw.data.clone()) {
            Ok(r) => {
                state.write().apply_risk(r.clone(), ts, Source::Ws);
                EngineEvent::Risk(Box::new(r))
            }
            Err(e) => {
                tracing::debug!(err = %e, "risk decode error");
                EngineEvent::Unknown {
                    event: raw.event,
                    ts,
                    data: raw.data,
                }
            }
        },
        "regime" | "regime_update" => match serde_json::from_value::<Regime>(raw.data.clone()) {
            Ok(r) => {
                state.write().apply_regime(r.clone(), ts, Source::Ws);
                EngineEvent::Regime(Box::new(r))
            }
            Err(e) => {
                tracing::debug!(err = %e, "regime decode error");
                EngineEvent::Unknown {
                    event: raw.event,
                    ts,
                    data: raw.data,
                }
            }
        },
        _ => EngineEvent::Unknown {
            event: raw.event,
            ts,
            data: raw.data,
        },
    };

    // Best-effort send; dropped receivers are fine. The state mirror
    // is the durable copy; broadcast events are a convenience tap.
    let _ = events.send(evt);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pure backoff math ──────────────────────────────────────────

    #[test]
    fn exp_backoff_cap_starts_at_initial_on_attempt_zero() {
        let d = exp_backoff_cap(Duration::from_millis(500), Duration::from_secs(30), 2, 0);
        assert_eq!(d, Duration::from_millis(500));
    }

    #[test]
    fn exp_backoff_cap_doubles_each_attempt_until_max() {
        let initial = Duration::from_millis(500);
        let max = Duration::from_secs(30);
        // 500 → 1000 → 2000 → 4000 → 8000 → 16000 → 30000 (cap)
        let seq: Vec<u128> = (0..8)
            .map(|a| exp_backoff_cap(initial, max, 2, a).as_millis())
            .collect();
        assert_eq!(
            seq,
            vec![500, 1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000]
        );
    }

    #[test]
    fn exp_backoff_cap_saturates_on_runaway_attempt() {
        // A truly pathological attempt count must not panic or
        // overflow; it just pins at max_backoff.
        let d = exp_backoff_cap(
            Duration::from_millis(500),
            Duration::from_secs(30),
            2,
            1_000_000,
        );
        assert_eq!(d, Duration::from_secs(30));
    }

    #[test]
    fn exp_backoff_cap_with_multiplier_one_stays_at_initial() {
        let d = exp_backoff_cap(Duration::from_millis(500), Duration::from_secs(30), 1, 5);
        assert_eq!(d, Duration::from_millis(500));
    }

    // ── Jitter ─────────────────────────────────────────────────────

    #[test]
    fn jitter_none_returns_cap_unchanged() {
        let mut rng = || 0_u64;
        let out = apply_jitter(Duration::from_millis(1_234), JitterMode::None, &mut rng);
        assert_eq!(out, Duration::from_millis(1_234));
    }

    #[test]
    fn jitter_full_is_bounded_by_cap() {
        // 10 000 draws from the real xorshift — every sample must
        // land in [0, cap]. A one-off violation here would break
        // the max_backoff invariant.
        let mut rng = XorshiftRng::seeded_from_now();
        let cap = Duration::from_millis(5_000);
        for _ in 0..10_000 {
            let d = apply_jitter(cap, JitterMode::Full, &mut || rng.next_u64());
            assert!(d <= cap, "jitter produced {d:?} > cap {cap:?}");
        }
    }

    #[test]
    fn jitter_full_varies_across_draws() {
        // Sanity check that jitter actually jitters — if the RNG
        // were constant the sequence would collapse to one value.
        let mut rng = XorshiftRng::seeded_from_now();
        let cap = Duration::from_millis(5_000);
        let samples: Vec<_> = (0..100)
            .map(|_| apply_jitter(cap, JitterMode::Full, &mut || rng.next_u64()))
            .collect();
        let unique: std::collections::BTreeSet<_> = samples.iter().collect();
        assert!(
            unique.len() > 1,
            "expected at least two distinct jitter values, got {}",
            unique.len()
        );
    }

    #[test]
    fn jitter_full_with_zero_cap_returns_zero() {
        let mut rng = || 0xDEAD_BEEF_u64;
        let out = apply_jitter(Duration::ZERO, JitterMode::Full, &mut rng);
        assert_eq!(out, Duration::ZERO);
    }

    // ── xorshift ───────────────────────────────────────────────────

    #[test]
    fn xorshift_is_deterministic_and_non_trivial() {
        let mut a = XorshiftRng { state: 0x1234_5678 };
        let mut b = XorshiftRng { state: 0x1234_5678 };
        let seq_a: Vec<u64> = (0..16).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..16).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b, "same seed must produce same sequence");
        let unique: std::collections::BTreeSet<_> = seq_a.iter().collect();
        assert!(
            unique.len() >= 15,
            "xorshift should not cycle in 16 draws, got {}",
            unique.len()
        );
    }
}
