# Market Data Adapters

ZERO market data adapters expose deterministic market observations to
strategies and the paper runtime. They must not place orders, manage custody,
or bypass risk checks.

The boundary is:

1. Adapter returns `Candle` data.
2. Strategy reads the adapter and returns `StrategySignal` or `None`.
3. ZERO converts the signal to an `OrderIntent`.
4. `PaperEngine.submit` applies risk limits and records the decision.

## Adapter Contract

Use `MarketDataAdapterMetadata` and implement `candles` plus `latest`:

```python
from dataclasses import dataclass

from zero_engine import Candle, MarketDataAdapterMetadata


@dataclass(frozen=True)
class MyAdapter:
    metadata: MarketDataAdapterMetadata = MarketDataAdapterMetadata(
        name="my-adapter",
        version="0.1.0",
        description="Deterministic paper example.",
        source="fixture",
    )

    def candles(self, symbol: str, limit: int | None = None) -> tuple[Candle, ...]:
        ...

    def latest(self, symbol: str) -> Candle:
        ...
```

## Safety Rules

Public market data adapters must:

- return `Candle` objects in chronological order;
- normalize symbols consistently;
- avoid private keys, wallet secrets, and exchange credentials;
- expose clear metadata;
- fail closed when a symbol or feed is unavailable;
- include deterministic tests.

Public market data adapters must not:

- place orders;
- call live execution endpoints;
- mutate engine state;
- publish operator data;
- silently fall back from live data to fixtures.

## Example

See [examples/market-data-adapter](../examples/market-data-adapter).

Run:

```bash
just market-data-adapter-example
```

Before opening a pull request:

```bash
just ci
```
