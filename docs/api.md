# API Contract

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
- `GET /hl/status`, `/market/quote`
- `GET /operator/state`
- `POST /execute`
- `POST /auto/toggle`
- `POST /operator/events`

`POST /execute` always runs through `PaperEngine.submit`, records a decision
with source `api:/execute`, and returns `simulated=true`. It honors the request
idempotency key so repeated submissions with the same key do not create
duplicate paper fills.

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

`GET /hl/status` returns disabled metadata by default. When `zero-paper-api
--hyperliquid` is used, it queries Hyperliquid's public info endpoint for
read-only mids and returns `secrets_required=false`. This endpoint must not
place orders, sign payloads, or require exchange credentials.

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
