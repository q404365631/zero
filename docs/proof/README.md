# Proof Packs

ZERO proof packs are public-safe artifacts that let a human or agent verify what
the runtime actually did.

The current committed pack is intentionally modest:

- it is generated from the deterministic paper scenario in
  `examples/paper-trading`;
- it includes a CSV of paper decisions, a small SVG summary, and a hash-addressed
  manifest;
- it explicitly does not claim live trading, PnL, or paper-vs-live correlation.

Generate or verify the demo pack:

```bash
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py --check
```

Future launch proof packs must add signed live records, exchange-side evidence,
and paper/live correlation only after those records exist. Do not publish an
R-squared value, latency claim, win rate, or PnL result unless the exact
supporting artifacts are committed or linked from the manifest.

## Required Live Correlation Inputs

A real paper-vs-live pack must include:

- paper decisions and fills exported from the same strategy window;
- exchange-side order and fill records;
- public-safe hashes of raw venue identifiers;
- a reproducible notebook or script that computes the metric;
- a manifest hash and signature.

Until those inputs exist, `live_correlation.status` must remain `unavailable`.
