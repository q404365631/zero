# Proof Pack

Use this recipe when an agent changes public proof-pack generation, ZERO
Network proof-chain generation, MCP proof exposure, or proof-pack docs.

## Context

The public demo proof pack is a deterministic paper-mode artifact. It must not
claim live trading, PnL, paper/live correlation, or exchange-side execution.
The Network proof pack is also paper-mode only and must remain public-safe:
no exchange credentials, wallet material, raw decisions, trace tokens, or
idempotency tokens.

## Steps

1. Read `docs/proof/README.md` and `scripts/proof_pack.py`.
2. Regenerate and verify the specific pack you changed:

```bash
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py --check
PYTHONPATH="$PWD/engine/src" scripts/network_proof_pack.py
PYTHONPATH="$PWD/engine/src" scripts/network_proof_pack.py --check
```

3. Inspect:

```bash
python3 -m json.tool docs/proof/demo/proof-pack.json
python3 -m json.tool docs/proof/network/network-proof-pack.json
```

4. Run the public proof gate:

```bash
just public-proof
```

5. Run:

```bash
scripts/hardening_gate.sh
scripts/public_readiness_gate.sh
```

## Handoff

Report the demo proof hash, Network proof hash, decision count, fill count,
rejection count, verification status, and privacy flags. State explicitly that
the artifacts are paper-only unless signed live evidence has been added in a
separate, reviewed change.
