# Architecture

ZERO is an autonomous operating system for self-custodial onchain operations,
starting with onchain perpetual markets.

ZERO has five product surfaces:

- ZERO Runtime: local autonomous operations engine with paper mode, safety
  gates, API, journals, and extension contracts.
- ZERO Terminal: operator CLI for setup, diagnostics, state inspection, replay,
  and supervised actions.
- ZERO Evolution: local memory, genesis proposals, guardian review, red-team,
  paper canaries, calibration, and evolve loops that let the system improve
  under evidence and review.
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

## Public Evolution Flow

```text
runtime journal -> memory -> knowledge -> genesis proposal -> guardian
                -> build/red-team -> paper canary -> calibration
                -> promote or rollback -> evolve backlog
```

The public repo now implements the first part of this loop: local memory
extraction, append-only memory JSONL, active-memory expiry, generated
`knowledge.md`, `/memory`, and read-only MCP memory snapshots. Genesis,
guardian, build/red-team, paper canary, calibration, and promote/rollback are
tracked as public extraction targets in
[Private Engine Capability Gap Audit](private-engine-capability-gap-audit.md).
Local memory, genesis, and evolve belong in open source because they are part
of a self-custodial operator's runtime. Commercial ZERO Intelligence begins
when many verified runtimes opt into aggregate realtime behavior, cohorts,
history, webhooks, redistribution, or operational SLAs.

## Commercial Flow

```text
Runtime behavior -> opt-in verification -> public network proof -> ZERO Intelligence API
```

The commercial product is advantaged access to verified autonomous behavior at
speed, scale, and history. Basic runtime use, self-custody, public profiles, and
public leaderboards, and delayed public intelligence snapshots remain public
surfaces.
