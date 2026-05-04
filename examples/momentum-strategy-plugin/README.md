# Paper Momentum Strategy Plugin

This example is a deterministic, paper-only strategy plugin for contributors.
It reads local candle fixtures, proposes a `StrategySignal` for momentum that
clears the threshold, and returns `None` when the setup is too weak or has too
little data.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src:$PWD/examples/momentum-strategy-plugin" \
  python3 examples/momentum-strategy-plugin/run.py
```

Or use:

```bash
just momentum-strategy-plugin-example
```

Expected output shape:

```json
{
  "mode": "paper",
  "plugin": {
    "name": "paper-momentum",
    "paper_only": true,
    "version": "0.1.0"
  },
  "signals": [
    {
      "symbol": "BTC",
      "proposed": true,
      "allowed": true
    },
    {
      "symbol": "ETH",
      "proposed": false,
      "allowed": null
    }
  ]
}
```

The plugin never places orders directly. Accepted proposals still flow through
`PaperEngine.submit`, where risk checks, decisions, fills, and rejections are
recorded by the engine.
