# Strategy Plugin Example

This is the smallest public contributor path for adding a strategy to ZERO.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src:$PWD/examples/strategy-plugin" \
  python3 examples/strategy-plugin/run.py
```

Or use:

```bash
just strategy-plugin-example
```

The plugin can inspect market data and return a `StrategySignal`. It cannot
place orders directly. The paper engine still owns risk checks, decision
recording, fills, and rejections.

Expected output shape:

```json
{
  "mode": "paper",
  "plugin": {
    "name": "close-strength",
    "paper_only": true,
    "version": "0.1.0"
  },
  "proposed": true,
  "allowed": true,
  "fills": 1,
  "rejections": 0
}
```

Contributor rules:

- Keep plugins deterministic and paper-first.
- Do not read private keys, wallet secrets, or exchange credentials.
- Do not call live execution APIs from plugin code.
- Add tests before proposing a new plugin.
