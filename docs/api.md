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
