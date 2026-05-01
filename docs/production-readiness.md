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
| CLI readiness | 84 | Mature Rust terminal, doctor, TUI, friction gates, tests, release binary path, and recovery-aware status output. Blocked by paper-only engine. |
| Engine runtime | 56 | Deterministic paper runtime, append-only decision journal, restart replay, read-only Hyperliquid info adapter, and live-mid paper execution exist. No OODA loop, runners, durable bus, or live executor. |
| Safety and risk | 58 | Good local contracts and CLI risk asymmetry. Missing live kill-switch drills, dead-man enforcement, custody flow, and exchange-failure tests. |
| API contracts | 61 | Paper fixtures are pinned across Python and Rust, `/hl/status` exposes read-only market status, `/market/quote` names the active price source, and `/health` plus `/v2/status` expose recovery state. Missing OpenAPI, versioned live contracts, auth scopes, and compatibility policy for production. |
| Deployment | 62 | Docker path, Railway config, healthcheck, restart policy, `PORT`-aware start script, durable journal replay, and Railway smoke test exist. Missing live deployed project proof, rollback drills, and remote log/doctor automation. |
| Observability and audit | 58 | Paper decision logs, idempotency keys, replay counts, and optional JSONL journal recovery exist. Missing production audit journal, metrics, trace IDs, retention policy, and operator export format. |
| Security and custody | 55 | No secrets needed for first run. Missing live key handling, redaction tests, permission model, and threat-model coverage for Railway deploys. |
| ZERO Network | 15 | Product boundary is defined. Profiles, leaderboards, verification, and opt-in publishing do not exist yet. |
| ZERO Intelligence | 12 | Commercial boundary is defined. API, billing, datasets, rate limits, and terms do not exist yet. |
| Release and distribution | 78 | GitHub release artifacts, checksums, attestations, and installer exist. Package registries and Homebrew are not yet shipped. |
| Documentation for operators | 76 | Good local docs, Hyperliquid read-only boundary docs, live-paper quote docs, Railway paper deploy docs, and restart recovery docs. Missing live-mode warnings and incident recovery playbooks. |

**Overall production product readiness: 70/100.**

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

**CLI readiness: 84/100.**

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

Forecast after Cycle 5: **6 more major cycles** to credible 100/100.

| Cycle | Target | Expected Score |
|---|---|---:|
| 5 | Durable runtime state and restart recovery | 70 |
| 6 | Production audit journal, metrics, and trace IDs | 76 |
| 7 | Live custody preflight and local key handling | 84 |
| 8 | Self-custodial Hyperliquid live execution with kill switches | 90 |
| 9 | ZERO Network verified public profiles and leaderboards | 94 |
| 10 | ZERO Intelligence API, billing boundary, and delayed public data | 97 |
| 11 | External hardening: security review, package registries, Homebrew, release drills | 100 |

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

Current progress:

- `HyperliquidInfoClient` reads public `allMids` and validates account-read
  addresses for `clearinghouseState`.
- `zero-paper-api --hyperliquid` enables `/hl/status` without requiring secrets
  or signing.
- `/hl/status` stays disabled by default so contributor examples remain
  deterministic and offline.

### Cycle 3: Paper Trading On Live Data

Target score: 74/100.

- Route live market data through the same evaluator, risk, and execution-intent
  path used by live mode.
- Add strategy/lens plugin contracts.
- Add replayable paper sessions.

Exit gate:

- A Railway or local deployment can run paper mode continuously and survive
  restart without losing journaled decisions.

Current progress:

- `zero-paper-api --hyperliquid-live-prices` routes paper quotes through cached
  Hyperliquid `allMids`.
- `POST /execute`, `GET /evaluate/{coin}`, and `GET /positions` use the same
  quote path, so paper fills and marks are sourced from live mids when enabled.
- `GET /market/quote?symbol=BTC` exposes the active quote source for operator
  inspection.
- Missing live symbols and market-data failures fail closed instead of silently
  falling back to deterministic fixture prices.

### Cycle 4: Railway-First Deployment

Target score: 80/100.

- Add Railway template/config.
- Document required variables, volumes, health checks, restart policy, and
  rollback.
- Add `zero doctor railway` checks or equivalent diagnostics.

Exit gate:

- A new operator can deploy paper mode to Railway, inspect it from the CLI, and
  view logs without private context.

Current progress:

- `railway.toml` defines Dockerfile deployment, `/health`, timeout, and restart
  policy.
- `/app/scripts/railway_start.sh` binds `0.0.0.0:${PORT}` and writes the paper
  decision journal to `/data/decisions.jsonl` by default.
- `docs/railway-deploy.md` documents the volume, variables, CLI connection, and
  failure modes.
- GitHub CI runs `scripts/railway_smoke.sh`, which boots the Docker image with
  Railway-style variables, checks `/health`, records a paper fill, and verifies
  journal recovery through `/journal`.

### Cycle 5: Durable Runtime State And Restart Recovery

Target score: 70/100.

- Persist runtime state needed to recover after process or host restart.
- Rehydrate positions, recent decisions, and paper session metadata from disk.
- Add explicit recovery status to `/health`, `/v2/status`, and CLI output.

Exit gate:

- A Railway or local deployment can restart and keep enough state to explain
  the active paper session without relying only on process memory.

Current progress:

- `PaperEngine.recover_from_journal` replays append-only decision records into
  in-memory decisions, simulated fills, open positions, and rejections.
- Journaled idempotency keys rebuild the API execution cache, so duplicate
  `POST /execute` requests after restart return the original simulated result
  instead of creating another paper fill.
- `/health` and `/v2/status` expose recovery status, durable-vs-ephemeral
  journal mode, recovered counts, current counts, and the last decision time.
- `zero run status` renders a recovery row when the engine reports recovery
  metadata.

### Cycle 6: Production Audit And Observability

Target score: 76/100.

- Add trace IDs across decisions, fills, API requests, and CLI commands.
- Add metrics endpoints and operator export format.
- Define journal retention, redaction, and integrity checks.

Exit gate:

- An operator can export a complete paper-session audit trail and correlate CLI
  actions with engine decisions.

### Cycle 7: Live Custody Preflight

Target score: 84/100.

- Add local-only Hyperliquid API wallet setup.
- Add key redaction, permission checks, account-read verification, and dry-run
  order validation.
- Add `zero doctor` checks that refuse live mode until custody, exchange, risk,
  journal, and emergency controls are all present.

Exit gate:

- Live mode cannot start without a passing custody and safety preflight.

### Cycle 8: Self-Custodial Live Execution

Target score: 90/100.

- Add live executor with idempotency, no-retry-on-order-submit, kill switch,
  dead-man switch, reduce-only flatten, max notional, max loss, and max order
  rate.
- Add exchange outage drills, kill-switch drills, and flatten drills.

Exit gate:

- Operators can start live mode only after preflight and can immediately pause,
  kill, or flatten exposure from the CLI.

### Cycle 9: ZERO Network

Target score: 94/100.

- Add opt-in public profile publishing contract.
- Add verified paper/live badge protocol.
- Add leaderboard data model.
- Keep default runtime private.

Exit gate:

- Operators can publish verified public behavior without leaking credentials,
  private notes, or non-consented strategy details.

### Cycle 10: ZERO Intelligence

Target score: 97/100.

- Add API key, rate-limit, billing, and dataset model.
- Ship delayed public snapshots and realtime paid feeds.
- Add cohort, benchmark, webhook, and export contracts.

Exit gate:

- Commercial users can pay for speed, history, scale, webhooks, exports, and
  redistribution while the runtime stays open.

### Cycle 11: Production Hardening

Target score: 100/100.

- Add chaos drills, exchange outage drills, restart drills, key-rotation drills,
  and incident runbooks.
- Add security review, threat model update, and signed release policy.
- Add Homebrew/package registry distribution once names are secured.

Exit gate:

- The production scorecard reaches at least 95 in every dimension, and no live
  capital path is blocked by missing safety, audit, deployment, or recovery
  work.
