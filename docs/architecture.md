# Architecture

ZERO is an autonomous operating system for self-custodial onchain operations,
starting with onchain perpetual markets.

ZERO has four product surfaces:

- ZERO Runtime: local autonomous operations engine with paper mode, safety
  gates, API, journals, and extension contracts.
- ZERO Terminal: operator CLI for setup, diagnostics, state inspection, replay,
  and supervised actions.
- ZERO Network: public profiles, leaderboards, verification badges, and public decision-flow proof.
- ZERO Intelligence: delayed public snapshots plus a commercial API and
  subscription layer for realtime intelligence, history, cohorts, webhooks, and
  enterprise support.

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

The runtime loop wraps that path in an explicit OODA cycle:

```text
observe -> orient -> decide -> act -> learn -> journal -> durable bus
```

The public `zero-engine-run` command currently runs this loop in paper mode and
emits `zero.runtime.cycle.v1` records. With `--runtime-bus`, it also writes
checksum-chained `zero.runtime.event.v1` events and a fast boot snapshot. See
[runtime-bus.md](runtime-bus.md) for the local event contract. Live-capable
runtime work must preserve the same cycle visibility and fail closed when
safety or reconciliation checks are not ready.

## Commercial Flow

```text
Runtime behavior -> opt-in verification -> public network proof -> ZERO Intelligence API
```

The commercial product is advantaged access to verified autonomous behavior at
speed, scale, and history. Basic runtime use, self-custody, public profiles, and
public leaderboards, and delayed public intelligence snapshots remain public
surfaces.
