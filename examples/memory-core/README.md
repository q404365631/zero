# Memory Core Example

This fixture demonstrates public-safe memory extraction from a local paper
decision journal. The input intentionally includes paper prices and sizes
because real journals do. The generated memory output must redact those
derivable fields.

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.memory extract \
  --decisions examples/memory-core/decisions.jsonl \
  --store artifacts/memory/memory.jsonl \
  --knowledge artifacts/memory/knowledge.md \
  --now 2026-05-01T00:00:00Z
```

The expected output has `zero.memory.extract.v1`, active signal and
strategy-reference entries, and privacy flags set to false for live prices,
wallet material, exchange order ids, and private keys.
