# New Strategy

Use this recipe when an agent adds a paper-first strategy example, declarative
runner, or plugin.

## Context

Strategies return signals or order intents for the paper engine. They must not
place orders directly, read credentials, call live execution APIs, or bypass
risk checks.

## Steps

1. Read `docs/strategy-plugins.md`, `examples/strategy-plugin/README.md`, and
   `examples/strategy-runner/README.md`.
2. Choose one path:
   - Declarative runner under `examples/strategy-runner/`.
   - Plugin example under `examples/strategy-plugin/`.
   - Engine strategy test under `engine/tests/`.
3. Add deterministic fixtures and tests before adding broader behavior.
4. Run the relevant checks:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/paper-trading/strategy_demo.py
PYTHONPATH="$PWD/engine/src:$PWD/examples/strategy-plugin" python3 examples/strategy-plugin/run.py
PYTHONPATH="$PWD/engine/src" python3 examples/strategy-runner/run.py
cd engine && PYTHONPATH="$PWD/src" pytest tests/test_strategy.py tests/test_plugins.py tests/test_runners.py
```

## Handoff

Report the strategy name, paper-only status, proposed symbol, fills/rejections
in the deterministic example, and the test files changed.
