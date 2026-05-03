# Evolve Example

This fixture demonstrates the public paper-only self-evolution harness. It
classifies genesis proposals, selects the accepted docs/example proposal, writes
a sandbox candidate patch, runs red-team review, replays the deterministic paper
canary, calibrates against the baseline, and produces a local-only promotion
decision.

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
```

The expected output has `zero.evolve.run.v1`, a passing red-team verdict, a
zero-drift paper canary, passing calibration, and `pushes_to_remote=false`.
