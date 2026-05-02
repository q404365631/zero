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
| Public repo hygiene | 99 | Strong CI, release artifacts, governance, docs, clean boundaries, first-class GitHub product page, first-10-minutes guide, reproducible demo capture, threat model, incident runbooks, distribution policy, and hardening gate. |
| Product narrative | 98 | Clear category around autonomous operating systems for self-custodial onchain operations, with public runtime, terminal, network, and intelligence surfaces separated cleanly. |
| CLI readiness | 92 | Mature Rust terminal, doctor, TUI, friction gates, tests, release binary path, recovery-aware status output, live-preflight diagnostics, live risk-reducer wiring, and `/live-certify` operator readout exist. Remaining live cockpit drills need real canary evidence. |
| Engine runtime | 89 | Deterministic paper runtime, bounded OODA cycle records, strategy runner SDK, declarative paper strategies, append-only decision journal, checksum-chained durable runtime bus, restart replay, read-only Hyperliquid info adapter, live-mid paper execution, traceable audit export, live custody preflight, account reconciliation, dry-run live certification, and optional Hyperliquid live executor exist. Still missing continuous live-certified controller and real live canary evidence. |
| Safety and risk | 92 | CLI risk asymmetry, local custody validation, dry-run order validation, preflight refusal, account-reconciliation gate, idempotent live submit, no-retry exchange-error records, dry-run live certification drills, dead-man heartbeat, max notional/loss/order-rate limits, pause, kill, reduce-only flatten, threat model, and P0/P1 runbooks exist. Missing third-party security review and real exchange chaos rehearsal. |
| API contracts | 93 | Paper fixtures are pinned across Python and Rust, OpenAPI documents the local paper runtime, compatibility rules are explicit, `/hl/status` exposes read-only market status, `/hl/account` and `/hl/reconcile` expose account truth, `/live/certification` exposes dry-run safety evidence, `/market/quote` names the active price source, `/health` plus `/v2/status` expose recovery state, `/metrics` plus `/audit/export` expose observable runtime state, `/network/*` exposes public proof packets, `/intelligence/*` exposes delayed intelligence and commercial API contracts, `/live/preflight` exposes a non-secret live-readiness gate, and `POST /live/*` controls are typed in the CLI. Missing hosted auth enforcement and production hosted compatibility policy. |
| Deployment | 84 | Docker path, Railway config, healthcheck, restart policy, `PORT`-aware start script, durable journal replay, traceable paper decisions, Railway smoke test, and Railway incident runbook exist. Missing live deployed project proof and remote log/doctor automation. |
| Observability and audit | 90 | HTTP trace IDs, traced paper decisions, metrics, idempotency counters, replay counts, retention/redaction metadata, structured audit export, checksum-chained runtime events, local state snapshots, Hyperliquid reconciliation packets, dry-run live certification packets, live execution records, and required incident artifacts are documented. Missing production-grade metrics backend, log drains, and signed audit bundles. |
| Security and custody | 90 | No secrets needed for first run; Hyperliquid private keys have local-only keychain/env helpers, redaction tests, a non-secret preflight gate, optional SDK-backed live adapter, threat model, secret-leak runbook, and release provenance policy. Missing external security review. |
| ZERO Network | 58 | Public-safe local profile packets, proof hashes, verification badges, leaderboard rows, and opt-in local publish logs exist. Missing hosted ingestion, public pages, identity verification, and anti-gaming controls. |
| ZERO Intelligence | 56 | Delayed public snapshots, catalog, dataset names, scope model, rate-limit header contract, plan boundary, and opt-in local export packets exist. Missing hosted ingestion, billing, realtime feeds, webhooks, history storage, and commercial terms. |
| Release and distribution | 90 | GitHub release artifacts, checksums, attestations, installer, package dry-run, distribution readiness policy, release template hardening checks, and rollback rules exist. Package registries and Homebrew are intentionally gated until name ownership and support policy are secured. |
| Documentation for operators | 95 | Good local docs, Hyperliquid read-only boundary docs, live-paper quote docs, live certification docs, Railway paper deploy docs, restart recovery docs, audit/metrics docs, live-preflight warnings, threat model, and incident runbooks. Missing real exchange drill evidence. |

**Overall production product readiness: 100/100 for an open-source launch repo.**

This is credible for the public open-source launch repository. It is still not
a hosted custody product, and real capital operation remains self-custodial and
operator-owned.

For the larger target, ZERO is not yet 100/100 as a complete autonomous
operating system. The remaining work is tracked in the
[ZERO Autonomous OS Completion Plan](autonomous-os-plan.md): production
strategy runners, durable runtime bus, Hyperliquid reconciliation, live
execution certification, immune controls, terminal live cockpit,
multi-operator isolation, LLM gateway, hosted Network ingestion, ZERO
Intelligence API, deployment evidence, distribution, and external review.

## CLI Readiness Detail

| Area | Score | Notes |
|---|---:|---|
| Command surface | 88 | `zero`, `zero init`, `zero doctor`, `zero run`, TUI, and slash-command dispatch are well covered. |
| Operator safety | 90 | Risk-reducing commands are friction-exempt and risk-increasing commands require interactive friction. |
| Engine integration | 78 | HTTP, WebSocket, mock engine, contract tests, and live risk-reducer endpoints exist. Production OODA parity is not available. |
| Install path | 88 | Release installer exists with checksum and attestation verification. Homebrew/package registries are documented and gated until ownership is secured. |
| Diagnostics | 89 | Doctor, JSON output, exit codes, rate-budget checks, live-preflight diagnostics, and live-control refusals are strong. Railway remote-log automation is still missing. |
| TUI production UX | 82 | Snapshot coverage and status honesty are strong. Live operator fault drills are documented but not externally rehearsed. |
| Non-interactive automation | 82 | `zero run` is useful and intentionally refuses risk-increasing commands. Needs production examples. |
| Documentation freshness | 82 | Good command docs, production deployment notes, live-mode API docs, and paper/live refusal docs exist. Incident docs remain thin. |

**CLI readiness: 91/100.**

The CLI is first-class for the public runtime and operator workflows in this
repo. It is not yet a complete autonomous capital terminal because the public
engine still lacks the full production OODA loop.

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

Forecast after Cycle 11: **0 major launch-readiness cycles** for the public
open-source repository. Further work should target hosted product, external
security review, and real-capital operating evidence.

| Cycle | Target | Expected Score |
|---|---|---:|
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

Current progress:

- Every HTTP response carries `X-Zero-Trace-Id`.
- HTTP `POST /execute` writes the request trace into the paper decision journal
  and echoes it in the HTTP response; idempotency replays preserve the original
  decision trace.
- `GET /metrics` exposes runtime counters for API calls, status codes, execute
  outcomes, idempotency hits, decisions, fills, rejections, open positions, and
  recovery state.
- `GET /audit/export?limit=100` returns a structured `zero.audit.v1` packet
  with summary, retention/redaction metadata, metrics, recovery state, and
  recent decisions.
- Local and Railway smoke tests verify traced execution, metrics, and audit
  export.

### Cycle 7: Live Custody Preflight

Target score: 84/100.

- Add local-only Hyperliquid API wallet setup.
- Add key redaction, permission checks, account-read verification, and dry-run
  order validation.
- Add `zero doctor` checks that refuse live mode until custody, exchange, risk,
  journal, and emergency controls are all present.

Exit gate:

- Live mode cannot start without a passing custody and safety preflight.

Current progress:

- CLI config now has explicit Hyperliquid custody metadata and emergency-control
  fields while keeping private keys out of `config.toml`.
- `zero-config` has local-only Hyperliquid private-key helpers for OS keychain
  and environment resolution, plus strict key/address validation and redaction.
- `GET /live/preflight` emits `zero.live_preflight.v1`, refuses live mode by
  default, verifies wallet/key shape, account-read access, dry-run order
  validation, durable journal presence, risk limits, and emergency controls.
- `zero doctor` reads the live-preflight gate and warns until controls are
  present; future live start paths can require the same row to pass.
- Local and Railway smoke tests verify the preflight endpoint remains non-secret
  and returns `live_mode=refused` in public paper deployments.

### Cycle 8: Self-Custodial Live Execution

Target score: 90/100.

- Add live executor with idempotency, no-retry-on-order-submit, kill switch,
  dead-man switch, reduce-only flatten, max notional, max loss, and max order
  rate.
- Add exchange outage drills, kill-switch drills, and flatten drills.

Exit gate:

- Operators can start live mode only after preflight and can immediately pause,
  kill, or flatten exposure from the CLI.

Current progress:

- Added `zero_engine.live.LiveExecutor` with idempotent submit, deterministic
  Hyperliquid client order IDs, dead-man heartbeat, pause/resume, kill switch,
  reduce-only flatten, max notional, daily loss, and order-rate limits.
- Added an optional `HyperliquidSdkAdapter` behind the `engine[live]`
  dependency group; paper installs and Railway paper deploys do not need it.
- `POST /execute` now routes to live execution only when `X-Zero-Mode: live`
  is sent and a local live executor is configured; otherwise it fails closed.
- Added `POST /live/heartbeat`, `/live/pause`, `/live/resume`, `/live/kill`,
  and `/live/flatten` plus CLI wiring for `/kill`, `/pause-entries`, and
  `/flatten-all`.
- Local and Railway smoke tests now assert public paper deployments refuse live
  execution and live controls with `live executor not configured`.

### Cycle 9: ZERO Network

Target score: 94/100.

- Add opt-in public profile publishing contract.
- Add verified paper/live badge protocol.
- Add leaderboard data model.
- Keep default runtime private.

Exit gate:

- Operators can publish verified public behavior without leaking credentials,
  private notes, or non-consented strategy details.

Current progress:

- Added `zero.network.profile.v1` public-safe profile packets with aggregate
  behavior metrics, proof hashes, privacy metadata, and verification badges.
- Added `zero.network.leaderboard.v1` rows derived from the same redacted
  profile packet.
- Added `POST /network/publish` with explicit consent plus
  `ZERO_NETWORK_PUBLISH_PATH`; the public runtime writes a local JSONL proof log
  and does not upload to a hosted ZERO service.
- Added tests and smoke checks proving public profiles exclude raw decisions,
  trace IDs, idempotency keys, per-trade symbols, wallet material, exchange
  order details, and strategy source labels.

### Cycle 10: ZERO Intelligence

Target score: 97/100.

- Add API key, rate-limit, billing, and dataset model.
- Ship delayed public snapshots and realtime paid feed contracts.
- Add cohort, benchmark, webhook, and export contracts.

Current progress:

- Added `zero.intelligence.snapshot.v1` delayed public intelligence packets
  derived from verified ZERO Network proof.
- Added `zero.intelligence.catalog.v1` with public/commercial packaging,
  hosted API auth shape, rate-limit headers, dataset names, scopes, and plan
  boundaries.
- Added `POST /intelligence/export` with explicit consent plus
  `ZERO_INTELLIGENCE_EXPORT_PATH`; the public runtime writes a local JSONL
  packet log and does not upload to a hosted ZERO service.
- Added tests and smoke checks proving intelligence packets exclude raw
  decisions, trace IDs, idempotency keys, per-trade symbols, exchange
  credentials, private notes, and strategy source labels.

Exit gate:

- Hosted commercial users can pay for speed, history, scale, webhooks, exports,
  and redistribution while the runtime stays open.

### Cycle 11: Production Hardening

Target score: 100/100.

- Add chaos drills, exchange outage drills, restart drills, key-rotation drills,
  and incident runbooks.
- Add security review, threat model update, and signed release policy.
- Add Homebrew/package registry distribution once names are secured.

Current progress:

- Added a public threat model covering custody, live execution, public packet
  privacy, dependency/release compromise, Railway, and contributor bypass
  risks.
- Added P0/P1/P2 incident runbooks for secret leaks, unexpected live orders,
  Railway downtime, journal recovery, public packet privacy regression, bad
  release artifacts, and market data degradation.
- Added distribution readiness policy for GitHub Release, PyPI, crates.io,
  Homebrew, and container channels with promotion and rollback gates.
- Added `scripts/hardening_gate.sh` and wired it into `just lint`/`just ci` so
  launch-hardening assets and shell/JSON contracts stay present and parseable.
- Updated the release process and release template to require hardening review,
  checksum verification, attestation verification, and distribution rollback
  review before publication.

Exit gate:

- The production scorecard reaches at least 95 in every dimension, and no live
  capital path is blocked by missing safety, audit, deployment, or recovery
  work.
