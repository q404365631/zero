# Market Data Adapter Example

This is the smallest public contributor path for adding a market data adapter
to ZERO.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src:$PWD/examples/market-data-adapter" \
  python3 examples/market-data-adapter/run.py
```

Or use:

```bash
just market-data-adapter-example
```

The adapter exposes candles only. It does not know about execution, custody,
risk limits, journals, or live mode.

Expected output shape:

```json
{
  "mode": "paper",
  "adapter": {
    "name": "memory-candles",
    "requires_secrets": false,
    "source": "example-fixture"
  },
  "latest_close": 40550.0,
  "proposed": true,
  "allowed": true,
  "fills": 1
}
```

Contributor rules:

- Keep examples deterministic and paper-first.
- Do not require secrets for public examples.
- Return candles in chronological order.
- Raise clear errors for missing symbols or invalid limits.
- Add tests before proposing a new adapter.
