# ZERO Network Leaderboard Page Example

This example builds a deterministic static HTML page from an already-redacted
`zero.network.leaderboard.v1` payload.

```bash
just network-leaderboard-page-example
```

The page renders rank, handle, display name, mode, aggregate counts,
verification score, and proof hash only. It uses no JavaScript, remote assets,
raw journals, symbols, trace IDs, idempotency keys, wallet addresses, exchange
order IDs, strategy labels, or private notes.

To regenerate the checked contract artifact:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/network-leaderboard-page/build.py \
  --output contracts/network/leaderboard.html
```
