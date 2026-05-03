# Proof Pack

Use this recipe when an agent changes public proof-pack generation or proof-pack
docs.

## Context

The public demo proof pack is a deterministic paper-mode artifact. It must not
claim live trading, PnL, paper/live correlation, or exchange-side execution.

## Steps

1. Read `docs/proof/README.md` and `scripts/proof_pack.py`.
2. Regenerate and verify:

```bash
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py --check
```

3. Inspect:

```bash
python3 -m json.tool docs/proof/demo/proof-pack.json
```

4. Run:

```bash
scripts/hardening_gate.sh
scripts/public_readiness_gate.sh
```

## Handoff

Report the proof hash, decision count, fill count, rejection count, and privacy
flags. State explicitly that the artifact is paper-only unless signed live
evidence has been added in a separate, reviewed change.
