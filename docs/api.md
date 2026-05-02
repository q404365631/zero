# API Contract

The machine-readable contract lives in
[openapi/zero-paper-api.v1.yaml](../openapi/zero-paper-api.v1.yaml). Compatibility
rules live in [docs/api-compatibility.md](api-compatibility.md).

The first public API contract is deliberately small. It describes the behavior
that must remain stable while the private engine is ported into the public
runtime.

## Engine Objects

### `RiskLimits`

Defines hard local safety limits:

- `max_notional_usd`
- `max_position_notional_usd`
- `max_leverage`
- `min_confidence`

All values must be positive. `min_confidence` must be less than or equal to `1`.

### `OrderIntent`

Represents a requested order:

- `symbol`
- `side`
- `quantity`
- `price`
- `confidence`
- `reduce_only`

`reduce_only=true` means the order can only reduce risk. The public safety
contract allows reduce-only orders even when confidence is low.

### `RiskDecision`

Returned by safety evaluation:

- `allowed`
- `reason`

The reason must be human-readable and stable enough for tests, logs, and CLI
rendering.

## Safety Functions

### `evaluate_order(intent, limits, current=None)`

Evaluates a proposed order before paper fill.

Rejects when:

- confidence is below `limits.min_confidence`
- order notional exceeds `limits.max_notional_usd`
- projected position notional exceeds `limits.max_position_notional_usd`

Allows when:

- order is reduce-only
- all risk checks pass

### `projected_position(intent, current=None)`

Computes the paper position that would exist if the order were accepted.

## Paper Engine

### `PaperEngine.submit(intent)`

Evaluates the intent, records rejected orders, records accepted fills, and
updates paper positions.

The method must not require exchange credentials or network access.

`submit(intent, source="manual")` returns a `RiskDecision` and appends a
`DecisionRecord` to `engine.decisions`.

### `DecisionRecord`

Inspectable paper decision log entry:

- `intent`
- `decision`
- `as_of`
- `source`

Use `to_dict()` for JSON output in examples, tests, and CLI inspection. Every
paper decision should name its source, such as `manual`, `scenario:<name>`, or
`strategy:<name>`.

`PaperEngine` accepts an optional `clock` callable so tests and examples can
produce deterministic `as_of` timestamps.

## Local Paper API

`zero-paper-api` starts a paper-only HTTP server on `127.0.0.1:8765`.
It requires no secrets and implements the CLI-facing subset of the engine
contract:

- `GET /`, `/health`, `/v2/status`
- `GET /positions`, `/risk`, `/brief`
- `GET /regime`, `/evaluate/{coin}`, `/pulse`, `/approaching`, `/rejections`, `/journal`
- `GET /metrics`, `/immune`, `/audit/export`
- `GET /network/profile`, `/network/leaderboard`
- `GET /intelligence/snapshot`, `/intelligence/catalog`
- `GET /hl/status`, `/hl/account`, `/hl/reconcile`, `/market/quote`
- `GET /live/preflight`, `/live/certification`
- `GET /operator/state`
- `POST /execute`
- `POST /auto/toggle`
- `POST /operator/events`
- `POST /network/publish`
- `POST /intelligence/export`
- `POST /live/heartbeat`, `/live/pause`, `/live/resume`, `/live/kill`, `/live/flatten`

`POST /execute` runs through `PaperEngine.submit`, records a decision with
source `api:/execute`, and returns `simulated=true` by default. When the caller
sends `X-Zero-Mode: live`, the same endpoint routes to the optional live
executor instead. If no live executor is configured, the engine returns
`accepted=false`, `simulated=false`, and `reason="live executor not configured"`.
It honors the request idempotency key so repeated submissions with the same key
do not create duplicate paper fills or duplicate live order submissions.

Every HTTP response carries `X-Zero-Trace-Id`. When an HTTP `POST /execute`
creates a paper decision, that trace ID is written into the decision journal
and echoed in the HTTP response. Idempotency replays return the original
decision trace, which keeps audit trails tied to the first accepted action.

`GET /journal?limit=50` returns the most recent paper decisions in the same
shape as `DecisionRecord.to_dict()`. When `zero-paper-api --journal PATH` is
used, records are read from the append-only JSONL journal at `PATH`; otherwise
the endpoint returns the in-memory decision log for the current process.

On startup with `--journal PATH`, `zero-paper-api` replays the journal before it
serves traffic. The replay restores paper decisions, fills, open positions,
rejections, and idempotency keys, so a repeated `POST /execute` with an already
journaled key returns the original simulated response instead of creating a
duplicate fill. `/health` and `/v2/status` include a `recovery` object with the
journal mode, recovered counts, and current runtime counts.

`GET /metrics` returns JSON runtime counters for API requests, status codes,
execute outcomes, idempotency hits, decision counts, fill counts, rejection
counts, and recovery state.

`GET /audit/export?limit=100` returns a structured `zero.audit.v1` export with
runtime summary, retention/redaction metadata, metrics, recovery state, and the
most recent decisions. The public paper runtime records no secrets.

`zero-engine-run --runtime-bus DIR` writes checksum-chained local runtime events
to `DIR/events.jsonl` and a fast boot snapshot to `DIR/state-snapshot.json`.
The bus is not an HTTP API yet; it is the local event contract for OODA cycles,
decisions, fills, rejections, positions, health, and future operator commands.
See [runtime-bus.md](runtime-bus.md).

`GET /network/profile` returns a `zero.network.profile.v1` public-safe profile
packet with aggregate behavior, verification badges, a proof hash, and privacy
metadata. It excludes raw decisions, trace IDs, idempotency keys, wallet
addresses, exchange order IDs, private notes, strategy source labels, and
per-trade symbols. Publication is disabled by default.

`GET /network/leaderboard` returns a `zero.network.leaderboard.v1` local row
derived from the same redacted profile. The first leaderboard model ranks
verified process data such as decision count and rejection rate, not PnL
screenshots.

`POST /network/publish` requires `{"consent": true}` and
`ZERO_NETWORK_PUBLISH_PATH`. When both are present, the runtime appends the
redacted profile packet to a local JSONL proof log. It does not upload to a
ZERO-hosted service from the public runtime.

`GET /intelligence/snapshot` returns a `zero.intelligence.snapshot.v1` delayed
public intelligence packet derived from the verified ZERO Network profile. It
contains aggregate signals such as activity level, rejection discipline,
execution pressure, journal quality, and proof hash. It excludes raw decisions,
trace IDs, idempotency keys, per-trade symbols, credentials, and private notes.

`GET /intelligence/catalog` returns a `zero.intelligence.catalog.v1` commercial
API contract. The contract makes the open-core rule explicit: the runtime,
paper mode, self-custodial operation, public profiles, public leaderboards, and
delayed snapshots are public; realtime feeds, history, cohorts, benchmarks,
webhooks, bulk exports, commercial redistribution, and SLOs are paid surfaces.

`POST /intelligence/export` requires `{"consent": true}` and
`ZERO_INTELLIGENCE_EXPORT_PATH`. When both are present, the runtime appends the
redacted delayed snapshot to a local JSONL packet log for future hosted
ingestion. It does not upload to a ZERO-hosted service from the public runtime.

`GET /live/preflight` returns a structured `zero.live_preflight.v1` readiness
packet for the Hyperliquid live executor. It never accepts private keys over
HTTP. The runtime reads local process configuration only, redacts key
diagnostics, verifies account-read access when a wallet address and read adapter
are present, validates a dry-run order locally, and refuses live mode unless
custody, journal, risk limits, and emergency controls are all present.

Live execution is local opt-in. Install the optional dependency group and start
the API with process-local credentials:

```bash
pip install -e "engine[live]"
ZERO_LIVE_EXECUTION_ENABLED=true \
ZERO_HYPERLIQUID_WALLET_ADDRESS=0x... \
ZERO_HYPERLIQUID_API_PRIVATE_KEY=0x... \
ZERO_LIVE_MAX_NOTIONAL_USD=1000 \
ZERO_LIVE_MAX_DAILY_LOSS_USD=250 \
ZERO_LIVE_MAX_ORDERS_PER_MINUTE=6 \
ZERO_LIVE_DEAD_MAN_TIMEOUT_S=30 \
zero-paper-api --journal .zero/decisions.jsonl --hyperliquid-live-prices
```

The live executor enforces:

- deterministic idempotency keys and exchange client order IDs;
- no automatic retry on order-submission POSTs;
- exchange dead-man heartbeats through `POST /live/heartbeat`;
- `POST /live/pause` and `/live/resume` for entry control;
- `POST /live/kill` for kill switch plus open-order cancellation;
- `POST /live/flatten` for reduce-only close orders;
- max notional, max daily loss, and max orders per minute.

`GET /live/certification` returns a dry-run `zero.live_certification.v1`
evidence packet. It runs fake-exchange drills for heartbeat, idempotency,
exchange outage, pause, reduce-only flatten, kill, rejected dead-man scheduling,
rate limit, and daily-loss behavior. It never needs secrets and must report
`orders_placed_live=0`.

`GET /immune` returns the `zero.immune.v1` breaker packet used by live
preflight and the CLI. It names each risk-blocking breaker, whether new risk is
allowed, and the evidence behind open breakers. Risk-reducing live controls
remain available even when `/immune` reports `risk_increasing_allowed=false`.

Public Railway paper deployments should not set live credentials. Their
expected behavior is `ready=false`, `live_mode=refused`, and `POST /live/*`
returning `ok=false` with `reason="live executor not configured"`.

`GET /hl/status` returns disabled metadata by default. When `zero-paper-api
--hyperliquid` is used, it queries Hyperliquid's public info endpoint for
read-only mids and returns `secrets_required=false`. This endpoint must not
place orders, sign payloads, or require exchange credentials.

`GET /hl/account` returns a normalized `zero.hl_account.v1` snapshot for the
configured `ZERO_HYPERLIQUID_WALLET_ADDRESS`: redacted wallet id, account
value, margin used, withdrawable balance, open positions, and open orders. It
uses read-only Hyperliquid info calls only.

`GET /hl/reconcile` returns a `zero.reconciliation.v1` packet comparing local
runtime positions with the Hyperliquid account snapshot. Status values include
`ok`, `not_configured`, `stale_data`, `local_lag`, `exchange_rejection`, and
`critical_mismatch`. Live risk-increasing `POST /execute` calls are refused
unless reconciliation reports `risk_increasing_allowed=true`; reduce-only
controls remain available for risk reduction.

`GET /market/quote?symbol=BTC` returns the price source currently feeding paper
mode. By default it returns deterministic fixture prices with `source=paper:static`.
When `zero-paper-api --hyperliquid-live-prices` is used, it returns cached
Hyperliquid mids with `source=hyperliquid:allMids`; `POST /execute`,
`GET /evaluate/{coin}`, and `GET /positions` use the same quote path. Missing
symbols or unavailable live market data fail closed instead of falling back to
fixtures.

### Contract Fixtures

Shared JSON fixtures live in `contracts/paper-api/`. Python tests assert the
local paper API emits these exact payloads, and Rust client tests deserialize the
same files into `zero-engine-client` models. Any endpoint change that affects
the CLI contract should update the fixture and both test expectations together.

The fixture set currently covers:

- `GET /v2/status`
- `GET /positions`
- `GET /risk`
- `GET /brief`
- `GET /rejections`
- `POST /execute` accepted and rejected responses

## Paper Scenarios

### `load_scenario(path)`

Loads a deterministic paper scenario from JSON.

Required behavior:

- `mode` must be `paper`.
- At least one order must be present.
- Symbols are normalized to uppercase.
- Side values are parsed as `buy` or `sell`.

The public examples use this path so contributor demos are data-driven and easy
to extend without touching engine code.

## Market Data Adapters

### `Candle`

Immutable OHLCV candle:

- `symbol`
- `ts`
- `open`
- `high`
- `low`
- `close`
- `volume`

Validation keeps public fixtures honest: prices must be positive, high/low must
bound open and close, and volume must be non-negative.

### `MarketDataAdapter`

Protocol for deterministic market data:

- `candles(symbol, limit=None)`
- `latest(symbol)`

Adapters must return candles in chronological order and must not require secrets
for paper examples.

### `JsonlCandleAdapter`

Loads newline-delimited JSON candles from disk. This is the public template for
future adapters because it is deterministic, reviewable, and CI-friendly.

## Strategy Plugins

### `StrategySignal`

Paper-only proposal object:

- `symbol`
- `side`
- `confidence`
- `quantity`
- `reason`

A signal is not a fill. It must be converted to an `OrderIntent` and submitted
through `PaperEngine.submit`.

### `Strategy`

Protocol for paper strategies:

- `name`
- `propose(market, symbol)`

Strategies receive a `MarketDataAdapter` and return either a `StrategySignal` or
`None`.

### `MomentumStrategy`

Minimal deterministic template. It proposes a buy signal when the latest candle
closes above its open by at least `min_move_pct`.

### `propose_order(strategy, market, symbol)`

Converts a strategy proposal into an `OrderIntent` priced at the latest candle
close. The returned order still needs safety evaluation.

## Compatibility Rules

- Additive fields are preferred over breaking changes.
- Reasons used by tests should not be renamed casually.
- Paper-mode behavior must stay deterministic.
- Live adapters must not change paper-mode defaults.
