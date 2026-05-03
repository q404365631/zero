# Evolve Harness

ZERO Evolve is the paper-only builder, red-team, canary, calibration,
promotion-plan, rollback-plan, and promotion-verification harness for the
public self-evolution loop.
It consumes accepted genesis decisions and produces local evidence before any
human considers promotion.

The public harness does not mutate the checkout, push branches, deploy services,
or change live trading code. It writes a sandbox artifact that describes the
candidate branch, candidate patch, materialized candidate tree, red-team
verdict, paper canary result, calibration result, local-only promotion
decision, promotion plan, rollback plan, and promotion verification.

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

Promotion is local-only. `pushes_to_remote=false`, `applies_to_checkout=false`,
and `promoted=false` are part of the public contract. The current public
harness mutates only the sandbox candidate tree.

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
- the exact approval phrase `I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION`;
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

Evolve is not autonomous deployment. It is the evidence layer before a human
review. A future promote command must remain local by default, require explicit
human approval, verify rollback first, and refuse remote push or live-code
mutation unless a stricter release policy is added.
