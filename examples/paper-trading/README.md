# Paper Trading Example

This example is the public first-run path.

Run it from a bootstrapped checkout:

```bash
just bootstrap
just example
```

It must:

- require no real exchange private key
- run from a clean checkout
- generate deterministic sample market data or use a harmless public feed
- show engine decisions, rejected trades, risk state, and operator CLI inspection
- be covered by CI smoke tests

The output is deterministic JSON so it can be copied into issues and tests.

The scenario lives in `scenario.json`. Keep it deterministic and paper-only.
The market fixture lives in `candles.jsonl`; it is loaded through the public
`JsonlCandleAdapter` and requires no network access.

Strategy template:

```bash
python examples/paper-trading/strategy_demo.py
```

The strategy demo proposes an order from the candle fixture, then submits it
through the paper safety gate.
