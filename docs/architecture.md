# Architecture

ZERO has two public runtime surfaces:

- Engine: local autonomous trading runtime with paper mode, safety gates, API, and extension contracts.
- CLI: operator terminal for setup, diagnostics, state inspection, replay, and supervised actions.

Commercial ZERO Cloud is outside the open-source runtime and provides hosted team operations, fleet management, premium data, and enterprise controls.

## Principles

- Local-first by default
- Paper mode before live execution
- Explicit operator control
- Inspectable decisions
- Risk-reducing actions stay fast
- Risk-increasing actions require friction

## Public Engine Flow

```text
JSONL candles -> MarketDataAdapter -> Strategy -> OrderIntent -> PaperEngine -> RiskDecision
```

Strategies propose. The paper engine decides. This keeps extension work
deterministic and prevents examples from bypassing safety gates.
