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

## Expected Output

`just example` prints one paper-only JSON summary. The abbreviated shape should
stay stable:

```json
{
  "scenario": "paper-launch-smoke",
  "mode": "paper",
  "fills": 2,
  "rejections": 2,
  "positions": {
    "BTC": {
      "quantity": 0.005,
      "avg_price": 40000.0,
      "notional_usd": 200.0
    }
  }
}
```

The first BTC buy is accepted because it fits the paper limits. The ETH and
SOL buys are rejected because each order exceeds the configured
`max_notional_usd`. The second BTC order is accepted even with low confidence
because it is `reduce_only=true`; risk-reducing orders bypass
risk-increasing friction and reduce the remaining paper BTC position.

The scenario lives in `scenario.json`. Keep it deterministic and paper-only.
The market fixture lives in `candles.jsonl`; it is loaded through the public
`JsonlCandleAdapter` and requires no network access.

Strategy template:

```bash
python examples/paper-trading/strategy_demo.py
```

The strategy demo proposes an order from the candle fixture, then submits it
through the paper safety gate.
