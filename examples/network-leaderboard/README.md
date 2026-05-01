# ZERO Network Leaderboard Example

This example builds a public leaderboard from already-redacted
`zero.network.profile.v1` packets.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/network-leaderboard/build.py
```

Or use:

```bash
just network-leaderboard-example
```

The builder does not read journals, raw decisions, symbols, trace IDs,
idempotency keys, wallet addresses, or exchange data. It only accepts public
profile packets and emits `zero.network.leaderboard.v1`.

Expected output shape:

```json
{
  "schema_version": "zero.network.leaderboard.v1",
  "row_count": 3,
  "rows": [
    {
      "rank": 1,
      "handle": "zero_alpha",
      "verification_score": 70.5
    }
  ]
}
```

Contributor rules:

- Build leaderboards from redacted public profile packets only.
- Keep ranking deterministic.
- Reject malformed profiles.
- Do not rank by self-reported PnL screenshots.
- Treat the leaderboard as proof-of-process, not financial advice.
