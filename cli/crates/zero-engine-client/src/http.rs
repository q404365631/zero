//! HTTP client for the engine REST surface.
//!
//! Semantics (spec §7, ADR-002):
//!
//! - 8 s per-attempt timeout.
//! - Retry **once** on transport errors, 502, 503, 504, with a fixed
//!   500 ms backoff. All other statuses fail immediately with the
//!   body carried.
//! - Bearer token applied per request when present.
//! - All typed helpers go through [`Self::get_json`] so retry / auth
//!   / error mapping lives in exactly one place.

use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use zero_operator_state::{Event as OperatorEvent, Snapshot as OperatorSnapshot};

use crate::models::{
    ApproachingFeed, AutoToggleRequest, AutoToggleResponse, Brief, Evaluation, ExecuteRequest,
    ExecuteResponse, Health, HyperliquidAccount, HyperliquidReconciliation, HyperliquidStatus,
    LiveCertification, LiveControlResponse, LivePreflight, MarketQuote, OperatorEventsAccepted,
    Positions, Pulse, Regime, RejectionsFeed, Risk, Root, V2Status,
};
use crate::rate_budget::{self, RateBudget};

const TIMEOUT: Duration = Duration::from_secs(8);
const RETRY_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("engine unreachable — {0}")]
    Unreachable(String),
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    #[error("auth rejected (401/403)")]
    Unauthorized,
    #[error("not found: {path}")]
    NotFound { path: String },
    #[error("http {status}: {body}")]
    Status { status: StatusCode, body: String },
    #[error("decode: {0}")]
    Decode(String),
    #[error("url: {0}")]
    Url(#[from] url::ParseError),
    /// Either the CLI-side [`RateBudget`] was exhausted before the
    /// request ran (common case — the operator is typing faster
    /// than the bucket refills), or the engine's own limiter
    /// returned 429 (rare case — usually means two CLIs / an Auto
    /// agent / a Telegram bot are sharing the operator's bucket).
    ///
    /// `retry_after` is a floor-rounded `Duration`. `source` is
    /// one of the two strings `"cli-budget"` or `"engine-429"` so
    /// logs can differentiate even though the operator-visible
    /// render is identical — from the operator's seat, both
    /// failures should read as "rate: exhausted — retry in Ns"
    /// because telling them "your local bucket vs. the engine's
    /// bucket" is a distinction without a difference.
    // `origin` (not `source`) — `thiserror` reserves the field name
    // `source` for the `Error::source()` chain; a plain data field
    // by that name gets pulled into trait-bound inference.
    //
    // `Display` is operator-targeted, not programmer-targeted. The
    // command-line handlers forward this through
    // `format!("{}: {e}", name)` onto a single TUI pane row; the
    // shape we want the operator to read is "rate: exhausted —
    // retry in 3s", not a `Duration { secs: 3, nanos: 0 }` dump.
    // [`format_retry_after`] does the dumb right thing (whole
    // seconds, or ">1h" when `Duration::MAX`). The origin is
    // elided from the operator-facing string (logs carry it via
    // `Debug`) because the CLI-vs-engine distinction is never
    // actionable for the operator — the correct response is
    // identical either way (wait).
    #[error("rate: exhausted — retry in {}", format_retry_after(*.retry_after))]
    RateBudgetExhausted {
        retry_after: Duration,
        origin: RateLimitSource,
    },
}

/// Where a [`HttpError::RateBudgetExhausted`] originated. Rendered
/// as a terse tag in log lines; never user-visible directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitSource {
    /// The CLI's local [`RateBudget`] refused the call before it
    /// left the process. Bucket already debited the `retry_after`
    /// against its own refill math.
    CliBudget,
    /// The engine's limiter returned 429. The client refunded its
    /// own bucket and re-packaged the engine's `Retry-After` into
    /// the returned duration.
    Engine429,
}

impl std::fmt::Display for RateLimitSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::CliBudget => "cli-budget",
            Self::Engine429 => "engine-429",
        })
    }
}

/// HTTP client bound to one engine.
///
/// Holds an optional [`RateBudget`]. When `None`, the client is
/// unthrottled — appropriate for narrow test paths that want to
/// exercise transport behavior without budget interference. In
/// production the client is always built with a budget attached;
/// the `with_rate_budget` builder method is the canonical on-ramp.
#[derive(Debug, Clone)]
pub struct HttpClient {
    base_url: url::Url,
    token: Option<String>,
    inner: reqwest::Client,
    rate_budget: Option<RateBudget>,
    /// Engine-mode override attached to every outgoing request
    /// via the `X-Zero-Mode` header. `None` means "respect the
    /// engine's launch-time mode" (no header emitted, the legacy
    /// path). [`Mode::Paper`] / [`Mode::Live`] are emitted verbatim
    /// — the header is the only per-invocation override surface
    /// and M2_PLAN §5/§7 pins the exact wire shape.
    mode: Option<Mode>,
}

/// Per-invocation engine-mode override. See [`HttpClient::with_mode`].
///
/// Named `Mode` rather than `EngineMode` so the import list on
/// the adapter side stays short (`use zero_engine_client::Mode;`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Paper,
    Live,
}

impl Mode {
    /// Wire representation used in the `X-Zero-Mode` header.
    /// Lowercase to match the header-value convention on the
    /// engine side; deliberately kept narrow so a future
    /// `Shadow` / `Replay` mode lands with an explicit parser
    /// rather than a silent `to_ascii_lowercase` extension.
    #[must_use]
    pub const fn as_header_value(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Live => "live",
        }
    }
}

impl HttpClient {
    /// Build a client for the given base URL. The URL must be
    /// parseable as an absolute URL (no trailing path needed; joined
    /// relative paths land under it). The returned client has **no**
    /// rate budget attached — callers who want one chain
    /// `.with_rate_budget(...)` (production path) or leave it off
    /// (narrow test paths that want raw transport behavior).
    pub fn new(base_url: impl AsRef<str>, token: Option<String>) -> Result<Self, HttpError> {
        let base_url = url::Url::parse(base_url.as_ref())?;
        let inner = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("zero-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| HttpError::Unreachable(e.to_string()))?;
        Ok(Self {
            base_url,
            token,
            inner,
            rate_budget: None,
            mode: None,
        })
    }

    /// Attach a per-invocation engine-mode override. Every
    /// subsequent request carries `X-Zero-Mode: <value>`; the
    /// engine honors the header per M2_PLAN §5 / §7. Passing
    /// `Mode::Live` is explicit (`None` means "respect engine
    /// launch mode"), so an operator invoking `zero --paper`
    /// followed by a non-paper command inside the same TUI
    /// session gets paper → live flipped via the adapter, not
    /// via a header absence.
    #[must_use]
    pub fn with_mode(mut self, mode: Mode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Access the attached engine-mode override. The TUI status
    /// bar + the doctor row use this so the mode breadcrumb is
    /// rendered off the same source of truth the HTTP layer
    /// will act on.
    #[must_use]
    pub const fn mode(&self) -> Option<Mode> {
        self.mode
    }

    /// Attach a [`RateBudget`]. Every subsequent call consults the
    /// budget (via [`rate_budget::cost_of`] on the request path)
    /// before the request leaves the process. A `None` budget
    /// (the default after [`Self::new`]) disables the whole layer.
    #[must_use]
    pub fn with_rate_budget(mut self, budget: RateBudget) -> Self {
        self.rate_budget = Some(budget);
        self
    }

    /// Access the attached [`RateBudget`], if any. The doctor row
    /// and the status-bar widget use this to read a
    /// [`crate::BudgetSnapshot`]; holding the reference rather
    /// than cloning lets callers take a fresh snapshot on every
    /// render.
    #[must_use]
    pub const fn rate_budget(&self) -> Option<&RateBudget> {
        self.rate_budget.as_ref()
    }

    #[must_use]
    pub fn base_url(&self) -> &url::Url {
        &self.base_url
    }

    #[must_use]
    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn url_for(&self, path: &str) -> Result<url::Url, HttpError> {
        let path = path.trim_start_matches('/');
        Ok(self.base_url.join(path)?)
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.token
            && let Ok(v) = HeaderValue::from_str(&format!("Bearer {token}"))
        {
            headers.insert(AUTHORIZATION, v);
        }
        // M2 §5: `--paper` and `/auto` adapters plumb a
        // per-invocation engine-mode override through this header.
        // `HeaderValue::from_static` is safe here — every arm of
        // `Mode::as_header_value` is ASCII.
        if let Some(mode) = self.mode {
            headers.insert(
                HeaderName::from_static("x-zero-mode"),
                HeaderValue::from_static(mode.as_header_value()),
            );
        }
        headers
    }

    /// Consult the attached [`RateBudget`] (if any) for `path`. On
    /// exhaustion returns [`HttpError::RateBudgetExhausted`] shaped
    /// as `CliBudget`; on success the bucket has been debited and
    /// the caller may proceed to the network.
    ///
    /// Cost is resolved via [`rate_budget::cost_of`] so the client
    /// and every out-of-band consumer (doctor row, status bar)
    /// agree on pricing by construction.
    fn check_rate_budget(&self, path: &str) -> Result<(), HttpError> {
        let Some(budget) = &self.rate_budget else {
            return Ok(());
        };
        let cost = rate_budget::cost_of(path);
        budget.try_consume(cost).map_err(|exh| {
            tracing::debug!(
                path = %path,
                cost = cost,
                retry_after = ?exh.retry_after,
                "cli rate budget exhausted",
            );
            HttpError::RateBudgetExhausted {
                retry_after: exh.retry_after,
                origin: RateLimitSource::CliBudget,
            }
        })
    }

    /// Refund the cost associated with `path` to the attached
    /// [`RateBudget`] (if any). Called on the engine-429 path so
    /// the local bucket is not double-charged when the engine's
    /// own limiter fires.
    fn refund_rate_budget(&self, path: &str) {
        if let Some(budget) = &self.rate_budget {
            budget.refund(rate_budget::cost_of(path));
        }
    }

    /// GET a path and decode the JSON body into `T`.
    ///
    /// Order of operations:
    /// 1. **Consult the rate budget.** Exhausted bucket → typed
    ///    error, no network call. An operator hammering `/status`
    ///    reads a typed refusal, not a silent stall.
    /// 2. **Send once.** On retryable failure (502/503/504/
    ///    transport/timeout): sleep `RETRY_DELAY`, send again. One
    ///    retry only.
    /// 3. **On 429 (engine's limiter, not ours):** refund the
    ///    local bucket (we debited it in step 1) and return
    ///    [`HttpError::RateBudgetExhausted`] shaped as `Engine429`
    ///    with the engine's own `Retry-After` value parsed out.
    ///
    /// Auth failures (401 / 403) and 404 are mapped to dedicated
    /// variants because the TUI renders them differently.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, HttpError> {
        self.check_rate_budget(path)?;
        let url = self.url_for(path)?;
        let headers = self.auth_headers();

        match self.send_once::<T>(url.clone(), headers.clone()).await {
            Ok(t) => Ok(t),
            Err(e) if is_retryable(&e) => {
                tracing::debug!(%url, error = %e, "retrying after transient failure");
                tokio::time::sleep(RETRY_DELAY).await;
                match self.send_once::<T>(url, headers).await {
                    Ok(t) => Ok(t),
                    Err(e2) => Err(self.maybe_refund_for_429(path, e2)),
                }
            }
            Err(e) => Err(self.maybe_refund_for_429(path, e)),
        }
    }

    /// If `err` is an engine-originated 429 (already normalized
    /// by `send_once`), refund the local bucket for `path` so the
    /// operator is not double-charged — we debited our bucket in
    /// `check_rate_budget` before the send, and the engine just
    /// refused the request, meaning no work landed on their side.
    /// Unrelated errors pass through unchanged.
    fn maybe_refund_for_429(&self, path: &str, err: HttpError) -> HttpError {
        if matches!(
            err,
            HttpError::RateBudgetExhausted {
                origin: RateLimitSource::Engine429,
                ..
            }
        ) {
            self.refund_rate_budget(path);
            tracing::debug!(
                path = %path,
                "engine returned 429; refunded local bucket",
            );
        }
        err
    }

    async fn send_once<T: DeserializeOwned>(
        &self,
        url: url::Url,
        headers: HeaderMap,
    ) -> Result<T, HttpError> {
        let resp = self
            .inner
            .get(url.clone())
            .headers(headers)
            .send()
            .await
            .map_err(|e| map_transport(&e))?;

        let status = resp.status();
        if status.is_success() {
            return resp
                .json::<T>()
                .await
                .map_err(|e| HttpError::Decode(e.to_string()));
        }

        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(HttpError::Unauthorized),
            StatusCode::NOT_FOUND => Err(HttpError::NotFound {
                path: url.path().to_string(),
            }),
            StatusCode::TOO_MANY_REQUESTS => {
                // Pull the Retry-After header *before* consuming
                // the body (reqwest can consume either in any
                // order, but mixing them in one expression makes
                // the dependency hard to see). Engine may send it
                // as a number-of-seconds or an HTTP-date; either
                // shape parses via `parse_retry_after`.
                let retry_after = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after)
                    .unwrap_or(Duration::from_secs(1));
                Err(HttpError::RateBudgetExhausted {
                    retry_after,
                    origin: RateLimitSource::Engine429,
                })
            }
            _ => {
                let body = resp.text().await.unwrap_or_default();
                Err(HttpError::Status {
                    status,
                    body: truncate(&body, 512),
                })
            }
        }
    }

    /// POST a JSON body to a path and decode the JSON response into `R`.
    ///
    /// Retry semantics mirror [`Self::get_json`]: one retry on 502/503/504/
    /// transport/timeout with a 500 ms backoff. POSTs to `/operator/events`
    /// are idempotent at the bus-adapter layer (the event-log is append-
    /// only and the classifier replay is deterministic), so a retried
    /// duplicate is a no-op in the worst case — an extra benign duplicate
    /// in the event-log rather than a phantom trade. Any endpoint added
    /// later that is **not** idempotent must not route through this
    /// helper; the entire M2 spec's POST surface today is idempotent.
    pub async fn post_json<B, R>(&self, path: &str, body: &B) -> Result<R, HttpError>
    where
        B: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.check_rate_budget(path)?;
        let url = self.url_for(path)?;
        let mut headers = self.auth_headers();
        // Explicit `content-type` avoids a future reqwest behavior
        // change silently flipping this to `application/octet-stream`.
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let payload = serde_json::to_vec(body).map_err(|e| HttpError::Decode(e.to_string()))?;

        match self
            .post_once::<R>(url.clone(), headers.clone(), payload.clone())
            .await
        {
            Ok(v) => Ok(v),
            Err(e) if is_retryable(&e) => {
                tracing::debug!(%url, error = %e, "retrying POST after transient failure");
                tokio::time::sleep(RETRY_DELAY).await;
                match self.post_once::<R>(url, headers, payload).await {
                    Ok(v) => Ok(v),
                    Err(e2) => Err(self.maybe_refund_for_429(path, e2)),
                }
            }
            Err(e) => Err(self.maybe_refund_for_429(path, e)),
        }
    }

    async fn post_once<R: DeserializeOwned>(
        &self,
        url: url::Url,
        headers: HeaderMap,
        body: Vec<u8>,
    ) -> Result<R, HttpError> {
        let resp = self
            .inner
            .post(url.clone())
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|e| map_transport(&e))?;

        let status = resp.status();
        if status.is_success() {
            return resp
                .json::<R>()
                .await
                .map_err(|e| HttpError::Decode(e.to_string()));
        }

        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(HttpError::Unauthorized),
            StatusCode::NOT_FOUND => Err(HttpError::NotFound {
                path: url.path().to_string(),
            }),
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after)
                    .unwrap_or(Duration::from_secs(1));
                Err(HttpError::RateBudgetExhausted {
                    retry_after,
                    origin: RateLimitSource::Engine429,
                })
            }
            _ => {
                let body = resp.text().await.unwrap_or_default();
                Err(HttpError::Status {
                    status,
                    body: truncate(&body, 512),
                })
            }
        }
    }

    /// POST a JSON body **without** any retry budget, optionally
    /// attaching an `X-Idempotency-Key` header.
    ///
    /// Retry semantics (M2_PLAN §7):
    ///
    /// > `POST` endpoints never auto-retry (idempotency key
    /// > compensates, but a silent retry of a live composition
    /// > change is the single worst failure mode a trading CLI can
    /// > have). Tests pin the no-retry rule against 5xx + timeout.
    ///
    /// Used by [`Self::post_execute`], [`Self::post_auto_toggle`],
    /// and the `/live/*` control endpoints — every surface where a
    /// silent duplicate would change operator or exchange state.
    /// The contrast with [`Self::post_json`] (idempotent `POST
    /// /operator/events`, retry-once is safe) is deliberate: any
    /// future POST surface must pick its bucket explicitly.
    ///
    /// `idempotency_key`, when `Some`, lands as an
    /// `X-Idempotency-Key: <value>` header **in addition to**
    /// whatever shape the body carries. Engine-side proxies that
    /// redact bodies but log headers still see the dedupe key;
    /// callers who want to skip the header entirely pass `None`.
    pub async fn post_json_no_retry<B, R>(
        &self,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> Result<R, HttpError>
    where
        B: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.check_rate_budget(path)?;
        let url = self.url_for(path)?;
        let mut headers = self.auth_headers();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(key) = idempotency_key
            && let Ok(v) = HeaderValue::from_str(key)
        {
            // Lowercase form matches every other header we emit
            // (`authorization`, `content-type`, `x-zero-mode`) so
            // log pattern-matching in the engine is consistent.
            headers.insert(HeaderName::from_static("x-idempotency-key"), v);
        }
        let payload = serde_json::to_vec(body).map_err(|e| HttpError::Decode(e.to_string()))?;

        // Single send, no retry even on 502/503/504/timeout. The
        // error surfaces verbatim — the operator (or the TUI
        // shell) decides whether a human retry is appropriate.
        // Silent retry would re-open the exact failure mode
        // this entire rule exists to prevent.
        match self.post_once::<R>(url, headers, payload).await {
            Ok(v) => Ok(v),
            Err(e) => Err(self.maybe_refund_for_429(path, e)),
        }
    }

    // ── Typed endpoints ────────────────────────────────────────────

    /// `GET /` — unauthenticated version probe.
    pub async fn root(&self) -> Result<Root, HttpError> {
        self.get_json("/").await
    }

    /// `GET /health` — unauthenticated component heartbeat rollup.
    pub async fn health(&self) -> Result<Health, HttpError> {
        self.get_json("/health").await
    }

    /// `GET /hl/status[?symbol=...]` — read-only Hyperliquid info adapter status.
    pub async fn hyperliquid_status(
        &self,
        symbol: Option<&str>,
    ) -> Result<HyperliquidStatus, HttpError> {
        match symbol {
            Some(s) => {
                let path = format!("/hl/status?symbol={}", urlencoding(s));
                self.get_json(&path).await
            }
            None => self.get_json("/hl/status").await,
        }
    }

    /// `GET /hl/account` — read-only Hyperliquid account truth.
    pub async fn hyperliquid_account(&self) -> Result<HyperliquidAccount, HttpError> {
        self.get_json("/hl/account").await
    }

    /// `GET /hl/reconcile` — local runtime versus Hyperliquid account state.
    pub async fn hyperliquid_reconciliation(&self) -> Result<HyperliquidReconciliation, HttpError> {
        self.get_json("/hl/reconcile").await
    }

    /// `GET /market/quote?symbol=...` — active quote source feeding paper mode.
    pub async fn market_quote(&self, symbol: &str) -> Result<MarketQuote, HttpError> {
        let path = format!("/market/quote?symbol={}", urlencoding(symbol));
        self.get_json(&path).await
    }

    /// `GET /live/preflight` — non-secret live readiness gate.
    pub async fn live_preflight(&self) -> Result<LivePreflight, HttpError> {
        self.get_json("/live/preflight").await
    }

    /// `GET /live/certification` — dry-run live certification drills.
    pub async fn live_certification(&self) -> Result<LiveCertification, HttpError> {
        self.get_json("/live/certification").await
    }

    /// `POST /live/heartbeat` — refresh the exchange-side dead-man switch.
    pub async fn post_live_heartbeat(&self) -> Result<LiveControlResponse, HttpError> {
        self.post_json_no_retry::<serde_json::Value, LiveControlResponse>(
            "/live/heartbeat",
            &serde_json::json!({}),
            None,
        )
        .await
    }

    /// `POST /live/pause` — stop new risk-increasing live entries.
    pub async fn post_live_pause(&self) -> Result<LiveControlResponse, HttpError> {
        self.post_json_no_retry::<serde_json::Value, LiveControlResponse>(
            "/live/pause",
            &serde_json::json!({}),
            None,
        )
        .await
    }

    /// `POST /live/resume` — resume risk-increasing live entries.
    pub async fn post_live_resume(&self) -> Result<LiveControlResponse, HttpError> {
        self.post_json_no_retry::<serde_json::Value, LiveControlResponse>(
            "/live/resume",
            &serde_json::json!({}),
            None,
        )
        .await
    }

    /// `POST /live/kill` — activate kill switch and cancel open exchange orders.
    pub async fn post_live_kill(&self) -> Result<LiveControlResponse, HttpError> {
        self.post_json_no_retry::<serde_json::Value, LiveControlResponse>(
            "/live/kill",
            &serde_json::json!({}),
            None,
        )
        .await
    }

    /// `POST /live/flatten` — submit reduce-only close orders for open positions.
    pub async fn post_live_flatten(&self) -> Result<LiveControlResponse, HttpError> {
        self.post_json_no_retry::<serde_json::Value, LiveControlResponse>(
            "/live/flatten",
            &serde_json::json!({}),
            None,
        )
        .await
    }

    /// `GET /v2/status` — condensed engine summary for the status bar.
    pub async fn v2_status(&self) -> Result<V2Status, HttpError> {
        self.get_json("/v2/status").await
    }

    /// `GET /positions` — open positions for the authenticated operator.
    pub async fn positions(&self) -> Result<Positions, HttpError> {
        self.get_json("/positions").await
    }

    /// `GET /risk` — risk guardrail summary.
    pub async fn risk(&self) -> Result<Risk, HttpError> {
        self.get_json("/risk").await
    }

    /// `GET /regime` (whole-market) or `/regime?coin={coin}` (per-coin).
    pub async fn regime(&self, coin: Option<&str>) -> Result<Regime, HttpError> {
        match coin {
            Some(c) => {
                // The engine accepts `?coin=...`; we url-encode to
                // tolerate any exotic ticker forms in the future.
                let path = format!("/regime?coin={}", urlencoding(c));
                self.get_json(&path).await
            }
            None => self.get_json("/regime").await,
        }
    }

    /// `GET /brief` — morning / midday briefing.
    pub async fn brief(&self) -> Result<Brief, HttpError> {
        self.get_json("/brief").await
    }

    /// `GET /evaluate/{coin}` — per-coin gate verdict.
    pub async fn evaluate(&self, coin: &str) -> Result<Evaluation, HttpError> {
        let path = format!("/evaluate/{}", urlencoding(coin));
        self.get_json(&path).await
    }

    /// `GET /pulse?limit=...` — live engine pulse feed.
    pub async fn pulse(&self, limit: u32) -> Result<Pulse, HttpError> {
        let limit = limit.clamp(1, 100);
        let path = format!("/pulse?limit={limit}");
        self.get_json(&path).await
    }

    /// `GET /approaching` — coins approaching entry gates.
    pub async fn approaching(&self) -> Result<ApproachingFeed, HttpError> {
        self.get_json("/approaching").await
    }

    /// `GET /operator/state` — operator behavioral state snapshot
    /// (ADR-016). The classifier runs on the engine host; this call
    /// is the CLI's only window into it. Returned payload is a
    /// `zero_operator_state::Snapshot`.
    pub async fn operator_state(&self) -> Result<OperatorSnapshot, HttpError> {
        self.get_json("/operator/state").await
    }

    /// `POST /operator/events` — append one operator-state event to
    /// the engine-side classifier log (ADR-016).
    ///
    /// The wire format is the canonical [`zero_operator_state::Event`]
    /// tagged-union pinned by the cross-language golden-vector test
    /// (`crates/zero-operator-state/tests/golden_vectors.rs`). Sending
    /// via the typed `Event` rather than a hand-rolled JSON map is what
    /// keeps operators honest: a future schema change breaks the
    /// compile, not the runtime.
    ///
    /// The engine response carries the post-ingest classifier snapshot
    /// so CLI callers can (a) confirm the event landed and (b) reflect
    /// any resulting label/friction change without a second round trip.
    /// Callers that only want a fire-and-forget tag can discard the
    /// returned `OperatorEventsAccepted`.
    ///
    /// Retries are safe — see [`Self::post_json`] on idempotency.
    pub async fn post_operator_event(
        &self,
        event: &OperatorEvent,
    ) -> Result<OperatorEventsAccepted, HttpError> {
        self.post_json("/operator/events", event).await
    }

    /// `POST /execute` — composition change (live-trade surface).
    ///
    /// Mints a fresh v4 idempotency key per call, embeds it into
    /// the body, and mirrors it into an `X-Idempotency-Key` HTTP
    /// header. The server-side dedupe window suppresses a second
    /// `/execute` with the same key so a CLI retry after a
    /// spurious timeout does not double-compose.
    ///
    /// **Never retries.** The caller sees the raw error. See
    /// [`Self::post_json_no_retry`] for the policy rationale;
    /// the short version is "silent retry of a live composition
    /// change is the single worst failure mode a trading CLI can
    /// have" (M2_PLAN §7).
    ///
    /// Paper vs. live is controlled by the `X-Zero-Mode` header,
    /// which is attached automatically when the client was built
    /// with [`Self::with_mode`]. The response's `simulated` flag
    /// is engine-asserted — the CLI suffixes the operator-visible
    /// line with `(paper)` when the engine says the fill was
    /// simulated, not when the CLI "thinks" it's in paper mode.
    // `side` vs. `size` is a pedantic similar-names trip, but the
    // wire shape pins both names — renaming either one would make
    // the call site read unfamiliarly vs. the engine-side handler.
    #[allow(clippy::similar_names)]
    pub async fn post_execute(
        &self,
        coin: &str,
        side: crate::models::ExecuteSide,
        size: f64,
    ) -> Result<ExecuteResponse, HttpError> {
        let idempotency_key = mint_idempotency_key();
        let body = ExecuteRequest {
            coin: coin.to_string(),
            side,
            size,
            idempotency_key: idempotency_key.clone(),
        };
        self.post_json_no_retry::<ExecuteRequest, ExecuteResponse>(
            "/execute",
            &body,
            Some(idempotency_key.as_str()),
        )
        .await
    }

    /// `POST /auto/toggle` — flip the engine's Auto-mode flag.
    ///
    /// **Never retries.** Same rationale as [`Self::post_execute`]:
    /// the engine treats this as a composition-affecting call
    /// because it changes whether subsequent `/plan` outputs
    /// auto-accept. The response's `state` is the engine's
    /// post-call truth, not the requested state — friction may
    /// have refused the flip.
    ///
    /// The body is the small [`AutoToggleRequest`] envelope; no
    /// idempotency key is emitted because the engine treats the
    /// endpoint as naturally idempotent (flipping `on` twice is
    /// a no-op). The no-retry rule still holds — a network
    /// failure mid-flight leaves the state ambiguous, and the
    /// correct response is an operator-visible alert, not a
    /// silent duplicate.
    pub async fn post_auto_toggle(&self, enabled: bool) -> Result<AutoToggleResponse, HttpError> {
        let body = AutoToggleRequest { enabled };
        self.post_json_no_retry::<AutoToggleRequest, AutoToggleResponse>(
            "/auto/toggle",
            &body,
            None,
        )
        .await
    }

    /// `GET /rejections?limit=...[&coin=...]`.
    pub async fn rejections(
        &self,
        limit: u32,
        coin: Option<&str>,
    ) -> Result<RejectionsFeed, HttpError> {
        let limit = limit.clamp(1, 500);
        let path = match coin {
            Some(c) => format!("/rejections?limit={limit}&coin={}", urlencoding(c)),
            None => format!("/rejections?limit={limit}"),
        };
        self.get_json(&path).await
    }
}

/// Minimal URL-component encoder. We only need to escape the handful
/// of characters that appear in symbols and operator-typed strings;
/// pulling in `urlencoding` for this is overkill.
fn urlencoding(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(b));
            }
            _ => {
                // `write!` to `String` is infallible.
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn map_transport(e: &reqwest::Error) -> HttpError {
    if e.is_timeout() {
        HttpError::Timeout(TIMEOUT)
    } else {
        HttpError::Unreachable(e.to_string())
    }
}

fn is_retryable(e: &HttpError) -> bool {
    // 429 is explicitly **never** retried automatically. The
    // engine is saying "wait N seconds"; looping through that wait
    // would either ignore it (wasted traffic, worse 429s) or
    // freeze the caller silently — the exact mystery stall
    // `RateBudgetExhausted` exists to prevent. It falls out of
    // this function as part of the default-false arm, but calling
    // it out in a doc line so a future reader doesn't try to
    // "fix" the omission.
    match e {
        HttpError::Timeout(_) | HttpError::Unreachable(_) => true,
        HttpError::Status { status, .. } => matches!(
            *status,
            StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
        ),
        _ => false,
    }
}

/// Parse an HTTP `Retry-After` header value.
///
/// Per RFC 9110 §10.2.3 the value is one of:
/// - A delta-seconds integer (e.g. `120`).
/// - An HTTP-date (RFC 7231 IMF-fixdate, e.g. `Fri, 31 Dec 1999
///   23:59:59 GMT`), in which case the returned duration is the
///   difference between that date and **now** (clamped to zero).
///
/// Unparseable values return `None`; the caller substitutes a
/// safe default (today: 1 second) so a malformed header from an
/// unknown upstream proxy cannot freeze the CLI.
///
/// Both shapes are exercised by unit tests in this module; the
/// HTTP-date path uses `chrono::DateTime::parse_from_rfc2822` for
/// the IMF-fixdate format.
/// Render a `Duration` for operator consumption. Whole seconds
/// only (sub-second precision is noise on a CLI); `Duration::MAX`
/// (or anything longer than an hour — permanently-blocked shape)
/// renders as `">1h"` so an operator does not stare at a 7-digit
/// number trying to convert to wall time.
#[must_use]
pub(crate) fn format_retry_after(d: Duration) -> String {
    let secs = d.as_secs();
    if secs > 3600 {
        ">1h".to_string()
    } else {
        format!("{secs}s")
    }
}

#[must_use]
pub(crate) fn parse_retry_after(value: &str) -> Option<Duration> {
    let trimmed = value.trim();
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    // HTTP-date path — chrono's RFC-2822 parser accepts the
    // RFC-7231 IMF-fixdate shape because IMF-fixdate is a strict
    // subset of RFC-2822 with the GMT timezone pinned.
    let target = chrono::DateTime::parse_from_rfc2822(trimmed).ok()?;
    let now = chrono::Utc::now();
    let delta = target.with_timezone(&chrono::Utc) - now;
    // `to_std` fails if the delta is negative — in that case the
    // date is in the past, so the effective retry-after is zero.
    Some(delta.to_std().unwrap_or(Duration::ZERO))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

/// Mint a fresh v4 UUID for use as an `/execute` idempotency key.
///
/// v4 (random) over v1 (time-based) deliberately: we want two
/// CLIs firing `/execute` at the same millisecond from the same
/// host to produce distinct keys without coordinating on a
/// counter. The engine-side dedupe window is short (seconds);
/// collision probability at that horizon is astronomically low
/// even across a fleet of operators, and v4 keeps the key a
/// pure random string with no embedded host / time signal.
///
/// Exposed at the module boundary (pub-in-crate) so the
/// integration tests can exercise the shape independently of the
/// `/execute` call path; the `/execute` helper is the sole
/// production caller.
#[must_use]
pub(crate) fn mint_idempotency_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[must_use]
pub const fn retry_delay() -> Duration {
    RETRY_DELAY
}

#[must_use]
pub const fn timeout() -> Duration {
    TIMEOUT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_parses_plain_seconds() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("  30  "), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("0"), Some(Duration::from_secs(0)));
    }

    #[test]
    fn retry_after_parses_http_date_in_the_future() {
        // A date well in the future must yield a non-zero duration.
        // We don't pin the exact value because `chrono::Utc::now()`
        // is unavoidable here — just that the result is positive and
        // comfortably less than a year.
        let one_year_ahead = chrono::Utc::now() + chrono::Duration::days(365);
        let formatted = one_year_ahead
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let d = parse_retry_after(&formatted).expect("parseable");
        assert!(d > Duration::from_secs(86_400));
        assert!(d < Duration::from_secs(366 * 86_400));
    }

    #[test]
    fn retry_after_clamps_past_date_to_zero() {
        let past = chrono::Utc::now() - chrono::Duration::days(3);
        let formatted = past.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert_eq!(parse_retry_after(&formatted), Some(Duration::ZERO));
    }

    #[test]
    fn retry_after_unparseable_returns_none() {
        assert_eq!(parse_retry_after("not-a-date"), None);
        assert_eq!(parse_retry_after(""), None);
    }

    #[test]
    fn rate_limit_source_display_is_stable() {
        // Log consumers grep on these tags; rename = breaking.
        assert_eq!(format!("{}", RateLimitSource::CliBudget), "cli-budget");
        assert_eq!(format!("{}", RateLimitSource::Engine429), "engine-429");
    }

    #[test]
    fn rate_budget_exhausted_display_is_terse_and_seconds() {
        // Copy-tested shape. Widened renders (origin tags, longer
        // nouns) belong in logs, not in the operator's pane row.
        let e = HttpError::RateBudgetExhausted {
            retry_after: Duration::from_secs(3),
            origin: RateLimitSource::CliBudget,
        };
        assert_eq!(format!("{e}"), "rate: exhausted — retry in 3s");

        let e429 = HttpError::RateBudgetExhausted {
            retry_after: Duration::from_secs(45),
            origin: RateLimitSource::Engine429,
        };
        // CLI-vs-engine origin must be invisible to the operator.
        assert_eq!(format!("{e429}"), "rate: exhausted — retry in 45s");

        let perma = HttpError::RateBudgetExhausted {
            retry_after: Duration::MAX,
            origin: RateLimitSource::CliBudget,
        };
        assert_eq!(format!("{perma}"), "rate: exhausted — retry in >1h");
    }

    #[test]
    fn mode_header_value_is_stable() {
        // Engine-side log ingestion greps on these literal strings
        // via `X-Zero-Mode`. Any rename lands on the wire as a
        // mode regression — the test locks the exact bytes.
        assert_eq!(Mode::Paper.as_header_value(), "paper");
        assert_eq!(Mode::Live.as_header_value(), "live");
    }

    #[test]
    fn with_mode_attaches_header_on_auth_headers() {
        // The `auth_headers` helper is the single request-assembly
        // site (verified by its call-sites in `get` / `post`); a
        // mode override on the client must surface there so every
        // request carries the header without an opt-in per call.
        let client = HttpClient::new("https://example.test", None)
            .expect("client")
            .with_mode(Mode::Paper);
        assert_eq!(client.mode(), Some(Mode::Paper));
        let headers = client.auth_headers();
        let got = headers
            .get("x-zero-mode")
            .expect("x-zero-mode header attached");
        assert_eq!(got.to_str().unwrap(), "paper");

        // Default client emits no mode header — absence is how the
        // engine reads "respect launch-time mode."
        let unset = HttpClient::new("https://example.test", None).expect("client");
        assert!(unset.mode().is_none());
        assert!(unset.auth_headers().get("x-zero-mode").is_none());
    }

    #[test]
    fn mint_idempotency_key_is_unique_per_call() {
        // Pins the "fresh key per call" rule at the unit level so a
        // future refactor that accidentally caches the key (e.g.
        // `OnceCell`) breaks here, not in a flaky integration test
        // that only sometimes trips on the dedupe window.
        let a = mint_idempotency_key();
        let b = mint_idempotency_key();
        assert_ne!(a, b, "successive calls must mint distinct keys");
        assert_eq!(a.len(), 36, "UUID v4 stringifies to 36 chars");
        assert_eq!(a.matches('-').count(), 4, "four hyphens in v4 form");
    }

    #[test]
    fn is_retryable_never_retries_rate_budget_exhausted() {
        // Explicit negative: looping on 429 instead of surfacing
        // the typed refusal is the exact failure mode the
        // exhausted variant exists to prevent.
        let err = HttpError::RateBudgetExhausted {
            retry_after: Duration::from_secs(2),
            origin: RateLimitSource::Engine429,
        };
        assert!(!is_retryable(&err));
    }
}
