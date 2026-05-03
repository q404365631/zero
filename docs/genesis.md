# Genesis Proposal Core

ZERO Genesis is the plan-only proposal layer for the autonomous operating
system. It turns local memory, proof artifacts, and operator evidence into
classified change proposals without applying code changes automatically.

Genesis is public because operators and contributors should be able to inspect
how the system thinks about self-improvement. Automatic mutation, deployment,
and commercial intelligence loops remain outside this public runtime until they
have review, canary, and rollback controls.

## Contract

Proposals use `zero.genesis.proposal.v1`. Guardian decisions use
`zero.genesis.guardian.v1`. The public API snapshot uses
`zero.genesis.snapshot.v1`.

Each proposal includes:

- title and summary;
- target paths;
- evidence references;
- sample size;
- risk tier;
- revert plan;
- public-safe metadata.

The guardian policy is deterministic:

| Tier | Minimum sample | Outcome |
| --- | ---: | --- |
| `low` | 5 | Accepted when evidence and revert plan are present. |
| `medium` | 30 | Accepted when evidence and revert plan are present. |
| `high` | 100 | Escalated for human review. |
| `protected` | 100 | Escalated for human review. |

Protected paths include execution, sizing, stops, circuit breakers, live
adapters, and immune core code. Any proposal touching those classes requires
human review even when the sample size is sufficient.

## Public-Safety Rules

Genesis proposals must not store:

- live prices, quantities, sizes, or notionals;
- raw exchange payloads;
- wallet addresses or private keys;
- exchange order ids or idempotency keys;
- private operator notes.

## Run

From a source checkout:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.genesis plan \
  --proposals examples/genesis/proposals.jsonl \
  --journal artifacts/genesis/genesis.jsonl \
  --now 2026-05-01T00:00:00Z

PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.genesis status \
  --journal artifacts/genesis/genesis.jsonl \
  --now 2026-05-01T00:00:00Z
```

After package install, the console script is:

```bash
zero-genesis plan --proposals examples/genesis/proposals.jsonl --journal artifacts/genesis/genesis.jsonl
```

The API exposes the same plan-only view:

```bash
curl -fsS http://127.0.0.1:8765/genesis
```

The MCP server exposes `zero_get_genesis_proposals` and the
`zero://genesis/proposals` resource. Both are read-only and fixture-backed.

## Boundary

Genesis is not `/evolve`. It does not write files, open PRs, change strategy
weights, touch live adapters, or deploy services. It only records the
guardian decision that a proposal is accepted, rejected, or escalated. Builder,
red-team, canary, and calibration loops are separate cycles with stricter gates.
