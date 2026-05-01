# Strategy Plugins

ZERO strategy plugins are paper-first proposal modules. A plugin may inspect
market data and return a `StrategySignal`; it must not place orders, manage
custody, bypass risk checks, or call live execution endpoints.

The runtime keeps the safety boundary:

1. Plugin reads market data.
2. Plugin returns `StrategySignal` or `None`.
3. ZERO converts the signal to an `OrderIntent`.
4. `PaperEngine.submit` applies risk limits.
5. The engine records accepted and rejected decisions.

## Plugin Contract

Use `StrategyPluginMetadata` and implement `propose`:

```python
from dataclasses import dataclass

from zero_engine import MarketDataAdapter, Side, StrategyPluginMetadata, StrategySignal


@dataclass(frozen=True)
class MyPlugin:
    metadata: StrategyPluginMetadata = StrategyPluginMetadata(
        name="my-plugin",
        version="0.1.0",
        description="Paper-only example.",
    )

    def propose(self, market: MarketDataAdapter, symbol: str) -> StrategySignal | None:
        latest = market.latest(symbol)
        if latest.close <= latest.open:
            return None
        return StrategySignal(
            symbol=latest.symbol,
            side=Side.BUY,
            confidence=0.8,
            quantity=0.01,
            reason="close above open",
        )
```

Submit through the helper so metadata validation always runs:

```python
order = propose_plugin_order(plugin, market, "BTC")
decision = engine.submit(order, source=f"strategy-plugin:{plugin.metadata.name}")
```

## Safety Rules

Public plugins must:

- set `paper_only=True`;
- produce deterministic output from deterministic fixtures;
- include tests;
- name their decision source as `strategy-plugin:<name>`;
- keep all exchange and wallet access outside plugin code.

Public plugins must not:

- read private keys or wallet secrets;
- submit orders directly;
- call `POST /execute` with `X-Zero-Mode: live`;
- start background processes;
- publish operator data.

## Example

See [examples/strategy-plugin](../examples/strategy-plugin).

Run:

```bash
just strategy-plugin-example
```

Before opening a pull request:

```bash
just ci
```
