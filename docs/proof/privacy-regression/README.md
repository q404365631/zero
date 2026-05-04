# Proof Privacy Regression Fixtures

These fixtures are intentionally negative. They are synthetic `zero.network.profile.v1`
payloads with private-looking fields injected so the public verifier proves it
will refuse artifacts that should never appear in a proof pack.

They are not real operator records, exchange records, wallet material, custody
proof, or PnL proof. ZERO proof packs are proof-of-process artifacts: they show
that a paper or public Network flow followed a reproducible, redacted path. They
do not prove custody, profitability, exchange account ownership, or live trading
results unless a future manifest explicitly attaches signed live records and
exchange-side evidence.

Run the regression check:

```bash
PYTHONPATH="$PWD/engine/src" scripts/proof_privacy_regression.py
```
