# Durable Runtime Bus

The durable runtime bus is ZERO's local source of replayable runtime truth. It
is separate from the decision journal:

- the decision journal stores paper execution decisions that rebuild fills,
  rejections, positions, and idempotency state;
- the runtime bus stores OODA cycles, decisions, fills, rejections, position
  snapshots, health records, and future operator commands as checksum-chained
  events.

The public implementation is dependency-free JSONL so contributors can inspect
and test it without a database. The interface is intentionally narrow enough to
mirror the same events into SQLite, Postgres, or hosted ingestion later.

## Run With A Bus

```bash
PYTHONPATH="$PWD/engine/src" zero-engine-run \
  --scenario examples/paper-trading/scenario.json \
  --journal .zero/decisions.jsonl \
  --runtime-bus .zero/runtime-bus \
  --once \
  --interval 0
```

This writes:

- `.zero/runtime-bus/events.jsonl`
- `.zero/runtime-bus/state-snapshot.json`

Each event has:

- `schema_version: zero.runtime.event.v1`
- a sequential `event_index`
- a deterministic `event_id`
- an `event_type`
- a payload
- `previous_checksum`
- `checksum`

`DurableRuntimeBus.verify_integrity()` walks the whole file and fails if any
event is mutated, deleted, reordered, or linked to the wrong previous checksum.

## Audit Export

`DurableRuntimeBus.export_audit()` returns `zero.runtime.audit.v1` from disk
only. It includes integrity status, event type counts, the latest state
snapshot, and every event.

The bus is private operator state by default. Public ZERO Network and ZERO
Intelligence exports must stay aggregate and redacted; raw bus events include
traceable runtime details and are not a public profile surface.

## Production-Parity Report

`zero-engine-run --production-parity` writes
`zero.runtime.production_parity.v1` to the chosen output directory:

```bash
PYTHONPATH="$PWD/engine/src" zero-engine-run \
  --production-parity \
  --scenario examples/paper-trading/scenario.json \
  --output artifacts/runtime-parity
```

The report runs the same OODA phase order as the paper runtime, mirrors each
intent through a disabled live executor, verifies zero adapter orders were
placed, checks the runtime-bus checksum chain, and emits
`zero.runtime.feedback.v1` rejection/execution-quality feedback. It is a
production-shape parity proof, not live trading proof.
