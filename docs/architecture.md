# Architecture

ZERO has four product surfaces:

- ZERO Runtime: local Autonomous Risk Operations engine with paper mode, safety gates, API, and extension contracts.
- ZERO Terminal: operator CLI for setup, diagnostics, state inspection, replay, and supervised actions.
- ZERO Network: public profiles, leaderboards, verification badges, and public decision-flow proof.
- ZERO Intelligence: commercial API and subscription layer for realtime intelligence, history, cohorts, webhooks, and enterprise support.

Deployment is Railway-first, Docker-compatible, and local-first. Operators own
their deployment project, secrets, exchange credentials, and runtime state.
ZERO does not need a separate hosted deployment product to be credible.

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

## Commercial Flow

```text
Runtime behavior -> opt-in verification -> public network proof -> ZERO Intelligence API
```

The commercial product is advantaged access to verified autonomous behavior at
speed, scale, and history. Basic runtime use, self-custody, public profiles, and
public leaderboards remain public surfaces.
