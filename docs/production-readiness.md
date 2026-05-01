# Production Readiness

This scorecard measures ZERO as a production product, not just as a clean public
repository.

The public repository is launch-ready for contributors. The production product
is not yet ready for real capital. The gap is intentional and should stay
visible until the runtime can run a self-custodial Hyperliquid operator end to
end.

## Current Score

| Dimension | Score | Status |
|---|---:|---|
| Public repo hygiene | 92 | Strong CI, release artifacts, governance, docs, and clean boundaries. |
| CLI readiness | 82 | Mature Rust terminal, doctor, TUI, friction gates, tests, and release binary path. Blocked by paper-only engine. |
| Engine runtime | 36 | Deterministic paper runtime and append-only paper decision journal exist. No real Hyperliquid data, OODA loop, runners, durable bus, or live executor. |
| Safety and risk | 58 | Good local contracts and CLI risk asymmetry. Missing live kill-switch drills, dead-man enforcement, custody flow, and exchange-failure tests. |
| API contracts | 50 | Paper fixtures are pinned across Python and Rust. Missing OpenAPI, versioned live contracts, auth scopes, and compatibility policy for production. |
| Deployment | 42 | Docker path exists. Railway template, persistent volume layout, env validation, health checks, and rollback docs are missing. |
| Observability and audit | 50 | Paper decision logs and optional JSONL journal exist. Missing production audit journal, metrics, trace IDs, retention policy, and operator export format. |
| Security and custody | 55 | No secrets needed for first run. Missing live key handling, redaction tests, permission model, and threat-model coverage for Railway deploys. |
| ZERO Network | 15 | Product boundary is defined. Profiles, leaderboards, verification, and opt-in publishing do not exist yet. |
| ZERO Intelligence | 12 | Commercial boundary is defined. API, billing, datasets, rate limits, and terms do not exist yet. |
| Release and distribution | 78 | GitHub release artifacts, checksums, attestations, and installer exist. Package registries and Homebrew are not yet shipped. |
| Documentation for operators | 62 | Good local docs. Missing real production runbook, Railway deploy guide, live-mode warnings, and incident recovery playbooks. |

**Overall production product readiness: 51/100.**

This is acceptable for an open-source foundation release. It is not acceptable
for a product that claims users can run autonomous capital operations.

## CLI Readiness Detail

| Area | Score | Notes |
|---|---:|---|
| Command surface | 88 | `zero`, `zero init`, `zero doctor`, `zero run`, TUI, and slash-command dispatch are well covered. |
| Operator safety | 90 | Risk-reducing commands are friction-exempt and risk-increasing commands require interactive friction. |
| Engine integration | 70 | HTTP, WebSocket, mock engine, and contract tests exist. Production engine parity is not available. |
| Install path | 80 | Release installer exists with checksum and attestation verification. Homebrew/package registries are missing. |
| Diagnostics | 84 | Doctor, JSON output, exit codes, and rate-budget checks are strong. Railway and live-HL diagnostics are missing. |
| TUI production UX | 78 | Snapshot coverage and status honesty are strong. Needs live operator drills against real engine faults. |
| Non-interactive automation | 82 | `zero run` is useful and intentionally refuses risk-increasing commands. Needs production examples. |
| Documentation freshness | 76 | Good command docs, but production deployment and live-mode docs are missing. |

**CLI readiness: 82/100.**

The CLI is close to first-class. The reason it is not above 90 is that it is
ahead of the engine: it can supervise a serious engine, but the public engine is
still paper-only.

## Definition Of 100

ZERO is 100/100 when a new serious operator can:

- deploy the runtime locally or on Railway from the public repo;
- run paper mode on live Hyperliquid market data;
- inspect every decision through the CLI;
- opt into public profile publishing without leaking secrets;
- switch to live mode only after custody, risk, and kill-switch checks pass;
- stop, flatten, or pause risk immediately from the operator terminal;
- export an audit journal that explains every accepted and rejected action;
- recover from restarts without losing position, risk, or decision state;
- publish verified behavior to ZERO Network;
- consume delayed public intelligence for free;
- pay for realtime ZERO Intelligence API access when speed, scale, history, or
  commercial rights matter.

## Execution Cycles

### Cycle 1: Runtime Skeleton

Target score: 58/100.

- Add runtime packages for `runtime`, `bus`, `risk`, `execution`, `operator`,
  `adapters`, and `audit`.
- Keep paper mode deterministic.
- Add a durable local journal format.
- Define the production event schema before adding live execution.

Exit gate:

- `zero-paper-api` writes replayable decisions to a local journal.
- CLI can show runtime status, journal tail, risk state, and last decision.

Current progress:

- `zero-paper-api --journal PATH` appends accepted and rejected paper decisions
  to JSONL.
- `GET /journal?limit=50` returns persisted journal records when a journal is
  configured, or the in-memory decision log otherwise.

### Cycle 2: Hyperliquid Read-Only Adapter

Target score: 66/100.

- Add public Hyperliquid market/account read adapter.
- Add `zero hl status` or equivalent CLI path.
- Add fixtures and replay tests.
- Add rate-limit and exchange-failure tests.

Exit gate:

- Users can run live market-data paper mode without exchange private keys.

### Cycle 3: Paper Trading On Live Data

Target score: 74/100.

- Route live market data through the same evaluator, risk, and execution-intent
  path used by live mode.
- Add strategy/lens plugin contracts.
- Add replayable paper sessions.

Exit gate:

- A Railway or local deployment can run paper mode continuously and survive
  restart without losing journaled decisions.

### Cycle 4: Railway-First Deployment

Target score: 80/100.

- Add Railway template/config.
- Document required variables, volumes, health checks, restart policy, and
  rollback.
- Add `zero doctor railway` checks or equivalent diagnostics.

Exit gate:

- A new operator can deploy paper mode to Railway, inspect it from the CLI, and
  view logs without private context.

### Cycle 5: Self-Custodial Live Execution

Target score: 88/100.

- Add local-only Hyperliquid API wallet flow.
- Add live executor with idempotency, no-retry-on-order-submit, kill switch,
  dead-man switch, reduce-only flatten, max notional, max loss, and max order
  rate.
- Add redaction and custody tests.

Exit gate:

- Live mode remains off until doctor confirms custody, risk, exchange, journal,
  and emergency controls.

### Cycle 6: ZERO Network

Target score: 93/100.

- Add opt-in public profile publishing contract.
- Add verified paper/live badge protocol.
- Add leaderboard data model.
- Keep default runtime private.

Exit gate:

- Operators can publish verified public behavior without leaking credentials,
  private notes, or non-consented strategy details.

### Cycle 7: ZERO Intelligence

Target score: 97/100.

- Add API key, rate-limit, billing, and dataset model.
- Ship delayed public snapshots and realtime paid feeds.
- Add cohort, benchmark, webhook, and export contracts.

Exit gate:

- Commercial users can pay for speed, history, scale, webhooks, exports, and
  redistribution while the runtime stays open.

### Cycle 8: Production Hardening

Target score: 100/100.

- Add chaos drills, exchange outage drills, restart drills, key-rotation drills,
  and incident runbooks.
- Add security review, threat model update, and signed release policy.
- Add Homebrew/package registry distribution once names are secured.

Exit gate:

- The production scorecard reaches at least 95 in every dimension, and no live
  capital path is blocked by missing safety, audit, deployment, or recovery
  work.
