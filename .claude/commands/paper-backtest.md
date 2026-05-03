# Paper Backtest

Use this recipe when an agent needs to inspect or modify deterministic paper
runtime behavior.

## Context

ZERO public examples are paper-first. This command must not add live execution,
wallet, venue-write, credential, or hosted-control-plane behavior.

## Steps

1. Read `AGENTS.md`, `docs/safety-model.md`, and the example being changed.
2. Run the deterministic paper examples:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/paper-trading/run.py
PYTHONPATH="$PWD/engine/src" python3 examples/paper-trading/strategy_demo.py
PYTHONPATH="$PWD/engine/src" python3 examples/runtime-loop/run.py
```

3. If behavior changes, update tests under `engine/tests/` and the relevant
   example README.
4. Run:

```bash
cd engine && PYTHONPATH="$PWD/src" pytest
just docs-check
```

## Handoff

Report the scenario name, fills, rejections, open positions, and any safety gate
that changed. Do not report PnL, live readiness, or exchange support unless the
repo contains reproducible evidence for the claim.
