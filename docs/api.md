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
- `GET /metrics`, `/immune`, `/memory`, `/genesis`, `/audit/export`
- `GET /deployment/claim`, `/deployment/heartbeat`, `/network/profile`, `/network/leaderboard`
- `GET /intelligence/snapshot`, `/intelligence/catalog`, `/intelligence/commercial`, `/intelligence/model-gateway`
- `GET /v1/intelligence/snapshots`, `/v1/intelligence/history`, `/v1/intelligence/cohorts`, `/v1/intelligence/benchmarks`
- `GET /hl/status`, `/hl/account`, `/hl/reconcile`, `/market/quote`
- `GET /live/preflight`, `/live/cockpit`, `/live/certification`, `/live/receipts`, `/live/evidence`
- `GET /operator/state`
- `POST /execute`
- `POST /auto/toggle`
- `POST /operator/events`
- `POST /network/publish`
- `POST /network/ingest`
- `POST /intelligence/export`
- `POST /v1/intelligence/webhooks`, `/v1/intelligence/exports`
- `POST /live/heartbeat`, `/live/pause`, `/live/resume`, `/live/kill`, `/live/flatten`

`POST /execute` runs through `PaperEngine.submit`, records a decision with
source `api:/execute`, and returns `simulated=true` by default. When the caller
sends `X-Zero-Mode: live`, the same endpoint routes to the optional live
executor instead. If no live executor is configured, the engine returns
`accepted=false`, `simulated=false`, and `reason="live executor not configured"`.
It honors the request idempotency key so repeated submissions with the same key
do not create duplicate paper fills or duplicate live order submissions.
Live responses include public-safe `request_hash` and `receipt_hash` fields
when the live executor records the attempt; the raw idempotency key remains
local to the response and is excluded from public evidence packets.
The interactive CLI command `/execute <coin> <buy|sell> <size>` uses this same
endpoint after the operator-state friction ladder clears. Non-interactive
`zero run` refuses risk-increasing execution commands because the typed-confirm
surface is TTY-only.

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

`GET /memory?limit=20` returns a `zero.memory.snapshot.v1` public-safe view of
local memory. Without a configured memory store, the endpoint extracts an
ephemeral snapshot from current paper decisions. With a `MemoryStore`, it reads
active append-only memory entries and omits expired records. `format=md` also
returns generated `knowledge.md` content. Memory output is explicitly redacted:
no live prices, wallet material, exchange order ids, or private keys.

`GET /genesis` returns a `zero.genesis.snapshot.v1` plan-only view of genesis
proposal classifications. It never applies code changes. The fixture-backed
snapshot demonstrates one accepted proposal, one rejected proposal with
insufficient sample size, and one escalated proposal touching protected live
execution paths. Protected execution, sizing, stops, circuit breaker, live
adapter, and immune-core proposals require human review.

`GET /audit/export?limit=100` returns a structured `zero.audit.v1` export with
runtime summary, retention/redaction metadata, metrics, recovery state, and the
most recent decisions. The public paper runtime records no secrets.

`zero-engine-run --runtime-bus DIR` writes checksum-chained local runtime events
to `DIR/events.jsonl` and a fast boot snapshot to `DIR/state-snapshot.json`.
The bus is not an HTTP API yet; it is the local event contract for OODA cycles,
decisions, fills, rejections, positions, health, and future operator commands.
See [runtime-bus.md](runtime-bus.md).

`GET /deployment/claim` returns a `zero.deployment.claim.v1` public-safe,
signature-ready identity packet for the local runtime. It binds deployment
metadata, operator audit handle, aggregate evidence counts, and signature status
without including raw decisions, symbols, trace IDs, idempotency keys, wallet
material, or exchange credentials.

`GET /deployment/heartbeat` returns a `zero.deployment.heartbeat.v1`
public-safe liveness packet bound to the deployment claim hash. It exposes
paper-only, fresh, or expired dead-man state without exposing raw decisions,
trace IDs, idempotency keys, credentials, or private runtime details.

`GET /network/profile` returns a `zero.network.profile.v1` public-safe profile
packet with aggregate behavior, verification badges, a proof hash, deployment
claim hash, deployment heartbeat hash, and privacy metadata. It excludes raw
decisions, trace IDs, idempotency keys, wallet addresses, exchange order IDs,
private notes, strategy source labels, and per-trade symbols. Publication is
disabled by default.

`GET /network/leaderboard` returns a `zero.network.leaderboard.v1` local row
derived from the same redacted profile. The first leaderboard model ranks
verified process data such as decision count and rejection rate, not PnL
screenshots.

`POST /network/publish` requires `{"consent": true}` and
`ZERO_NETWORK_PUBLISH_PATH`. When both are present, the runtime appends the
redacted profile packet to a local JSONL proof log. It does not upload to a
ZERO-hosted service from the public runtime.

`POST /network/ingest` accepts one redacted profile packet or a list of
packets and returns `zero.network.ingestion.v1`. It is the public-safe
hosted-compatible validation contract for ZERO Network: profile publication
must be explicitly enabled, proof hashes must match recomputed aggregate
evidence, aggregate metrics must be internally consistent, duplicate accepted
handles/proofs are refused, and deployment claim/heartbeat hashes must bind
when present. The response includes accepted/refused records plus an
accepted-only leaderboard. It never requires raw journals, exchange
credentials, wallet material, trace IDs, idempotency keys, or per-trade
symbols.

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

`GET /intelligence/commercial` returns
`zero.intelligence.commercial.v1`, the billing-ready hosted Intelligence API
contract. It names the open/commercial/not-sold boundary, bearer-token hosted
auth shape, plan scopes, datasets, endpoint usage events, rate-limit headers,
webhook delivery contract, export rules, reliability tiers, and privacy rules.
It is not enforced by the local open-source runtime and does not require
operator secrets, raw journals, custody transfer, or exchange credentials.

The `/v1/intelligence/*` endpoints are a hosted-compatible reference surface
for ZERO Intelligence API. They are intentionally small and runnable inside the
public paper server so contributors can build clients against the commercial
shape before the production warehouse exists:

- `GET /v1/intelligence/snapshots` serves delayed public snapshots without a
  token and returns real `x-zero-ratelimit-*` headers.
- `GET /v1/intelligence/snapshots?freshness=realtime`,
  `/v1/intelligence/history`, `/v1/intelligence/cohorts`, and
  `/v1/intelligence/benchmarks` require `Authorization: Bearer <token>` when
  `ZERO_INTELLIGENCE_API_TOKEN` is configured.
- `POST /v1/intelligence/webhooks` returns a subscription record plus a
  verifiable HMAC-SHA256 signature fixture. It never returns signing key
  material.
- `POST /v1/intelligence/exports` returns a reference export job for aggregate
  JSONL or CSV data.

Reference env vars:

```text
ZERO_INTELLIGENCE_API_TOKEN=...
ZERO_INTELLIGENCE_API_PLAN=team_fund
ZERO_INTELLIGENCE_API_ACCOUNT_ID=acct_...
ZERO_INTELLIGENCE_WEBHOOK_SIGNING_KEY=...
```

These endpoints prove auth boundaries, scopes, usage events, rate-limit
headers, and webhook signing behavior. They do not imply the local runtime is a
hosted billing system or a historical data warehouse.

`GET /intelligence/model-gateway` returns `zero.model_gateway.status.v1`, a
public-safe provider and routing status packet. The default mode is
`fail_closed`: no configured provider means no model-derived certainty. Mock
providers report `local_ready`; explicitly configured external providers report
`external_ready`. All model output is advisory-only and never bypasses
execution safety. Provider status also reports bounded retry and timeout policy;
usage counters include attempts, token counts when available, and public-safe
cost-estimate source.

`GET /intelligence/model-gateway/health` returns
`zero.model_gateway.health.v1`. By default it is a config-only health probe and
does not make network calls. Passing `?network=true` runs an explicit structured
provider probe through the selected model boundary and returns only provider,
attempt, token-count, and status metadata.

`GET /intelligence/model-gateway/audit` returns
`zero.model_gateway.audit.v1`, a production-readiness bundle for model
operations. It includes status, config-only health, usage totals, fail-closed
controls, evidence requirements, and privacy assertions without prompts, raw
outputs, headers, request IDs, or secret values.

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

`GET /live/cockpit` returns a consolidated `zero.live_cockpit.v1` operator
packet. It joins preflight failures, immune breakers, reconciliation status,
dry-run certification, heartbeat expiry, recent live records, operator actions,
the resolved `zero.operator_context.v1` audit identity, and the next required
action. It is read-only and safe to expose in diagnostics.

`GET /live/receipts` returns `zero.live_execution_receipts.v1`: a local,
public-safe receipt bundle for live executor attempts. Each receipt includes
the exact order intent plus hashes for the request, operator context, trace
token, idempotency token, and venue acknowledgement. It never includes wallet
material, exchange credentials, raw venue responses, trace tokens, or raw
idempotency tokens.

`GET /live/evidence` returns a public-safe `zero.live_evidence.v1` packet for
supervised tiny-capital canary rehearsal. It hashes preflight, cockpit,
live execution receipts, reconciliation, immune, certification, audit export,
deployment claim, and deployment heartbeat artifacts instead of embedding raw
private data. Set
`ZERO_LIVE_EVIDENCE_SIGNING_KEY` to attach a local HMAC-SHA256 signature without
echoing key material.

`scripts/live_canary_rehearsal.py URL --mode refusal` is the maintained local
collector for the canary path. It captures preflight, heartbeat, cockpit,
certification, reconciliation, fail-closed live execute evidence when the
engine is not ready, receipts, live evidence, metrics, audit export, manifest,
and `SHA256SUMS`. `--mode canary` can submit a real live order only after an
explicit confirmation string and ready live gates.

`scripts/live_canary_verify.py DIR` verifies the local bundle before it is used
as launch evidence. It recomputes `SHA256SUMS`, checks required packets and
status codes, compares manifest receipt/evidence fields to the packet payloads,
and fails on common unredacted trace/idempotency/token shapes.

`scripts/live_canary_exchange_evidence.py DIR hyperliquid-export.json` attaches
public-safe exchange-side evidence to the same canary bundle. It hashes the raw
source file, hashes raw venue identifiers, normalizes only symbol/side/quantity
and optional price, matches accepted ZERO receipts by symbol/side/quantity, and
refreshes `SHA256SUMS`. `scripts/live_canary_verify.py DIR
--require-exchange-evidence` then fails unless the packet is present and every
accepted ZERO receipt has exchange-side evidence.

`scripts/live_canary_operator.py URL --mode refusal` wraps the full public-safe
operator workflow: collect rehearsal bundle, attach exchange evidence, run the
verifier, and write `operator_report.json`. For accepted live canary receipts,
the operator workflow refuses to produce a passing report unless a matching
Hyperliquid order/fill export is attached.

`scripts/live_canary_operator_verify.py DIR` independently verifies the
operator workflow directory. It checks recursive `SHA256SUMS`, operator-report
privacy flags, common redaction leaks, accepted-live exchange-evidence rules,
and the nested canary bundle verifier.

`scripts/live_cockpit_drill.py URL` collects the read-only live cockpit stack
into a public-safe drill bundle. It captures `/health`, `/v2/status`,
`/live/preflight`, `/live/cockpit`, `/immune`, `/hl/reconcile`,
`/live/certification`, `/live/receipts`, `/live/evidence`, `/metrics`, and
`/audit/export?limit=100`, then writes `manifest.json` and `SHA256SUMS`. In
public paper mode it fails unless live readiness remains fail-closed.
`scripts/live_cockpit_drill_verify.py DIR` independently verifies a captured
bundle by recomputing checksums, checking packet schemas, replaying the
manifest summary from packet payloads, and enforcing redaction rules.
`scripts/live_cockpit_drill_tamper_rehearsal.py DIR` proves the verifier
rejects both checksum drift and semantic cockpit tampering before the bundle is
used as operator-readiness evidence.

`GET /operator/context` returns the operator audit identity currently attached
to requests. The engine resolves it from `X-Zero-Operator-*` headers,
`ZERO_OPERATOR_*` environment variables, or the local default. Live control
responses and `/audit/export` include the same packet so incidents can be
reviewed per operator without recording secrets.

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
