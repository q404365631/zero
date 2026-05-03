//! `EngineState` — a CLI-side mirror of live engine state.
//!
//! Populated by the WS subscriber (push) and opportunistically by
//! HTTP polls (pull). Every field that the TUI renders is a
//! `Stat<T>`, so staleness is visible and the renderer can refuse
//! to display values that age out.
//!
//! The mirror is read by widgets via a cheap clone of its `Arc`,
//! and it is updated only from within the subscriber's task. This
//! keeps the lock scope small and lets ratatui's render loop
//! acquire `read()` handles without contention.
//!
//! See ADR-003 (state model) and spec §3.5 ("engine is the source
//! of truth").

use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use zero_operator_state::Snapshot as OperatorSnapshot;

use crate::models::{LiveCockpit, Positions, Regime, Risk, V2Status};
use crate::stat::{Source, Stat};

/// A connection health roll-up for the status bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectionHealth {
    /// Whether the WS is currently up.
    pub ws_connected: bool,
    /// Failed-connect attempts since the last successful connect.
    /// Drives the "reconnecting (attempt N)…" status-bar hint and
    /// resets to zero the moment the socket is up again.
    pub reconnect_count: u32,
    /// Lifetime reconnect attempts. Never resets. Used by
    /// observability tooling (and tests) that need to see that a
    /// reconnect happened even after the subscriber recovered.
    pub total_attempts: u64,
    /// When the most recent reconnect attempt began. Useful for
    /// rendering "reconnecting in 2s…" hints.
    pub last_reconnect_at: Option<DateTime<Utc>>,
}

/// Live mirror of the engine fields the CLI renders.
///
/// Every field is an `Option<Stat<T>>` — `None` means "we have not
/// seen this yet," not "the engine doesn't have one." The renderer
/// distinguishes these states: unseen → placeholder, stale → amber
/// staleness badge, fresh → normal render.
#[derive(Debug, Default, Clone)]
pub struct EngineState {
    pub status: Option<Stat<V2Status>>,
    pub positions: Option<Stat<Positions>>,
    pub risk: Option<Stat<Risk>>,
    pub regime: Option<Stat<Regime>>,
    /// Consolidated live-readiness cockpit from `GET /live/cockpit`.
    /// This is read-only operator state: preflight, immune,
    /// reconciliation, certification, heartbeat, and local receipt
    /// counts. It intentionally lives in the same mirror as status,
    /// positions, risk, and regime so the full-screen TUI cockpit can
    /// render without issuing network calls from the draw path.
    pub live_cockpit: Option<Stat<LiveCockpit>>,
    /// Operator behavioral state snapshot mirrored from the engine's
    /// `GET /operator/state` endpoint (ADR-016). The classifier runs
    /// on the engine host; the CLI only renders. `None` means "never
    /// observed" — the status bar falls back to `?`.
    pub operator_state: Option<Stat<OperatorSnapshot>>,
    /// Most recent heartbeat timestamp from the engine's bus poller.
    /// Drives the freshness clock — if this stops advancing, the
    /// status bar goes amber then red per `feed` thresholds.
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub connection: ConnectionHealth,
}

impl EngineState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an `Arc<RwLock<Self>>` ready for sharing between the
    /// WS subscriber task and the TUI render loop.
    #[must_use]
    pub fn shared() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self::new()))
    }

    /// Freshness of the feed, in seconds, measured against `now`.
    /// Returns `None` if no heartbeat has been observed.
    #[must_use]
    pub fn feed_age_seconds(&self, now: DateTime<Utc>) -> Option<i64> {
        self.last_heartbeat
            .map(|hb| now.signed_duration_since(hb).num_seconds())
    }

    /// Hyperliquid per-minute rate the engine is reporting, if any.
    ///
    /// Returns `None` when `/v2/status` has not been observed yet
    /// or when the observed payload did not carry an `hl_rate`
    /// field (older engine). The caller renders `hl:?` in both
    /// cases — from the operator's seat, "we haven't heard"
    /// and "the engine is not telling" are indistinguishable, so
    /// the same metadata-color placeholder is the honest render.
    #[must_use]
    pub fn hl_rate_snapshot(&self) -> Option<crate::models::HlRate> {
        self.status.as_ref().and_then(|s| s.value.hl_rate)
    }

    /// Record that a WS connection was established. Resets the
    /// reconnect counter to zero — the one we just made is behind us.
    pub fn on_ws_connected(&mut self) {
        self.connection.ws_connected = true;
        self.connection.reconnect_count = 0;
    }

    /// Record that the WS dropped. Does not reset `reconnect_count`
    /// — that ticks up on each attempt until one succeeds.
    pub fn on_ws_disconnected(&mut self) {
        self.connection.ws_connected = false;
    }

    /// Record the start of a new reconnect attempt.
    pub fn on_reconnect_attempt(&mut self, at: DateTime<Utc>) {
        self.connection.reconnect_count = self.connection.reconnect_count.saturating_add(1);
        self.connection.total_attempts = self.connection.total_attempts.saturating_add(1);
        self.connection.last_reconnect_at = Some(at);
    }

    /// Merge a status event into the mirror. `source` distinguishes
    /// push updates (WS) from pull backfill (HTTP). The status bar
    /// does not currently branch on `Source`, but the persisted
    /// `Stat<T>` carries it so observability tooling + future
    /// lints can tell which surface wrote each value.
    ///
    /// `last_heartbeat` is only bumped on WS updates — HTTP
    /// backfill is opportunistic and must not paper over a stalled
    /// bus feed.
    pub fn apply_status(&mut self, status: V2Status, as_of: DateTime<Utc>, source: Source) {
        self.status = Some(Stat::new(status, source).with_as_of(as_of));
        if matches!(source, Source::Ws) {
            self.last_heartbeat = Some(as_of);
        }
    }

    /// Merge a positions event into the mirror. See
    /// [`Self::apply_status`] for the source + heartbeat contract.
    pub fn apply_positions(&mut self, positions: Positions, as_of: DateTime<Utc>, source: Source) {
        self.positions = Some(Stat::new(positions, source).with_as_of(as_of));
        if matches!(source, Source::Ws) {
            self.last_heartbeat = Some(as_of);
        }
    }

    /// Merge a risk event into the mirror. See
    /// [`Self::apply_status`] for the source + heartbeat contract.
    pub fn apply_risk(&mut self, risk: Risk, as_of: DateTime<Utc>, source: Source) {
        self.risk = Some(Stat::new(risk, source).with_as_of(as_of));
        if matches!(source, Source::Ws) {
            self.last_heartbeat = Some(as_of);
        }
    }

    /// Merge a regime event into the mirror. See
    /// [`Self::apply_status`] for the source + heartbeat contract.
    pub fn apply_regime(&mut self, regime: Regime, as_of: DateTime<Utc>, source: Source) {
        self.regime = Some(Stat::new(regime, source).with_as_of(as_of));
        if matches!(source, Source::Ws) {
            self.last_heartbeat = Some(as_of);
        }
    }

    /// Merge a live-cockpit packet fetched from the engine.
    ///
    /// Cockpit is an HTTP-only surface today and must not bump
    /// `last_heartbeat`; a healthy cockpit response does not prove
    /// the market/event feed is alive.
    pub fn apply_live_cockpit(&mut self, cockpit: LiveCockpit, as_of: DateTime<Utc>) {
        self.live_cockpit = Some(Stat::new(cockpit, Source::Http).with_as_of(as_of));
    }

    /// Record a heartbeat with no payload. Bumps the freshness clock
    /// so the status-bar feed indicator stays green even during
    /// quiet market periods.
    pub fn apply_heartbeat(&mut self, at: DateTime<Utc>) {
        self.last_heartbeat = Some(at);
    }

    /// Merge an operator-state snapshot fetched from the engine.
    ///
    /// The snapshot carries its own `as_of` but we also wrap it in a
    /// [`Stat`] so the staleness clock stays uniform across every
    /// mirror field. Note: this does **not** touch `last_heartbeat`
    /// — operator-state lives on a slower poll cadence than the bus
    /// feed, and we do not want it masking a stalled market feed.
    pub fn apply_operator_state(&mut self, snap: OperatorSnapshot, as_of: DateTime<Utc>) {
        self.operator_state = Some(Stat::new(snap, Source::Http).with_as_of(as_of));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_counter_ticks_and_resets() {
        let mut s = EngineState::new();
        assert_eq!(s.connection.reconnect_count, 0);
        assert_eq!(s.connection.total_attempts, 0);
        assert!(!s.connection.ws_connected);

        s.on_reconnect_attempt(Utc::now());
        s.on_reconnect_attempt(Utc::now());
        assert_eq!(s.connection.reconnect_count, 2);
        assert_eq!(s.connection.total_attempts, 2);
        assert!(!s.connection.ws_connected);

        s.on_ws_connected();
        assert_eq!(s.connection.reconnect_count, 0);
        assert_eq!(
            s.connection.total_attempts, 2,
            "total_attempts must survive successful reconnect"
        );
        assert!(s.connection.ws_connected);

        s.on_ws_disconnected();
        assert!(!s.connection.ws_connected);
    }

    #[test]
    fn heartbeat_updates_feed_age() {
        let mut s = EngineState::new();
        let t0 = Utc::now();
        s.apply_heartbeat(t0);
        let later = t0 + chrono::Duration::seconds(3);
        assert_eq!(s.feed_age_seconds(later), Some(3));
    }
}
