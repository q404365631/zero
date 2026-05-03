# Evolve Harness

ZERO Evolve is the paper-first builder, red-team, canary, calibration,
promotion-plan, local-apply, and rollback harness for the public
self-evolution loop.
It consumes accepted genesis decisions and produces local evidence before any
human considers promotion.

The default run path does not mutate the checkout, push branches, deploy
services, or change live trading code. It writes a sandbox artifact that
describes the candidate branch, candidate patch, materialized candidate tree,
red-team verdict, paper canary result, calibration result, local-only promotion
decision, promotion plan, rollback plan, and promotion verification.

Checkout mutation exists only through the explicit `apply` subcommand. `apply`
requires an exact human approval phrase, verifies every original/candidate hash
before writing any file, writes an apply receipt, never pushes remotely, and
never places orders. `rollback` requires a separate exact approval phrase and
restores from the apply receipt backups while verifying original hashes.

## Contract

The run artifact uses `zero.evolve.run.v1`.

Each run includes:

- selected accepted genesis proposal;
- local sandbox and candidate patch manifest;
- red-team review;
- deterministic paper canary;
- calibration against the fixture baseline;
- promotion decision that always requires human approval;
- promotion plan with the exact approval phrase and no remote push;
- rollback plan with original and candidate hashes;
- promotion verification that fails if either plan can mutate the checkout or
  push remotely.

Promotion is local-only. `pushes_to_remote=false` and `promoted=false` are part
of the public contract. Run artifacts keep `applies_to_checkout=false`; apply
receipts use `applies_to_checkout=true` only after local human approval and hash
verification.

## Policy

The public harness allows generated candidate patches only under:

- `docs/`
- `examples/`

It blocks protected runtime paths including live adapters, Hyperliquid adapter,
immune core, safety code, and CLI live dispatch. Protected proposals must stay
at the genesis escalation stage until reviewed by a human.

Every promotable run must also include:

- `zero.evolve.promotion_plan.v1`;
- `zero.evolve.rollback_plan.v1`;
- `zero.evolve.promotion_verification.v1`;
- `zero.evolve.apply_receipt.v1` for any approved checkout mutation;
- `zero.evolve.rollback_receipt.v1` after any approved rollback;
- the exact approval phrase `I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION`;
- the exact rollback phrase `I_APPROVE_ZERO_EVOLVE_LOCAL_ROLLBACK`;
- original and candidate hashes for every sandbox mutation.

## Run

From a source checkout:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.genesis plan \
  --proposals examples/genesis/proposals.jsonl \
  --journal artifacts/evolve/genesis.jsonl \
  --now 2026-05-01T00:00:00Z

PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.evolve run \
  --genesis-journal artifacts/evolve/genesis.jsonl \
  --output artifacts/evolve \
  --repo-root "$PWD" \
  --now 2026-05-01T00:00:00Z

PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.evolve status \
  --run artifacts/evolve/evolve-run.json \
  --now 2026-05-01T00:00:00Z
```

To apply into a local checkout, review `artifacts/evolve/evolve-run.json`, then
run the explicit approval command:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.evolve apply \
  --run artifacts/evolve/evolve-run.json \
  --repo-root "$PWD" \
  --output artifacts/evolve/apply \
  --approval-phrase I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION
```

To roll back that local apply:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.evolve rollback \
  --apply-receipt artifacts/evolve/apply/apply-receipt.json \
  --repo-root "$PWD" \
  --output artifacts/evolve/rollback \
  --approval-phrase I_APPROVE_ZERO_EVOLVE_LOCAL_ROLLBACK
```

After package install, the console script is:

```bash
zero-evolve run --proposals examples/genesis/proposals.jsonl --output artifacts/evolve
```

The API exposes a fixture-backed snapshot:

```bash
curl -fsS http://127.0.0.1:8765/evolve
```

The MCP server exposes `zero_get_evolve_status` and the
`zero://evolve/status` resource. Both are read-only and fixture-backed.

## Boundary

Evolve is not autonomous deployment. It is the evidence and local checkout
promotion layer before a human review. It remains local by default, requires
explicit human approval, verifies rollback before apply, and refuses remote push
or live-code mutation unless a stricter release policy is added.
