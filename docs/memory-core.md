# Memory Core

ZERO Memory is the local, public-safe knowledge layer for the autonomous
operating system. It extracts reusable observations from runtime decisions
without preserving derivable market state, wallet material, exchange order
identifiers, or private keys.

The memory core is open source because it belongs to the operator's
self-custodial runtime. Hosted aggregation, realtime cohorts, history, and
commercial redistribution belong to ZERO Intelligence.

## Contract

Memory entries use `zero.memory.entry.v1` and are append-only JSONL records.
The first public kinds are:

| Kind | TTL | Purpose |
| --- | ---: | --- |
| `signal` | 7 days | Accepted paper decision evidence, redacted to behavior class. |
| `regime` | 3 days | Short-lived market or strategy context. |
| `operator` | 30 days | Operator action outcomes and risk-direction evidence. |
| `strategy_reference` | 90 days | Rejection and strategy-learning references. |

Every entry has a content-derived id, an evidence hash, an expiry time, a
scope, a confidence score, tags, and public-safe metadata. Duplicate entries
are ignored by id. Expired entries remain in the append-only store but are not
included in active memory or generated knowledge.

## Public-Safety Rules

Memory output must not store:

- live prices, quantities, sizes, or notional values;
- raw exchange payloads;
- wallet addresses or private keys;
- exchange order ids or idempotency keys;
- private operator notes.

The extractor may read raw decision journals locally, but the generated memory
records keep only redacted behavior: symbol, gate result, reason class,
source class, reduce-only flag, and hashes.

## Run

From a source checkout:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.memory extract \
  --decisions examples/memory-core/decisions.jsonl \
  --store artifacts/memory/memory.jsonl \
  --knowledge artifacts/memory/knowledge.md \
  --now 2026-05-01T00:00:00Z

PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.memory status \
  --store artifacts/memory/memory.jsonl \
  --now 2026-05-01T00:00:00Z
```

After package install, the console script is:

```bash
zero-memory extract --decisions examples/memory-core/decisions.jsonl --store artifacts/memory/memory.jsonl
```

The API exposes the same public-safe view:

```bash
curl -fsS http://127.0.0.1:8765/memory
curl -fsS 'http://127.0.0.1:8765/memory?format=md'
```

The MCP server exposes `zero_get_memory_snapshot` and the
`zero://memory/snapshot` resource. Both are read-only and fixture-backed.

## Knowledge

`knowledge.md` is generated from active memory only. It is meant for humans and
coding agents as a compact local context packet, not as a raw journal export.
If a future extractor needs private notes, live prices, or raw venue payloads
to learn correctly, that material must stay outside public memory and be
represented only by a redacted hash or operator-owned local file reference.
