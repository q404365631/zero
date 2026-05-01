//! Axum-based mock of the engine's FastAPI surface.
//!
//! Mirrors the JSON shapes emitted by `engine/zero/server.py` for the
//! endpoints the CLI actually calls. Missing endpoints return 404 so
//! tests fail loud when a new call is added without a mock.
//!
//! Usage in tests:
//!
//! ```no_run
//! # use zero_testkit::mock_engine::MockEngine;
//! # async fn run() -> anyhow::Result<()> {
//! let mock = MockEngine::spawn().await?;
//! let base = mock.base_url();
//! // … construct an HttpClient against `base` and exercise it …
//! mock.shutdown().await;
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use parking_lot::Mutex;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Overrides the test can inject to simulate engine states.
#[allow(clippy::struct_excessive_bools)] // flags are independent
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    /// Force `/health` status to `"degraded"` with a custom component.
    pub degrade_health: bool,
    /// Return 503 on `/health` (simulate overloaded engine).
    pub health_503: bool,
    /// Return 401 on every typed `GET` endpoint other than `/` and
    /// `/health`. Exercises [`HttpClient`]'s auth-error mapping
    /// path (`HttpError::Unauthorized`). The version probe and
    /// health surface stay open because the CLI uses them during
    /// doctor runs before auth is wired.
    pub force_unauthorized: bool,
    /// Return 404 on every typed `GET` endpoint (same scope as
    /// [`Self::force_unauthorized`]). Tests [`HttpError::NotFound`]
    /// mapping and the client's missing-endpoint log behavior.
    pub force_not_found: bool,
    /// Return 500 on every typed `GET` endpoint. Non-retryable —
    /// asserts that the client does **not** double-call on a
    /// server error that isn't 502/503/504.
    pub force_server_error: bool,
    /// Return a degenerate-but-valid 200 on `/evaluate/{coin}`
    /// — the JSON decodes, but `layers` is empty and `direction`
    /// is absent. Exercises the dispatcher's empty-verdict guard
    /// (`evaluate_cmd` must emit an alert + dismiss stale overlays
    /// instead of opening an empty verdict card). Does not affect
    /// any other endpoint.
    pub force_empty_evaluation: bool,
    /// Return `{}` (HTTP 200) on `/regime`. Matches a real
    /// production failure mode where the engine exposes the
    /// endpoint but never populates a payload. The dispatcher
    /// must alert the operator instead of rendering a row of
    /// em-dashes that looks like data.
    pub force_empty_regime: bool,
    /// Return `{"error": "<msg>"}` (HTTP 200) on `/regime`. Matches
    /// the engine's "coin not found" envelope on `?coin=...`.
    /// The dispatcher must surface the embedded error as an alert.
    pub force_regime_error_envelope: bool,
    /// Return 404 on `/approaching`. Matches older engine builds
    /// that predate the endpoint. The dispatcher must detect the
    /// `NotFound` and emit an explanatory alert instead of the
    /// raw `"not found: /approaching"` error-display.
    pub force_approaching_not_found: bool,
    /// Return a `/risk` payload where `account_value > peak_equity`
    /// (mathematically impossible by definition — peak is monotonic
    /// max of equity). Mirrors a real production drift where the
    /// engine kept writing `risk.json` with a stale equity number
    /// that no live code path refreshed while the portfolio snapshot
    /// was fresh. The dispatcher must surface the contradiction
    /// instead of rendering a confident (but wrong) drawdown percent.
    pub force_stale_risk_equity: bool,
    /// How many further requests should respond with a transient
    /// 503 before the real handler runs. Decremented atomically
    /// per matched request. Used to verify the retry-once policy
    /// (`HttpClient::get_json`) recovers when the first attempt
    /// fails with 503 and the second succeeds. Setting this to
    /// `>= 2` lets the test observe the retry limit: after one
    /// retry, the second failure surfaces as `HttpError::Status`.
    pub transient_fail_count: u32,
    /// How many further requests should respond with a 429 Too Many
    /// Requests before the real handler runs. Sister field to
    /// [`Self::transient_fail_count`]; separated so a test can
    /// pin engine-429 behavior without also exercising the
    /// transient-retry code path.
    ///
    /// When this fires, the response body is empty and the
    /// `Retry-After` header carries [`Self::rate_limit_retry_after`]
    /// (or a sensible default when unset). The CLI's `HttpClient`
    /// parses the header, refunds the local bucket, and surfaces
    /// `HttpError::RateBudgetExhausted { origin: Engine429, .. }`.
    pub rate_limit_count: u32,
    /// Value placed in the `Retry-After` header on every injected
    /// 429. Accepts any string the real engine might emit — plain
    /// integer seconds (`"30"`) or an RFC-7231 IMF-fixdate
    /// (`"Fri, 31 Dec 1999 23:59:59 GMT"`). When `None` (default),
    /// the header carries `"1"` so tests inspecting the client's
    /// parsed duration see an unambiguous 1 s rather than having
    /// to reason about a missing header's fallback.
    pub rate_limit_retry_after: Option<String>,
    /// Custom version string for `GET /`.
    pub version: Option<String>,
    /// Cause the `/ws` handler to immediately close the connection
    /// after accepting the upgrade, exercising the subscriber's
    /// reconnect path. Resets to `false` automatically after one
    /// drop so a test can: set → wait for drop → unset → verify
    /// reconnect succeeds.
    pub ws_drop_once: bool,
    /// Operator-state label the `/operator/state` endpoint reports.
    /// Defaults to `"steady"` when unset. Valid values match
    /// `zero_operator_state::Label` snake-case: `fresh`, `steady`,
    /// `elevated`, `tilt`, `fatigued`, `recovery`.
    pub operator_label: Option<String>,
    /// Monotonic version bumped on each change to `operator_label`
    /// in tests that want to exercise the widget's version-skip
    /// logic. Auto-increments when tests flip the label via
    /// `with_overrides`.
    pub operator_version: u64,
    /// Engine-side `/auto/toggle` state the mock echoes back to the
    /// next `POST /auto/toggle` caller. Tests asserting the "engine
    /// refuses the flip" path pre-set this to a value that differs
    /// from the request, so the response `state` reflects the
    /// engine's truth rather than the caller's wish. `None` means
    /// "mirror the request" (happy path).
    pub auto_toggle_echo_state: Option<bool>,
    /// Optional `reason` string the mock returns alongside
    /// `auto_toggle_echo_state` — used to pin the refusal path
    /// (e.g. `"operator state is TILT"`). Emitted verbatim.
    pub auto_toggle_reason: Option<String>,
    /// When set, every `POST /execute` and `POST /auto/toggle`
    /// response carries `"simulated": true` regardless of the
    /// `X-Zero-Mode` header the client sent. Used by tests that
    /// want to assert the client surfaces the engine's truth
    /// rather than locally inferring paper from its own `--paper`
    /// flag. In production the engine flips this based on the
    /// inbound header; the mock lets tests drive either path.
    pub force_simulated: bool,
    /// When set, `POST /execute` and `POST /auto/toggle` return
    /// a single 503. Verifies that no silent retry fires on the
    /// no-retry POST surface — the caller sees exactly one
    /// upstream request with a typed `HttpError::Status` back.
    pub post_transient_fail: bool,
    /// When set, `POST /execute` returns 500. Same intent as
    /// [`Self::post_transient_fail`] but for a non-retryable
    /// status; belt-and-suspenders against any future change to
    /// `is_retryable` accidentally catching the 500 family.
    pub post_server_error: bool,
}

/// Shared state for the mock axum app.
#[derive(Debug, Clone)]
pub struct AppState {
    pub overrides: Arc<Mutex<Overrides>>,
    /// Every body the mock has received on `POST /operator/events`,
    /// captured as the raw decoded JSON in arrival order. Tests that
    /// exercise the `/rate`, `/break`, etc. rewires assert on this
    /// to confirm the typed-event serialization actually reached the
    /// wire. Kept as `serde_json::Value` rather than the typed
    /// `zero_operator_state::Event` so the mock does not pre-validate
    /// — the engine's real behavior (400 on bad shapes) is already
    /// covered by the Python-side integration test.
    pub received_events: Arc<Mutex<Vec<serde_json::Value>>>,
    /// Every `(headers-snapshot, body)` pair the mock has received on
    /// `POST /execute`, in arrival order. Tests assert on the headers
    /// (especially `x-zero-mode` and `x-idempotency-key`) and the
    /// body shape (typed `ExecuteRequest` round-trip via `serde_json`).
    /// Captured as `(BTreeMap<String, String>, Value)` so both the
    /// test and the stored data are trivially cloneable / printable
    /// when an assertion fails.
    pub received_executes: Arc<Mutex<Vec<CapturedPost>>>,
    /// Every `(headers-snapshot, body)` pair the mock has received on
    /// `POST /auto/toggle`. Same shape as [`Self::received_executes`].
    pub received_auto_toggles: Arc<Mutex<Vec<CapturedPost>>>,
}

/// A snapshot of one POST the mock captured — headers (lowercased
/// names, string values) plus a parsed-JSON body. Structured so a
/// failing assertion in a test prints the full payload rather than
/// a byte vector the human has to decode by hand.
#[derive(Debug, Clone)]
pub struct CapturedPost {
    /// Header name → value, keys lowercased. Only the headers the
    /// mock actually inspects are captured (see
    /// [`capture_relevant_headers`]); a test that wants to assert a
    /// custom header adds it to the capture list in one place.
    pub headers: std::collections::BTreeMap<String, String>,
    /// Parsed JSON body. When the client sends malformed JSON the
    /// handler short-circuits with 400 before capturing, so this is
    /// always a valid `Value`.
    pub body: serde_json::Value,
}

impl AppState {
    fn new() -> Self {
        Self {
            overrides: Arc::new(Mutex::new(Overrides::default())),
            received_events: Arc::new(Mutex::new(Vec::new())),
            received_executes: Arc::new(Mutex::new(Vec::new())),
            received_auto_toggles: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// A running mock. Holds the listening address and a shutdown
/// handle; automatically aborts on drop as a safety net, but prefer
/// explicit `.shutdown()` so the port is reclaimable immediately.
#[derive(Debug)]
pub struct MockEngine {
    addr: SocketAddr,
    state: AppState,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl MockEngine {
    /// Bind to `127.0.0.1:0`, return the running mock.
    pub async fn spawn() -> anyhow::Result<Self> {
        let state = AppState::new();
        let app = router(state.clone()).with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (tx, rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });

        // Brief pause so the server is ready to accept connections
        // before the first request. Keeps tests flake-free without a
        // full readiness handshake.
        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(Self {
            addr,
            state,
            shutdown: Some(tx),
            handle: Some(handle),
        })
    }

    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// `ws://…/ws` URL for the subscriber to connect to.
    #[must_use]
    pub fn ws_url(&self) -> String {
        format!("ws://{}/ws", self.addr)
    }

    pub fn with_overrides(&self, mutate: impl FnOnce(&mut Overrides)) {
        let mut o = self.state.overrides.lock();
        mutate(&mut o);
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for MockEngine {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}

fn router(shared: AppState) -> Router<AppState> {
    // Typed endpoints run behind a failure-injection layer so tests
    // can exercise auth / not-found / transient-503 paths without
    // having to mutate each handler. `/`, `/health`, and `/ws` stay
    // outside the layer: the version probe + health surface are
    // unauthenticated by design, and the ws handler has its own
    // drop-once hook (`ws_drop_once`).
    //
    // The middleware captures `shared` at construction time — the
    // same `AppState` the rest of the router is given via
    // `.with_state()`. Because `AppState`'s interior is `Arc<Mutex<…>>`,
    // mutations from the test side are visible to both.
    let typed = Router::new()
        .route("/v2/status", get(v2_status))
        .route("/positions", get(positions))
        .route("/risk", get(risk))
        .route("/regime", get(regime))
        .route("/brief", get(brief))
        .route("/evaluate/:coin", get(evaluate))
        .route("/pulse", get(pulse))
        .route("/approaching", get(approaching))
        .route("/rejections", get(rejections))
        .route("/hl/status", get(hl_status))
        .route("/operator/state", get(operator_state))
        .route("/operator/events", post(operator_events))
        .route("/execute", post(execute))
        .route("/auto/toggle", post(auto_toggle))
        .layer(middleware::from_fn_with_state(shared, inject_failures));

    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/ws", get(ws_handler))
        .merge(typed)
}

/// Middleware that consults [`Overrides`] before every typed
/// request and short-circuits with a synthetic failure when any
/// injection is active. Priorities (most-to-least specific):
///
/// 1. `force_unauthorized` → 401. This comes first because
///    "your token is wrong" must beat "rate limited" on the
///    operator's screen.
/// 2. `force_not_found` → 404. Runs before `force_server_error`
///    so tests can express "this engine doesn't serve that
///    endpoint" cleanly.
/// 3. `rate_limit_count > 0` → 429 with `Retry-After`, decrement.
///    Ahead of the transient-503 rule: a test that sets both
///    wants to see the 429 path first (the CLI client does not
///    retry 429, so the two injections are never meant to
///    cascade). Decrement is atomic under the overrides lock.
/// 4. `transient_fail_count > 0` → 503, decrement. This is the
///    retry-policy probe; the decrement happens atomically
///    under the overrides lock so a concurrent test can't
///    double-spend the budget.
/// 5. `force_server_error` → 500. Non-retryable per the
///    client's policy; verifies the client does **not**
///    re-call on this status.
async fn inject_failures(
    State(s): State<AppState>,
    req: Request,
    next: Next,
) -> axum::response::Response {
    let action = {
        let mut o = s.overrides.lock();
        if o.force_unauthorized {
            InjectAction::Unauthorized
        } else if o.force_not_found {
            InjectAction::NotFound
        } else if o.rate_limit_count > 0 {
            o.rate_limit_count -= 1;
            InjectAction::RateLimited(o.rate_limit_retry_after.clone())
        } else if o.transient_fail_count > 0 {
            o.transient_fail_count -= 1;
            InjectAction::Transient
        } else if o.force_server_error {
            InjectAction::ServerError
        } else {
            InjectAction::Pass
        }
    };
    match action {
        InjectAction::Unauthorized => (StatusCode::UNAUTHORIZED, "missing token").into_response(),
        InjectAction::NotFound => (StatusCode::NOT_FOUND, "unknown endpoint").into_response(),
        InjectAction::RateLimited(header) => {
            // Default to `"1"` so a test inspecting the client's
            // parsed duration observes an unambiguous 1 s. A real
            // engine would carry a real wall-clock budget here.
            let retry_after = header.unwrap_or_else(|| "1".to_string());
            let mut resp = (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response();
            if let Ok(v) = retry_after.parse() {
                resp.headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, v);
            }
            resp
        }
        InjectAction::Transient => (StatusCode::SERVICE_UNAVAILABLE, "retry me").into_response(),
        InjectAction::ServerError => {
            (StatusCode::INTERNAL_SERVER_ERROR, "unexpected").into_response()
        }
        InjectAction::Pass => next.run(req).await,
    }
}

#[derive(Debug, Clone)]
enum InjectAction {
    Pass,
    Unauthorized,
    NotFound,
    RateLimited(Option<String>),
    Transient,
    ServerError,
}

async fn root(State(s): State<AppState>) -> impl IntoResponse {
    let version = s
        .overrides
        .lock()
        .version
        .clone()
        .unwrap_or_else(|| "1.2.3-mock".to_string());
    Json(json!({
        "name": "ZERO OS",
        "version": version,
        "status": "running",
        "ts": chrono_utc_now_iso(),
    }))
}

async fn health(State(s): State<AppState>) -> Response {
    let o = s.overrides.lock().clone();
    if o.health_503 {
        return (StatusCode::SERVICE_UNAVAILABLE, "overloaded").into_response();
    }
    let status = if o.degrade_health { "degraded" } else { "ok" };
    Json(json!({
        "status": status,
        "components": {
            "controller": {"status": "healthy", "last_seen": chrono_utc_now_iso(), "age_s": 1.1},
            "market_data": {"status": "healthy", "last_seen": chrono_utc_now_iso(), "age_s": 0.4},
        },
        "dependencies": {"hyperliquid": "healthy", "llm": "healthy"},
        "circuit_breakers": {},
        "risk": {
            "account_value": 10_000.0,
            "drawdown_pct": 0.8,
            "halted": false,
        },
        "ws_connections": 0,
    }))
    .into_response()
}

// ─── M1 HTTP breadth endpoints ─────────────────────────────────────

async fn v2_status() -> Json<serde_json::Value> {
    // Real engine shape (see `zero-engine-client/tests/fixtures/v2_status.json`):
    // confidence/market/positions/today are nested sub-objects;
    // `regime` lives under `market.regime`, `engine_confidence`
    // under `confidence.score` (0..=100 integer, not 0..=1 float).
    Json(json!({
        "confidence": {"score": 72, "level": "high"},
        "market": {
            "regime": "TREND_LONG confirmed across majors.",
            "health": 0.954,
            "signal": "stable",
            "prediction": "stable",
            "fear_greed": 54,
            "coins_tradeable": 30
        },
        "positions": {"open": 2, "unrealized_pnl": 34.12, "equity": 10_034.12},
        "today": {"trades": 24, "wins": 15, "pnl": -3.95, "streak": -3, "sizing_mult": 0.7},
        "approaching": [],
        "blind_spots": [],
        "alert": null,
        "ts": chrono_utc_now_iso(),
    }))
}

async fn positions() -> Json<serde_json::Value> {
    Json(json!({
        "positions": [
            {
                "symbol": "BTC",
                "side": "long",
                "size": 0.42,
                "entry": 64_120.5,
                "mark": 64_480.0,
                "unrealized_pnl": 151.13,
                "unrealized_r": 0.82,
                "stop": 63_800.0,
                "target": 65_400.0,
                "lens_id": "alpha_v3",
                "age_s": 1_824.0
            },
            {
                "symbol": "ETH",
                "side": "short",
                "size": 1.2,
                "entry": 3_120.0,
                "mark": 3_098.0,
                "unrealized_pnl": 26.4,
                "unrealized_r": 0.31,
                "stop": 3_160.0,
                "target": 3_010.0,
                "lens_id": "beta_v1",
                "age_s": 421.0
            }
        ],
        "account_value": 10_034.12,
        "total_unrealized_pnl": 177.53
    }))
}

async fn risk(State(s): State<AppState>) -> Json<serde_json::Value> {
    let o = s.overrides.lock().clone();
    // Stale-equity-field path: production engines have been observed
    // to carry an `account_value` in `risk.json` that has drifted
    // above `peak_equity`, which is impossible by definition (peak
    // is a monotonic max of account_value). The dispatcher must
    // flag the cross-field contradiction instead of passing the
    // fake drawdown percent through. We mirror the real numbers
    // from the reported incident so the tested error text is the
    // one operators will see.
    if o.force_stale_risk_equity {
        return Json(json!({
            "account_value": 638.488_706,       // stale (higher)
            "updated_at": chrono_utc_now_iso(),
            "daily_pnl_usd": -3.312,
            "daily_loss_usd": 4.1261,
            "per_runner": {},
            "global_halt": false,
            "daily_loss_since": chrono_utc_now_iso(),
            "halted": false,
            "halt_reason": null,
            "halt_until": null,
            "stop_failure_halt": false,
            "open_count": 0,
            "drawdown_pct": 0.22,               // computed against a stale peak
            "peak_equity": 577.338_628,         // actual peak
            "peak_equity_30d": 577.34,
            "last_drawdown_alert_pct": 20,
            "capital_floor_hit": false
        }));
    }
    // Real engine shape (see `zero-engine-client/tests/fixtures/risk.json`):
    // `account_value`, `daily_pnl_usd` / `daily_loss_usd` (dollars,
    // not percent), `halted` / `global_halt` / `stop_failure_halt`
    // (booleans), `open_count`, `peak_equity`. The legacy
    // `kill_all` / `exposure_pct` / `daily_loss_pct` names were
    // invented by the old mock; the live engine does not emit
    // them.
    Json(json!({
        "account_value": 10_034.12,
        "updated_at": chrono_utc_now_iso(),
        "daily_pnl_usd": 34.12,
        "daily_loss_usd": 4.1261,
        "per_runner": {},
        "global_halt": false,
        "daily_loss_since": chrono_utc_now_iso(),
        "halted": false,
        "halt_reason": null,
        "halt_until": null,
        "stop_failure_halt": false,
        "open_count": 2,
        "drawdown_pct": 0.8,
        "peak_equity": 10_100.0,
        "peak_equity_30d": 10_100.0,
        "last_drawdown_alert_pct": 20,
        "capital_floor_hit": false
    }))
}

async fn regime(State(s): State<AppState>) -> Json<serde_json::Value> {
    let o = s.overrides.lock().clone();
    // Engine-error envelope: a 200 that carries `{"error": "..."}`
    // instead of a regime payload. Lets tests pin the dispatcher's
    // "don't render em-dashes on an error envelope" contract.
    if o.force_regime_error_envelope {
        return Json(json!({"error": "coin not found"}));
    }
    // Empty-body path: engine exposes the endpoint but has no
    // regime reading. Lets tests pin the "alert instead of
    // em-dashes" contract.
    if o.force_empty_regime {
        return Json(json!({}));
    }
    Json(json!({
        "regime": "TREND_LONG",
        "confidence": 0.81,
        "trending_long": 7,
        "trending_short": 2,
        "choppy": 3
    }))
}

async fn brief() -> Json<serde_json::Value> {
    // Real engine shape (see `zero-engine-client/tests/fixtures/brief.json`):
    // fear_greed, open_positions, positions list, recent_signals,
    // approaching, last_cycle object. No headline/summary strings.
    Json(json!({
        "timestamp": chrono_utc_now_iso(),
        "fear_greed": 54,
        "open_positions": 2,
        "positions": [
            {
                "symbol": "BTC",
                "side": "long",
                "size": 0.42,
                "entry": 64_120.5,
                "mark": 64_480.0,
                "unrealized_pnl": 151.13,
                "unrealized_r": 0.82
            }
        ],
        "recent_signals": [
            {"coin": "BTC", "kind": "signal", "message": "edge_floor cleared"}
        ],
        "approaching": [
            {"coin": "AVAX", "direction": "long", "distance_to_gate": 0.04}
        ],
        "last_cycle": {
            "regime": "TREND_LONG",
            "signals_evaluated": 30,
            "actions_taken": 2
        }
    }))
}

async fn evaluate(
    State(s): State<AppState>,
    axum::extract::Path(coin): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    // Degenerate-200 path, off by default. When
    // `force_empty_evaluation` is set, return a body that decodes
    // as `Evaluation` but has no `layers` and no `direction` —
    // the exact shape that tricked the real engine into rendering
    // an empty verdict card. Lets tests lock in the dispatcher's
    // "empty verdict → alert + dismiss, don't open overlay" contract
    // without having to mock a half-crashed engine end-to-end.
    if s.overrides.lock().force_empty_evaluation {
        return Json(json!({
            "coin": coin,
            "layers": [],
            "data_fresh": true,
            "timestamp": chrono_utc_now_iso()
        }));
    }
    // Real engine shape (see `zero-engine-client/tests/fixtures/evaluate_sol.json`):
    // a flat object with `layers: [{layer, passed, value, detail}]`,
    // `direction` ("LONG" | "SHORT" | "NONE"), `conviction`,
    // `consensus`, `regime`, `data_fresh`, `timestamp`. The legacy
    // mock's `verdict` / `gates` / `rationale` were never emitted
    // by the live engine.
    Json(json!({
        "coin": coin,
        "price": 85.48,
        "consensus": 10,
        "conviction": 0.64,
        "direction": "NONE",
        "regime": "random_quiet",
        "layers": [
            {"layer": "layer_0", "passed": true,  "value": "random_quiet", "detail": "regime=random_quiet"},
            {"layer": "layer_1", "passed": true,  "value": {"agree": 0, "oppose": 0}, "detail": "technical neutral"},
            {"layer": "layer_2", "passed": false, "value": 1.25e-05, "detail": "funding_rate below threshold"}
        ],
        "data_fresh": true,
        "timestamp": chrono_utc_now_iso()
    }))
}

async fn pulse() -> Json<serde_json::Value> {
    // Real engine shape (see `zero-engine-client/tests/fixtures/pulse.json`):
    // `{ events: [...], count, timestamp }`. The `Pulse` struct
    // aliases both `pulse` and `events`, so either key works on
    // the wire; we emit the real one.
    Json(json!({
        "events": [
            {"kind": "signal",    "coin": "BTC", "message": "edge_floor cleared",      "ts": chrono_utc_now_iso(), "severity": "info"},
            {"kind": "rejection", "coin": "SOL", "message": "stage2 HOLD on volume",   "ts": chrono_utc_now_iso(), "severity": "info"}
        ],
        "count": 2,
        "timestamp": chrono_utc_now_iso()
    }))
}

async fn approaching(State(s): State<AppState>) -> axum::response::Response {
    // Simulate older engine builds that predate `/approaching`.
    // The real production engine at api.getzero.dev returns
    // `{"detail": "Not Found"}` on this path, so we mirror that
    // body shape so the client's error-mapping stays honest.
    if s.overrides.lock().force_approaching_not_found {
        return (StatusCode::NOT_FOUND, Json(json!({"detail": "Not Found"}))).into_response();
    }
    Json(json!({
        "approaching": [
            {"coin": "AVAX", "direction": "long", "distance_to_gate": 0.04, "gate": "edge_floor", "ts": chrono_utc_now_iso()},
            {"coin": "LINK", "direction": "short", "distance_to_gate": 0.07, "gate": "stage2", "ts": chrono_utc_now_iso()}
        ]
    }))
    .into_response()
}

async fn rejections() -> Json<serde_json::Value> {
    Json(json!({
        "rejections": [
            {"coin": "SOL", "direction": "long", "stage": "stage2", "reason": "volume below threshold", "ts": chrono_utc_now_iso()}
        ]
    }))
}

async fn hl_status(
    axum::extract::Query(query): axum::extract::Query<BTreeMap<String, String>>,
) -> Json<serde_json::Value> {
    let mids = match query.get("symbol").map(String::as_str) {
        Some("BTC") => json!({"BTC": 40500.0}),
        Some("ETH") => json!({"ETH": 2850.0}),
        Some(symbol) => json!({symbol: 100.0}),
        None => json!({"BTC": 40500.0, "ETH": 2850.0}),
    };
    Json(json!({
        "enabled": true,
        "exchange": "hyperliquid",
        "endpoint": "https://api.hyperliquid.xyz/info",
        "coins": 2,
        "mids": mids,
        "secrets_required": false
    }))
}

// ─── /operator/state — behavioral classifier snapshot ─────────────

async fn operator_state(State(s): State<AppState>) -> Json<serde_json::Value> {
    let (label, version) = {
        let o = s.overrides.lock();
        (
            o.operator_label
                .clone()
                .unwrap_or_else(|| "steady".to_string()),
            o.operator_version,
        )
    };
    let friction = match label.as_str() {
        "elevated" | "fatigued" => "l1",
        "tilt" => "l2",
        _ => "l0",
    };
    // Minimal vector — every numeric component defaults to zero.
    // Tests that care about classifier internals can build their
    // own payload by hitting the endpoint directly with reqwest.
    Json(json!({
        "label": label,
        "friction": friction,
        "vector": {
            "velocity": {"last_1h": 0, "last_4h": 0, "last_24h": 0, "baseline_1h": null},
            "deviation": {
                "overrides_last_10": 0, "verdicts_last_10": 0,
                "overrides_last_50": 0, "verdicts_last_50": 0,
            },
            "session": {"active_duration_ms": 0, "longest_focus_ms": 0, "since_last_break_ms": 0},
            "loss_reaction": {
                "median_last_10_ms": 0, "fastest_session_ms": 0, "baseline_ms": null,
            },
            "re_entry": {"within_15m": 0, "within_30m": 0, "within_2h": 0},
            "sleep_proxy": {"hours_since_rest_ended": null},
            "on_break": false,
        },
        "as_of": chrono_utc_now_iso(),
        "version": version,
    }))
}

// ─── POST /operator/events — one-way ingress for classifier events ───
//
// Mirrors the real engine's contract enough to let the CLI-side
// rewires (`/rate`, `/break`) be exercised end-to-end: accept either
// a single event object or a `{"events": [...]}` batch, record each
// decoded body into `received_events` for test assertions, and reply
// with `{"accepted": N, "snapshot": <Snapshot>}` using the same
// `/operator/state` snapshot shape so the CLI's `post_operator_event`
// deserializer does not need a separate test fixture.
//
// Validation is minimal on purpose — the mock is not the engine. A
// request whose JSON does not deserialize at all falls through to
// 400; malformed event *shapes* still succeed here because the
// engine-side integration tests (Python `test_operator_state_endpoints`)
// own the per-field rejection paths, and duplicating them would mean
// the Rust side starts drifting from the Python contract.

async fn operator_events(
    State(s): State<AppState>,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("{{\"error\":\"{e}\"}}")))?;

    let events: Vec<serde_json::Value> = match &parsed {
        serde_json::Value::Object(map) if map.contains_key("events") => {
            map["events"].as_array().cloned().unwrap_or_default()
        }
        serde_json::Value::Array(arr) => arr.clone(),
        serde_json::Value::Object(_) => vec![parsed.clone()],
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "{\"error\":\"body must be an object or array\"}".to_string(),
            ));
        }
    };

    {
        let mut log = s.received_events.lock();
        for ev in &events {
            log.push(ev.clone());
        }
    }

    // Reply with a fresh snapshot; reuse the `operator_state` logic
    // so the post-accept snapshot the CLI sees is identical to what
    // a subsequent GET would return.
    let Json(snapshot) = operator_state(State(s)).await;
    Ok(Json(json!({
        "accepted": events.len(),
        "snapshot": snapshot,
    })))
}

impl MockEngine {
    /// Snapshot of every `POST /operator/events` body the mock has
    /// received, in arrival order. Returned by clone so a later POST
    /// cannot mutate a value the test is inspecting.
    #[must_use]
    pub fn received_operator_events(&self) -> Vec<serde_json::Value> {
        self.state.received_events.lock().clone()
    }

    /// Snapshot of every `POST /execute` (headers + body) the mock
    /// has received, in arrival order. Tests inspect the captured
    /// `x-zero-mode` and `x-idempotency-key` headers here.
    #[must_use]
    pub fn received_executes(&self) -> Vec<CapturedPost> {
        self.state.received_executes.lock().clone()
    }

    /// Snapshot of every `POST /auto/toggle` (headers + body) the
    /// mock has received, in arrival order. Sister to
    /// [`Self::received_executes`].
    #[must_use]
    pub fn received_auto_toggles(&self) -> Vec<CapturedPost> {
        self.state.received_auto_toggles.lock().clone()
    }
}

// ─── /execute (POST) ───────────────────────────────────────────────
//
// M2_PLAN §7 — mock surface for the composition-change endpoint.
// The handler is deliberately dumb: accept any syntactically-valid
// JSON body, capture it + the headers the CLI must populate, and
// echo back a realistic response. Overrides drive the 5xx / 500 /
// simulated paths for tests that pin the no-retry rule and the
// paper-mode discriminator.

// `side` / `size` mirror the wire shape of the live `/execute`
// handler; renaming either to appease `similar_names` would break
// the visual parity with the engine-side Python counterpart.
#[allow(clippy::similar_names)]
async fn execute(
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Injection paths must short-circuit *before* body capture so a
    // test that sets `post_transient_fail` sees no capture — the
    // CLI's no-retry rule means the server observes one request,
    // returns one 503, and the client does not re-send.
    {
        let o = s.overrides.lock();
        if o.post_transient_fail {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "retry me".into()));
        }
        if o.post_server_error {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "boom".into()));
        }
    }

    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("{{\"error\":\"{e}\"}}")))?;

    let captured = CapturedPost {
        headers: capture_relevant_headers(&headers),
        body: parsed.clone(),
    };
    s.received_executes.lock().push(captured);

    // The engine asserts `simulated` based on the inbound
    // `X-Zero-Mode` header (or the launch-time default when the
    // header is absent); the mock mirrors that so tests can pin
    // both paths via the header alone.
    let mode_header = headers
        .get("x-zero-mode")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let force_sim = s.overrides.lock().force_simulated;
    let simulated = force_sim || mode_header.eq_ignore_ascii_case("paper");

    // Echo the inbound coin/side/size so the test can round-trip
    // the typed `ExecuteRequest` through the wire and see its
    // shape arrive in the typed `ExecuteResponse`.
    let coin = parsed.get("coin").cloned().unwrap_or(json!("BTC"));
    let side = parsed.get("side").cloned().unwrap_or(json!("buy"));
    let size = parsed.get("size").cloned().unwrap_or(json!(0.0));
    let key = parsed
        .get("idempotency_key")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    // `fill_id` is deterministic for paper fills, randomized on
    // live. The mock substitutes the idempotency key so tests can
    // assert the key made the round trip from request to response.
    let fill_id = if simulated {
        format!("paper-{key}")
    } else {
        format!("live-{key}")
    };

    Ok(Json(json!({
        "accepted": true,
        "simulated": simulated,
        "fill_id": fill_id,
        "coin": coin,
        "side": side,
        "size": size,
    })))
}

// ─── /auto/toggle (POST) ───────────────────────────────────────────

async fn auto_toggle(
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    {
        let o = s.overrides.lock();
        if o.post_transient_fail {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "retry me".into()));
        }
        if o.post_server_error {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "boom".into()));
        }
    }

    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("{{\"error\":\"{e}\"}}")))?;

    let captured = CapturedPost {
        headers: capture_relevant_headers(&headers),
        body: parsed.clone(),
    };
    s.received_auto_toggles.lock().push(captured);

    let requested = parsed
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let (echo, reason) = {
        let o = s.overrides.lock();
        (o.auto_toggle_echo_state, o.auto_toggle_reason.clone())
    };
    let actual = echo.unwrap_or(requested);
    let state_str = if actual { "on" } else { "off" };

    let mode_header = headers
        .get("x-zero-mode")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let force_sim = s.overrides.lock().force_simulated;
    let simulated = force_sim || mode_header.eq_ignore_ascii_case("paper");

    let mut resp = serde_json::Map::new();
    resp.insert("state".into(), json!(state_str));
    resp.insert("simulated".into(), json!(simulated));
    if let Some(r) = reason {
        resp.insert("reason".into(), json!(r));
    }
    Ok(Json(serde_json::Value::Object(resp)))
}

/// Capture exactly the headers tests assert on (lowercased names →
/// string values). Intentionally narrow so the captured blob is
/// tractable to `assert_eq!` against; headers the tests don't care
/// about (user-agent, accept, content-length) are dropped.
fn capture_relevant_headers(
    headers: &axum::http::HeaderMap,
) -> std::collections::BTreeMap<String, String> {
    const RELEVANT: &[&str] = &[
        "x-zero-mode",
        "x-idempotency-key",
        "content-type",
        "authorization",
    ];
    let mut out = std::collections::BTreeMap::new();
    for name in RELEVANT {
        if let Some(v) = headers.get(*name)
            && let Ok(s) = v.to_str()
        {
            out.insert((*name).to_string(), s.to_string());
        }
    }
    out
}

// ─── /ws — push surface for EngineState ────────────────────────────

async fn ws_handler(ws: WebSocketUpgrade, State(s): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_ws(socket, s))
}

async fn handle_ws(mut socket: WebSocket, s: AppState) {
    // If the test asked us to drop the connection on accept, take
    // that flag (consuming it) and close immediately so the
    // subscriber exercises its reconnect path. Note the explicit
    // scope on the mutex guard — parking_lot's guard is !Send, so
    // it must not live across the `.await` below.
    let should_drop = {
        let mut o = s.overrides.lock();
        if o.ws_drop_once {
            o.ws_drop_once = false;
            true
        } else {
            false
        }
    };
    if should_drop {
        let _ = socket.close().await;
        return;
    }

    // Canonical test fixture sequence. Order matters: heartbeat
    // first so any subscriber waiting on `last_heartbeat` unblocks,
    // then state-carrying events.
    let events = [
        json!({"event": "heartbeat", "ts": now_iso(), "data": {}}),
        json!({
            "event": "v2_status",
            "ts": now_iso(),
            "data": {
                "confidence": {"score": 72, "level": "high"},
                "market": {
                    "regime": "TREND_LONG",
                    "health": 0.954,
                    "fear_greed": 54,
                    "coins_tradeable": 30
                },
                "positions": {"open": 2, "unrealized_pnl": 34.12, "equity": 10_034.12},
                "today": {"trades": 24, "wins": 15, "pnl": -3.95},
                "approaching": [],
                "blind_spots": []
            }
        }),
        json!({
            "event": "positions_update",
            "ts": now_iso(),
            "data": {
                "positions": [
                    {"symbol": "BTC", "side": "long", "size": 0.42, "entry": 64_120.5,
                     "mark": 64_480.0, "unrealized_pnl": 151.13, "unrealized_r": 0.82}
                ],
                "account_value": 10_034.12,
                "total_unrealized_pnl": 151.13
            }
        }),
        json!({
            "event": "risk_update",
            "ts": now_iso(),
            "data": {
                "account_value": 10_034.12,
                "drawdown_pct": 0.8,
                "halted": false,
                "global_halt": false,
                "stop_failure_halt": false,
                "daily_pnl_usd": 34.12,
                "daily_loss_usd": 20.0,
                "peak_equity": 10_100.0,
                "open_count": 2
            }
        }),
        json!({
            "event": "regime_update",
            "ts": now_iso(),
            "data": {"regime": "TREND_LONG", "confidence": 0.81}
        }),
    ];

    for ev in events {
        if socket.send(Message::Text(ev.to_string())).await.is_err() {
            return;
        }
    }

    // Remain connected until the client disconnects or asks us to
    // close. Periodic heartbeats keep the freshness clock alive in
    // longer-running tests.
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let hb = json!({"event": "heartbeat", "ts": now_iso(), "data": {}});
                if socket.send(Message::Text(hb.to_string())).await.is_err() {
                    return;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                    _ => {}
                }
            }
        }
    }
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // RFC-3339 stub good enough for the subscriber's parser; the
    // subscriber falls back to `Utc::now()` when parsing fails.
    format!("2026-01-01T00:00:{:02}Z", secs % 60)
}

/// Alias kept tight so response bodies in this module stay single-line.
type Response = axum::response::Response;

fn chrono_utc_now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Cheap ISO-ish timestamp good enough for test fixtures; no
    // dependency on `chrono` inside this crate.
    format!("1970-01-01T00:00:{:02}Z", secs % 60)
}
