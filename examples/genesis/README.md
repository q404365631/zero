# Genesis Example

This fixture demonstrates ZERO Genesis in plan-only mode. Genesis can classify
proposed changes from local memory and proof artifacts, but the public runtime
does not apply code changes automatically.

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.genesis plan \
  --proposals examples/genesis/proposals.jsonl \
  --journal artifacts/genesis/genesis.jsonl \
  --now 2026-05-01T00:00:00Z

PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.genesis status \
  --journal artifacts/genesis/genesis.jsonl \
  --now 2026-05-01T00:00:00Z
```

The expected decisions are deterministic:

- one accepted docs/example proposal;
- one rejected proposal with insufficient sample size;
- one escalated proposal because live execution paths require human review.

Genesis proposals must not contain wallet material, private keys, live prices,
exchange order ids, raw exchange payloads, sizes, or notionals.
