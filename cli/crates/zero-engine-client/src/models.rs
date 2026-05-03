//! Typed response shapes mirrored from the engine's FastAPI surface.
//!
//! Each type is **narrow on purpose.** We deserialize only the fields
//! the CLI actually renders; extra fields are tolerated via
//! `#[serde(flatten)]` `extra`, so the engine can evolve without
//! breaking us, and missing fields surface as `Option::None`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── /  ────────────────────────────────────────────────────────────

/// Response shape of `GET /` — unauthenticated version probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub name: String,
    pub version: String,
    pub status: String,
    pub ts: Option<String>,
}

// ─── /health  ──────────────────────────────────────────────────────

/// Response shape of `GET /health` — unauthenticated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    /// `"ok"` or `"degraded"`.
    pub status: String,
    #[serde(default)]
    pub components: BTreeMap<String, ComponentHealth>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub circuit_breakers: BTreeMap<String, String>,
    #[serde(default)]
    pub risk: RiskSummary,
    #[serde(default)]
    pub recovery: Option<RecoveryStatus>,
    #[serde(default)]
    pub ws_connections: u64,
}

/// Runtime recovery state emitted by the paper engine after journal replay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoveryStatus {
    pub status: Option<String>,
    pub source: Option<String>,
    pub durable: bool,
    pub journal_path: Option<String>,
    pub decisions_recovered: Option<u32>,
    pub fills_recovered: Option<u32>,
    pub rejections_recovered: Option<u32>,
    pub positions_recovered: Option<u32>,
    pub last_decision_at: Option<String>,
    pub current_decisions: Option<u32>,
    pub current_fills: Option<u32>,
    pub current_rejections: Option<u32>,
    pub current_positions: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /hl/status  ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HyperliquidStatus {
    pub enabled: bool,
    pub exchange: Option<String>,
    pub endpoint: Option<String>,
    pub coins: Option<u32>,
    pub mids: BTreeMap<String, f64>,
    pub secrets_required: Option<bool>,
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /hl/account and /hl/reconcile ────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HyperliquidAccount {
    pub schema_version: String,
    pub exchange: String,
    pub user: String,
    pub as_of: Option<String>,
    pub account_value: Option<f64>,
    pub margin_used: Option<f64>,
    pub withdrawable: Option<f64>,
    pub positions: Vec<HyperliquidAccountPosition>,
    pub open_orders: Vec<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HyperliquidAccountPosition {
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub position_value: f64,
    pub unrealized_pnl: f64,
    pub margin_used: f64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HyperliquidReconciliation {
    pub schema_version: String,
    pub exchange: String,
    pub status: String,
    pub risk_increasing_allowed: bool,
    pub reason: String,
    pub as_of: Option<String>,
    pub drifts: Vec<HyperliquidReconciliationDrift>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HyperliquidReconciliationDrift {
    pub code: String,
    pub severity: String,
    pub symbol: Option<String>,
    pub reason: String,
    pub local_quantity: Option<f64>,
    pub exchange_quantity: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /market/quote  ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MarketQuote {
    pub symbol: String,
    pub price: f64,
    pub source: String,
    pub as_of: Option<String>,
    pub mode: Option<String>,
    pub live: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /live/preflight ───────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LivePreflight {
    pub schema_version: String,
    pub exchange: String,
    pub mode: String,
    pub ready: bool,
    pub live_mode: String,
    pub controls_ready: bool,
    pub checks: Vec<LivePreflightCheck>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LivePreflightCheck {
    pub name: String,
    pub status: String,
    pub note: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /live/certification ──────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCertification {
    pub schema_version: String,
    pub mode: String,
    pub passed: bool,
    pub live_start_certified: bool,
    pub summary: BTreeMap<String, Value>,
    pub drills: Vec<LiveCertificationDrill>,
    pub evidence_requirements: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCertificationDrill {
    pub name: String,
    pub status: String,
    pub note: String,
    pub evidence: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /live/evidence ───────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveEvidence {
    pub schema_version: String,
    pub generated_at: Option<String>,
    pub mode: String,
    pub live_mode: String,
    pub ready: bool,
    pub risk_increasing_allowed: bool,
    pub operator_context: OperatorContext,
    pub summary: BTreeMap<String, Value>,
    pub artifacts: Vec<LiveEvidenceArtifact>,
    pub canary_rule: BTreeMap<String, Value>,
    pub privacy: BTreeMap<String, Value>,
    pub evidence_hash: String,
    pub signature: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveEvidenceArtifact {
    pub name: String,
    pub schema_version: String,
    pub status: String,
    pub hash: String,
    pub included: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /live/cockpit ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCockpit {
    pub schema_version: String,
    pub generated_at: Option<String>,
    pub mode: String,
    pub live_mode: String,
    pub ready: bool,
    pub controls_ready: bool,
    pub risk_increasing_allowed: bool,
    pub next_action: String,
    pub operator_context: OperatorContext,
    pub preflight: LiveCockpitPreflight,
    pub immune: LiveCockpitImmune,
    pub reconciliation: LiveCockpitReconciliation,
    pub certification: LiveCockpitCertification,
    pub heartbeat: LiveCockpitHeartbeat,
    pub live_records: LiveCockpitRecords,
    pub operator_actions: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OperatorContext {
    pub schema_version: Option<String>,
    pub operator_id: String,
    pub handle: String,
    pub role: String,
    pub scope: String,
    pub source: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCockpitPreflight {
    pub schema_version: String,
    pub ready: bool,
    pub live_mode: String,
    pub controls_ready: bool,
    pub summary: BTreeMap<String, Value>,
    pub failed_checks: Vec<LivePreflightCheck>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCockpitImmune {
    pub schema_version: String,
    pub risk_increasing_allowed: bool,
    pub summary: BTreeMap<String, Value>,
    pub open_breakers: Vec<ImmuneBreaker>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCockpitReconciliation {
    pub schema_version: String,
    pub status: String,
    pub risk_increasing_allowed: bool,
    pub reason: String,
    pub drifts: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCockpitCertification {
    pub schema_version: String,
    pub mode: String,
    pub passed: bool,
    pub live_start_certified: bool,
    pub summary: BTreeMap<String, Value>,
    pub failed_drills: Vec<LiveCertificationDrill>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCockpitHeartbeat {
    pub configured: bool,
    pub expired: bool,
    pub last_heartbeat_at: Option<f64>,
    pub timeout_s: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveCockpitRecords {
    pub total: u64,
    pub accepted: u64,
    pub refused: u64,
    pub exchange_error: u64,
    pub recent: Vec<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /immune ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImmuneReport {
    pub schema_version: String,
    pub generated_at: Option<String>,
    pub mode: String,
    pub risk_increasing_allowed: bool,
    pub summary: BTreeMap<String, Value>,
    pub breakers: Vec<ImmuneBreaker>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImmuneBreaker {
    pub name: String,
    pub status: String,
    pub blocks_risk: bool,
    pub severity: String,
    pub reason: String,
    pub evidence: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /live/* control POSTs ─────────────────────────────────────────

/// Response body for live risk-reduction controls.
///
/// The live control endpoints intentionally use a broad response envelope:
/// `/live/kill`, `/live/pause`, `/live/resume`, `/live/heartbeat`, and
/// `/live/flatten` share `ok` / `reason`, while endpoint-specific details
/// such as `exchange_cancel`, `exchange_dead_man`, and flatten `orders` stay
/// in `extra`. This keeps the CLI honest without forcing it to mirror every
/// exchange adapter field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveControlResponse {
    pub ok: bool,
    pub state: Option<String>,
    pub reason: Option<String>,
    pub orders: Vec<Value>,
    pub operator_context: Option<OperatorContext>,
    pub action: Option<String>,
    pub risk_direction: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: String,
    pub last_seen: Option<String>,
    #[serde(default)]
    pub age_s: Option<f64>,
}

impl ComponentHealth {
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.status == "healthy"
    }

    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.status == "dead"
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskSummary {
    pub equity: Option<f64>,
    pub drawdown_pct: Option<f64>,
    #[serde(default)]
    pub kill_all: bool,
}

impl Health {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.status == "ok"
    }

    #[must_use]
    pub fn component_counts(&self) -> ComponentCounts {
        let mut c = ComponentCounts::default();
        for comp in self.components.values() {
            match comp.status.as_str() {
                "healthy" => c.healthy += 1,
                "stale" => c.stale += 1,
                "dead" => c.dead += 1,
                _ => c.unknown += 1,
            }
        }
        c
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ComponentCounts {
    pub healthy: u32,
    pub stale: u32,
    pub dead: u32,
    pub unknown: u32,
}

// ─── /positions  ───────────────────────────────────────────────────

/// A single open position.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Position {
    /// Engine emits `coin` (`TRX`, `BTC`, …); the spec + REST
    /// variants call this `symbol`. Alias covers the WS shape
    /// and the raw `positions.json` bus file.
    #[serde(alias = "coin")]
    pub symbol: String,
    /// `"long"` | `"short"`. Engine `_bus_poller` emits
    /// `direction: "LONG" | "SHORT"`; alias covers it.
    #[serde(alias = "direction")]
    pub side: String,
    /// Engine's raw `positions.json` uses `size_coins` for the
    /// coin-quantity field; REST surfaces `size` directly.
    #[serde(alias = "size_coins")]
    pub size: f64,
    #[serde(alias = "entry_price")]
    pub entry: f64,
    pub mark: Option<f64>,
    pub unrealized_pnl: Option<f64>,
    pub unrealized_r: Option<f64>,
    pub stop: Option<f64>,
    pub target: Option<f64>,
    pub lens_id: Option<String>,
    pub age_s: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `GET /positions` envelope. Engine returns a list, optionally
/// wrapped in `{positions: [...]}` depending on handler version; we
/// accept both.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Positions {
    #[serde(alias = "positions", default)]
    pub items: Vec<Position>,
    #[serde(default)]
    pub account_value: Option<f64>,
    #[serde(default)]
    pub total_unrealized_pnl: Option<f64>,
}

// ─── /risk  ────────────────────────────────────────────────────────

/// `GET /risk` summary. Field names mirror the engine's real wire
/// shape (see `engine/zero/api.py::get_risk` and the `risk.json`
/// fixture captured under `tests/fixtures/`).
///
/// Historical note: older mock fixtures used `daily_loss_pct`,
/// `exposure_pct`, `kill_all`, `circuit_breaker_active`,
/// `concurrent_positions`, and `max_concurrent`. The live engine
/// emits none of those; they were removed to stop the CLI from
/// silently rendering `—` for fields that never existed. Current
/// render code derives percentages from dollar amounts where
/// necessary (see `Risk::daily_loss_pct`, `Risk::drawdown_pct`).
// Four `bool` fields trip `clippy::struct_excessive_bools`. The
// suggested refactor — collapse into a state-machine enum — would
// require the CLI to interpret and re-emit the engine's halt
// classification rather than echo it. That is exactly the kind of
// re-derivation the honesty-bar rejects: the engine's wire shape
// has four distinct halt booleans (`halted`, `global_halt`,
// `stop_failure_halt`, `capital_floor_hit`) because they are
// produced by four independent code paths on the engine side.
// Folding them into a single enum here would force a lossy
// projection at the deserialize boundary and then the CLI would
// have to re-split them everywhere they are rendered (status bar
// halt reason, heat read-out, `/risk` line). The struct mirrors
// the wire; `Risk::is_halted()` / `halt_reason()` provide the
// derived views callers actually want.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Risk {
    pub account_value: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub daily_pnl_usd: Option<f64>,
    pub daily_loss_usd: Option<f64>,
    pub peak_equity: Option<f64>,
    pub peak_equity_30d: Option<f64>,
    pub open_count: Option<u32>,
    pub halted: bool,
    pub global_halt: bool,
    pub stop_failure_halt: bool,
    pub capital_floor_hit: bool,
    pub halt_reason: Option<String>,
    pub halt_until: Option<String>,
    pub updated_at: Option<String>,
    pub daily_loss_since: Option<String>,
    pub last_drawdown_alert_pct: Option<f64>,
    pub per_runner: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Risk {
    /// True when the engine has stopped accepting new risk, for any
    /// reason the wire format exposes. The engine currently sets a
    /// single `halted` bool plus a pair of more-specific flags; any
    /// of them should land the operator on an alert line.
    #[must_use]
    pub fn is_halted(&self) -> bool {
        self.halted || self.global_halt || self.stop_failure_halt
    }

    /// Daily loss as a percent of peak equity, derived from the
    /// two dollar fields the engine publishes. Returns `None` if
    /// peak equity is missing or zero so callers can render `—`
    /// rather than a bogus zero.
    #[must_use]
    pub fn daily_loss_pct(&self) -> Option<f64> {
        let loss = self.daily_loss_usd?;
        let peak = self.peak_equity.or(self.peak_equity_30d)?;
        if peak <= 0.0 {
            return None;
        }
        Some((loss / peak) * 100.0)
    }
}

// ─── /regime  ──────────────────────────────────────────────────────

/// `GET /regime` — either per-coin or whole-market depending on
/// query. The engine returns a loose shape with `regime`, `confidence`,
/// and auxiliary fields; we capture the core and flatten the rest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Regime {
    /// `"TREND_LONG"` | `"TREND_SHORT"` | `"CHOP"` | `"VOL_EXPAND"` | ...
    pub regime: Option<String>,
    pub confidence: Option<f64>,
    pub coin: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /brief  ───────────────────────────────────────────────────────

/// `GET /brief` — the engine's situational readout. Shape matches the
/// real wire payload (see `tests/fixtures/brief.json`): a timestamp, a
/// fear-greed reading, open positions + their snapshots, recent
/// signals, coins approaching a gate, and the last macro cycle
/// summary. The CLI renders a concise header line and optionally
/// expands into the lists.
///
/// Historical note: earlier struct advertised `headline`/`summary`
/// fields. The engine never sent them; the CLI always rendered
/// "(engine has no briefing right now)". Those two fields are gone.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Brief {
    pub timestamp: Option<String>,
    pub fear_greed: Option<i64>,
    pub open_positions: Option<u32>,
    pub positions: Vec<Position>,
    pub recent_signals: Vec<Value>,
    pub approaching: Vec<Value>,
    pub last_cycle: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Brief {
    /// Best-effort "is there anything to tell the operator?" signal.
    /// Returns true when any of the narrative lists have at least one
    /// item or a fear-greed reading is available. A fully-empty brief
    /// deserves the honest "nothing right now" line; anything else
    /// should render the data the engine actually sent.
    #[must_use]
    pub fn has_content(&self) -> bool {
        self.fear_greed.is_some()
            || self.open_positions.is_some_and(|n| n > 0)
            || !self.positions.is_empty()
            || !self.recent_signals.is_empty()
            || !self.approaching.is_empty()
            || !self.last_cycle.is_null()
                && self.last_cycle.as_object().is_some_and(|o| !o.is_empty())
    }
}

// ─── /evaluate/{coin}  ─────────────────────────────────────────────

/// One layer in an `/evaluate/{coin}` response. The engine emits a
/// numbered list; each entry includes whether the layer passed, an
/// arbitrary `value` payload (scalar or nested object), and a
/// human-readable `detail` string that already summarizes the
/// layer's decision.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EvaluationLayer {
    pub layer: String,
    pub passed: bool,
    pub value: Value,
    pub detail: String,
}

/// `GET /evaluate/{coin}` — the gate-level verdict for a single
/// coin. The real engine returns a flat object with per-layer
/// decisions (see `tests/fixtures/evaluate_sol.json`); this struct
/// mirrors that exactly.
///
/// Legacy fields (`verdict`, `rationale`, `gates`, `as_of`) were
/// removed — they were artifacts of a mock that never matched the
/// engine. Call sites should read the real fields below and derive
/// a verdict from `direction` + `conviction` when they need a
/// single-word summary.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Evaluation {
    pub coin: Option<String>,
    pub price: Option<f64>,
    pub consensus: Option<i64>,
    pub conviction: Option<f64>,
    /// `"LONG"` | `"SHORT"` | `"NONE"`.
    pub direction: Option<String>,
    pub regime: Option<String>,
    pub layers: Vec<EvaluationLayer>,
    pub data_fresh: Option<bool>,
    pub timestamp: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Evaluation {
    /// Human-readable verdict derived from the real fields. Matches
    /// the legacy `verdict` string values used by existing render
    /// code: `"PASS"` when direction is actionable, `"HOLD"` when
    /// every layer passed but direction is `"NONE"`, and
    /// `"REJECT"` when any layer failed.
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        if self.layers.iter().any(|l| !l.passed) {
            "REJECT"
        } else if self
            .direction
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case("LONG") || d.eq_ignore_ascii_case("SHORT"))
        {
            "PASS"
        } else {
            "HOLD"
        }
    }
}

// ─── /pulse  ───────────────────────────────────────────────────────

/// One entry in the engine's pulse stream. Events are semi-free-form;
/// the client only consumes `kind`, `coin`, `message`, `ts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PulseEvent {
    pub kind: Option<String>,
    pub coin: Option<String>,
    pub message: Option<String>,
    pub ts: Option<String>,
    pub severity: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `GET /pulse` envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pulse {
    #[serde(alias = "pulse", alias = "events", default)]
    pub items: Vec<PulseEvent>,
}

// ─── /approaching  ─────────────────────────────────────────────────

/// A coin approaching an entry gate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Approaching {
    pub coin: String,
    pub direction: Option<String>,
    pub distance_to_gate: Option<f64>,
    pub gate: Option<String>,
    pub ts: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `GET /approaching` envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApproachingFeed {
    #[serde(alias = "approaching", alias = "items", default)]
    pub items: Vec<Approaching>,
}

// ─── /rejections  ──────────────────────────────────────────────────

/// A single rejection record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rejection {
    pub coin: Option<String>,
    pub direction: Option<String>,
    pub stage: Option<String>,
    pub reason: Option<String>,
    pub ts: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `GET /rejections` envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RejectionsFeed {
    #[serde(alias = "rejections", alias = "items", default)]
    pub items: Vec<Rejection>,
}

// ─── /v2/status  ───────────────────────────────────────────────────

/// Engine confidence sub-object on `/v2/status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct V2Confidence {
    /// 0..=100 integer score.
    pub score: Option<f64>,
    /// `"low"` | `"medium"` | `"high"` | ...
    pub level: Option<String>,
}

/// Market sub-object on `/v2/status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct V2Market {
    pub regime: Option<String>,
    pub health: Option<f64>,
    pub signal: Option<String>,
    pub prediction: Option<String>,
    pub fear_greed: Option<i64>,
    pub coins_tradeable: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Positions sub-object on `/v2/status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct V2Positions {
    pub open: Option<u32>,
    pub unrealized_pnl: Option<f64>,
    pub equity: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Today-summary sub-object on `/v2/status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct V2Today {
    pub trades: Option<u32>,
    pub wins: Option<u32>,
    pub pnl: Option<f64>,
    pub streak: Option<i32>,
    pub sizing_mult: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// `GET /v2/status` — the condensed engine summary used by the
/// status bar. Shape mirrors the live wire format (see
/// `tests/fixtures/v2_status.json`): a nested object with
/// confidence/market/positions/today sub-objects, plus two list
/// fields the engine uses for signals and blind spots.
///
/// Historical note: the previous CLI model declared flat fields
/// (`engine_confidence`, `regime`, `equity`, `drawdown_pct`) at the
/// top level. The engine never emitted those names; every
/// `Option<…>` deserialized to `None`, so the status bar and
/// `/status` command always rendered em-dashes. Accessors below
/// (`regime()`, `engine_confidence()`, `equity()`, etc.) preserve
/// the original call-site ergonomics while reading the real shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct V2Status {
    pub confidence: V2Confidence,
    pub market: V2Market,
    pub positions: V2Positions,
    pub today: V2Today,
    pub approaching: Vec<Value>,
    pub blind_spots: Vec<Value>,
    pub alert: Option<Value>,
    pub recovery: Option<RecoveryStatus>,
    pub ts: Option<String>,
    /// Hyperliquid per-minute API rate the engine is seeing, as
    /// reported alongside `/v2/status`. `None` when the engine
    /// has not yet surfaced the field — the TUI renders `hl:?`
    /// in metadata color (same honest-rendering rule as `ops:?`
    /// before the classifier reports). Once the engine-side cut
    /// lands (a separate track from M2_PLAN §2), the field
    /// populates and the segment starts showing `hl:N/M`.
    ///
    /// Serde is tolerant here: the field is optional on the
    /// wire, so older engines deserializing into a newer CLI
    /// keep rendering `hl:?` without a decode error.
    pub hl_rate: Option<HlRate>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Hyperliquid API rate snapshot, optionally reported by the
/// engine on `/v2/status`.
///
/// `used` is the number of requests counted against the rolling
/// one-minute window; `cap` is the engine's own per-operator cap
/// (today 240/min — see `engine/zero/shared/http.py::_HL_GLOBAL_MAX`).
/// The widget renders `hl:<used>/<cap>` with the same tri-color
/// thresholds as the CLI-side rate bucket, so an operator reads
/// CLI-side pressure and Hyperliquid-side pressure from two
/// visually consistent segments.
///
/// The engine is the source of truth for both numbers — the CLI
/// never computes them locally.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HlRate {
    pub used: u32,
    pub cap: u32,
}

impl V2Status {
    /// Market regime text (e.g. `"SHORT MARKET. 6 of 7 coins …"`).
    #[must_use]
    pub fn regime(&self) -> Option<&str> {
        self.market.regime.as_deref()
    }

    /// Engine confidence as a 0..=100 score. Historically the CLI
    /// expected a 0..=1 float; the live engine reports a 0..=100
    /// integer, so renderers that format with `{:.2}` need to
    /// switch to `{:.0}` or display it as a score instead of a
    /// probability.
    #[must_use]
    pub fn engine_confidence(&self) -> Option<f64> {
        self.confidence.score
    }

    /// Qualitative confidence level (`"low" | "medium" | "high"`).
    #[must_use]
    pub fn confidence_level(&self) -> Option<&str> {
        self.confidence.level.as_deref()
    }

    /// Current account equity.
    #[must_use]
    pub fn equity(&self) -> Option<f64> {
        self.positions.equity
    }

    /// Count of open positions.
    #[must_use]
    pub fn open(&self) -> Option<u32> {
        self.positions.open
    }

    /// Aggregate unrealized PnL across open positions.
    #[must_use]
    pub fn unrealized_pnl(&self) -> Option<f64> {
        self.positions.unrealized_pnl
    }

    /// `/v2/status` itself does not surface drawdown — the engine
    /// moved that to `/risk`. Kept as an accessor for call-site
    /// parity; always returns `None`.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn drawdown_pct(&self) -> Option<f64> {
        None
    }
}

/// Response shape for `POST /operator/events` (ADR-016).
///
/// `accepted` — the number of events the engine appended to its
/// classifier log (matches the number sent on success; on batch
/// rejection the engine returns 400 so this value is never partial).
///
/// `snapshot` — the post-ingest classifier snapshot. Returned so the
/// caller can reflect any label / friction / state-vector change
/// without a follow-up `GET /operator/state`; saves a round-trip and
/// guarantees the snapshot the caller acts on is the one the engine
/// computed *after* the event landed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperatorEventsAccepted {
    pub accepted: u32,
    pub snapshot: zero_operator_state::Snapshot,
}

// ─── /execute (POST) ───────────────────────────────────────────────
//
// M2_PLAN §7 pins this as the first live-trade surface the CLI can
// actually speak to. The request carries a coin, a side, a size,
// and an **idempotency key** the client mints per `/execute`
// invocation; the engine deduplicates by that key within a short
// window so a CLI retry (we don't auto-retry `/execute`, but an
// operator hitting `↑ Enter` after a timeout will) cannot place a
// second fill. The key is serialized into the body *and* mirrored
// into an `X-Idempotency-Key` HTTP header so engine-side proxies
// that log headers but redact bodies still see the dedupe key.
//
// `Side` is `"buy" | "sell"` on the wire. We expose a small enum
// rather than a `String` so a future `"reduce_only"` addition
// lands as an explicit parse failure on the CLI side (a typed
// refusal the operator can see) rather than as a silent mis-tag.

/// Direction of an `/execute` request. Wire format is the lowercase
/// variant name via `serde(rename_all = "lowercase")`; see
/// [`Self::as_wire`] for a stable string helper used by the doctor
/// row + the `(paper)` suffix renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecuteSide {
    Buy,
    Sell,
}

impl ExecuteSide {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

/// Request body for `POST /execute`.
///
/// `size` is the notional **base-asset** quantity (coins, not USD);
/// the engine resolves the USD notional against the current mid so
/// the CLI does not have to round-trip mark data to place an order.
/// A future `size_usd: Option<f64>` column will land as an additive
/// field — the `#[serde(default)]` + narrow deserialization means
/// older engines tolerate extra fields and older CLIs tolerate
/// missing fields.
///
/// `idempotency_key` is required. The typed helper
/// [`crate::HttpClient::post_execute`] mints one per call via
/// `uuid::Uuid::new_v4` so callers cannot forget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub coin: String,
    pub side: ExecuteSide,
    pub size: f64,
    pub idempotency_key: String,
}

/// Response body for `POST /execute`.
///
/// `accepted` is the engine's tri-state: the order was composed and
/// sent to the exchange (or the paper adapter). `simulated` is the
/// paper-mode discriminator — engine truth, not a local guess. The
/// CLI suffixes the operator-visible line with `(paper)` whenever
/// this field is `true`, so the operator can never be fooled into
/// thinking a paper fill was a live fill.
///
/// `fill_id` is `None` until the exchange returns an ack; for paper
/// fills the engine synthesizes a deterministic string so the CLI
/// still has something to grep.
///
/// Extra fields land in `extra` verbatim; the engine is free to
/// add `slippage_bps`, `fee_bps`, etc. without breaking the CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub accepted: bool,
    #[serde(default)]
    pub simulated: bool,
    #[serde(default)]
    pub fill_id: Option<String>,
    #[serde(default)]
    pub coin: Option<String>,
    #[serde(default)]
    pub side: Option<ExecuteSide>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ─── /auto/toggle (POST) ───────────────────────────────────────────
//
// Flips the engine's Auto-mode flag. Request carries the desired
// state; response echoes the engine's **new** state (not the
// requested state — if friction refused the flip the operator sees
// `state: off` + an explanation, rather than a silent no-op with a
// mis-optimistic local UI).

/// Request body for `POST /auto/toggle`. `enabled = true` asks the
/// engine to enter Auto-mode (Plan-mode auto-accept); `false` asks
/// it to fall back to operator-confirm. The engine may refuse; read
/// the reply's `state` to see what actually landed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoToggleRequest {
    pub enabled: bool,
}

/// Response body for `POST /auto/toggle`.
///
/// `state` is the engine's post-call Auto-mode state. `simulated`
/// is the paper-mode discriminator — in paper mode the flip is a
/// bookkeeping change but the downstream fills stay simulated, so
/// the CLI tags the operator-visible confirmation with `(paper)`
/// to keep that distinction loud.
///
/// `reason` carries an engine-provided explanation on refusal
/// (e.g. "operator state is TILT"); `None` on the happy path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoToggleResponse {
    pub state: AutoState,
    #[serde(default)]
    pub simulated: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Wire representation of the engine's Auto-mode state, mirrored
/// from `zero_commands::AutoState` but narrow on purpose: the
/// engine client speaks in its own vocabulary so a dispatcher
/// refactor on the CLI side does not reshape the HTTP surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoState {
    On,
    Off,
}

impl AutoState {
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

#[cfg(test)]
mod wire_compat_tests {
    //! Tests that pin the deserialization of real engine bus-
    //! file shapes. Regression test captured after Session 10
    //! debugging found that `positions.json` uses
    //! `coin`/`direction`/`entry_price`/`size_coins` instead of
    //! the spec'd `symbol`/`side`/`entry`/`size`. If the engine
    //! later unifies on one shape, the aliases stay harmless.
    use super::*;

    #[test]
    fn positions_bus_file_shape_parses_with_aliases() {
        // Trimmed from a live `positions.json` as of 2026-04-22.
        let raw = r#"{
            "updated_at": "2026-04-22T12:19:29.563466+00:00",
            "positions": [
                {
                    "coin": "TRX",
                    "direction": "LONG",
                    "entry_price": 0.33444,
                    "size_coins": 149.0,
                    "size_usd": 49.83156,
                    "stop_loss_pct": 0.025,
                    "id": "TRX_LONG_1776857828",
                    "strategy": "production",
                    "lens_id": "lens_flow"
                },
                {
                    "coin": "BTC",
                    "direction": "SHORT",
                    "entry_price": 63450.0,
                    "size_coins": 0.0012,
                    "size_usd": 76.14
                }
            ]
        }"#;
        let parsed: Positions = serde_json::from_str(raw).expect("engine shape must parse");
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].symbol, "TRX");
        assert_eq!(parsed.items[0].side, "LONG");
        assert!((parsed.items[0].size - 149.0).abs() < f64::EPSILON);
        assert!((parsed.items[0].entry - 0.33444).abs() < f64::EPSILON);
        assert_eq!(parsed.items[0].lens_id.as_deref(), Some("lens_flow"));
        // Extra engine-only fields land in `extra` via the
        // `#[serde(flatten)] extra` catch-all, so nothing gets
        // silently dropped.
        assert!(parsed.items[0].extra.contains_key("size_usd"));
    }

    #[test]
    fn risk_bus_file_shape_parses() {
        let raw = r#"{
            "account_value": 581.49647,
            "updated_at": "2026-04-22T12:19:29.564814+00:00",
            "daily_pnl_usd": 0.0,
            "daily_loss_usd": 0.0,
            "global_halt": false,
            "halted": false,
            "drawdown_pct": 9.24,
            "peak_equity": 613.450419,
            "peak_equity_30d": 640.7,
            "open_count": 2
        }"#;
        let parsed: Risk = serde_json::from_str(raw).expect("risk shape must parse");
        assert_eq!(parsed.account_value, Some(581.49647));
        assert_eq!(parsed.drawdown_pct, Some(9.24));
        assert_eq!(parsed.open_count, Some(2));
        assert!(!parsed.halted);
    }
}
