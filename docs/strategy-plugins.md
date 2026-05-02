# Strategy Runners And Plugins

ZERO strategy runners and plugins are paper-first proposal modules. They may
inspect market data and return a `StrategySignal`; they must not place orders,
manage custody, bypass risk checks, or call live execution endpoints.

The runtime keeps the safety boundary:

1. Runner or plugin reads market data.
2. Runner or plugin returns `StrategySignal` or `None`.
3. ZERO converts the signal to an `OrderIntent`.
4. `PaperEngine.submit` applies risk limits.
5. The engine records accepted and rejected decisions.

## Declarative Runner Contract

Declarative runners are the preferred first contribution path because they are
small, deterministic, and easy to review.

```yaml
name: close-strength-yaml
version: 0.1.0
description: Declarative close-above-open paper runner.
paper_only: true
symbol: BTC
side: buy
quantity: 0.01
confidence: 0.8
condition:
  type: close_above_open
  min_move_pct: 0.01
```

Load and submit through the helper:

```python
runner = load_strategy_runner("examples/strategy-runner/close-strength.yaml")
order = propose_runner_order(runner, market, "BTC")
decision = engine.submit(order, source=f"strategy-runner:{runner.metadata.name}")
```

Run the example:

```bash
just strategy-runner-example
```

The current dependency-free YAML subset supports simple `key: value` pairs and
one nested mapping for `condition`. Use JSON if the strategy needs a richer
shape.

## Plugin Contract

Use `StrategyPluginMetadata` and implement `propose` when the strategy needs
Python code:

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

Public runners and plugins must:

- set `paper_only=True`;
- produce deterministic output from deterministic fixtures;
- include tests;
- name their decision source as `strategy-runner:<name>` or
  `strategy-plugin:<name>`;
- keep all exchange and wallet access outside plugin code.

Public runners and plugins must not:

- read private keys or wallet secrets;
- submit orders directly;
- call `POST /execute` with `X-Zero-Mode: live`;
- start background processes;
- publish operator data.

## Example

See [examples/strategy-runner](../examples/strategy-runner) and
[examples/strategy-plugin](../examples/strategy-plugin).

Run:

```bash
just strategy-runner-example
just strategy-plugin-example
```

Before opening a pull request:

```bash
just ci
```
