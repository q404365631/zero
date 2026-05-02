# ZERO Autonomous OS Completion Plan

This plan defines the path from the current open-source launch repository to a
complete ZERO autonomous operating system for self-custodial onchain
operations.

The current public repo is strong as an open-source product page, contributor
surface, CLI, paper runtime, and safety-first launch artifact. It is not yet a
complete autonomous real-capital operating system. The remaining work is mostly
runtime truth, live exchange evidence, multi-operator isolation, public Network
ingestion, and commercial Intelligence infrastructure.

## North Star

ZERO reaches 100/100 when a serious operator can:

- deploy ZERO locally, in Docker, or on Railway without private ZERO
  infrastructure;
- run paper mode on live Hyperliquid market data;
- run a continuous OODA loop with strategy runners, risk gates, execution
  policy, and immune controls;
- inspect every accepted and rejected decision through the terminal;
- switch to live Hyperliquid execution only after local custody, journal,
  risk, liveness, and emergency controls pass;
- reconcile open orders, fills, positions, equity, funding, and exchange
  failures against Hyperliquid;
- pause, kill, flatten, or reduce risk immediately from the CLI;
- restart without losing runtime state, idempotency state, risk state, or
  position truth;
- export an audit bundle that explains every cycle and every decision;
- opt into public ZERO Network publishing without leaking secrets;
- consume delayed public ZERO Intelligence snapshots for free;
- pay for realtime ZERO Intelligence API access when speed, history, scale,
  webhooks, redistribution rights, or support matter.

## Product Boundary

Open:

- ZERO Runtime
- ZERO Terminal
- paper trading
- venue adapters needed for self-custodial operation
- local journals and audit exports
- strategy and market-data extension contracts
- ZERO Network proof contracts, profiles, leaderboards, and delayed public
  snapshots
- Railway and Docker self-host deployment paths

Commercial:

- realtime ZERO Intelligence API
- historical intelligence datasets
- cohorts, benchmarks, webhooks, exports, redistribution rights, SLAs, and
  enterprise support
- hosted ingestion and reliability commitments for the Intelligence product

ZERO should not sell basic execution, custody, or safety as proprietary
features. ZERO should sell advantaged access to verified autonomous behavior at
speed, scale, history, and reliability.

## Current Baseline

| Dimension | Current | 100/100 Gap |
|---|---:|---|
| Public repo hygiene | 99 | registry ownership, external release drill |
| Product narrative | 98 | keep narrative aligned as runtime becomes real |
| CLI readiness | 97 | richer TUI cockpit layout, operator automation examples |
| Engine runtime | 90 | live canary evidence, richer exchange history reconciliation |
| Safety and risk | 94 | real exchange chaos drills, external review |
| API contracts | 96 | hosted auth/rate contracts, signed live runtime packets |
| Deployment | 84 | live Railway proof, remote logs, doctor automation |
| Observability and audit | 95 | signed bundles, metrics backend, log drains |
| Security and custody | 91 | external review, key-handling drill evidence |
| ZERO Network | 58 | hosted ingestion, anti-gaming, identity, public pages |
| ZERO Intelligence | 56 | hosted API, billing, history, webhooks, terms |
| Release and distribution | 90 | registries, Homebrew, release rollback rehearsal |
| Operator docs | 98 | real exchange drill evidence |

## Execution Cycles

Each cycle should land as a small, reviewable merge to `main` with tests, docs,
and a scorecard update. A cycle is done only when local `just ci` and remote
CI/security workflows pass.

### Cycle 12: Autonomous Runtime Loop

Target score: 78.

Build the public OODA controller as the engine source of truth:

- `observe`: collect market, account, journal, operator, and liveness state.
- `orient`: derive regime, exposure, stale data, and risk posture.
- `decide`: run strategy runners and risk policy to produce intents or
  rejections.
- `act`: dispatch to paper or live execution bus.
- `learn`: append audit records and calibration signals.

Exit gate:

- `zero-engine-run --once` produces one complete cycle record.
- `zero-engine-run --interval 5` can run continuously.
- Restart recovery preserves last cycle, idempotency keys, positions, and
  rejection counts.

Current progress:

- `zero-engine-run --once` runs one paper OODA cycle from a public scenario.
- `RuntimeLoop` records explicit observe, orient, decide, act, and learn
  phases as `zero.runtime.cycle.v1`.
- Decision journals recover through the existing `PaperEngine` replay path, so
  later runtime invocations continue at the next scenario intent instead of
  duplicating the first action.
- `examples/runtime-loop` demonstrates a bounded paper cycle with temporary
  decision and cycle journals.

### Cycle 13: Strategy Runner SDK

Target score: 80.

Turn examples into a real contributor SDK:

- define `StrategyRunner` and `MarketLens` protocols;
- load declarative YAML strategies and Python plugins in paper mode;
- require paper-only defaults for community plugins;
- add conformance fixtures for runner outputs, risk labels, and failure modes.

Exit gate:

- a new strategy can be added with one file plus one fixture;
- malformed runners fail closed;
- strategy output cannot bypass risk evaluation.

Current progress:

- `StrategyRunner`, `StrategyRunnerMetadata`, and `DeclarativeStrategyRunner`
  define a paper-first runner SDK.
- `load_strategy_runner` loads JSON or ZERO's dependency-free YAML subset.
- `assert_runner_conformance` emits deterministic
  `zero.strategy_runner.conformance.v1` packets for review and CI fixtures.
- `examples/strategy-runner` demonstrates one-file declarative strategy
  contribution.
- Tests prove runner output still goes through `PaperEngine.submit` and can be
  rejected by risk limits.

### Cycle 14: Durable Runtime Bus

Target score: 83.

Replace process memory assumptions with a durable local bus:

- event log for cycles, decisions, fills, positions, health, and operator
  commands;
- state snapshots for fast boot;
- append-only journal integrity checks;
- SQLite or JSONL-backed local store with a clean interface for future
  Postgres mirroring.

Exit gate:

- kill and restart during a paper loop recovers state without duplicated fills;
- audit export can reconstruct the session from disk only.

Current progress:

- `DurableRuntimeBus` writes dependency-free local
  `zero.runtime.event.v1` events to `events.jsonl`.
- Events are checksum-chained through `previous_checksum` and `checksum`, and
  `verify_integrity()` fails closed on mutation, deletion, reorder, or chain
  break.
- `zero-engine-run --runtime-bus DIR` records runtime cycles, decisions,
  fills, rejections, position snapshots, and health events.
- `state-snapshot.json` stores the latest fast-boot state with event count and
  last checksum.
- `export_audit()` returns `zero.runtime.audit.v1` from disk only, including
  integrity status, event type counts, latest snapshot, and events.

### Cycle 15: Hyperliquid Account Reconciliation

Target score: 86.

Make Hyperliquid account truth explicit before expanding live trading:

- read open orders, fills, positions, margin, funding, and account equity;
- reconcile local state against exchange state;
- classify drift as stale data, local lag, exchange rejection, or critical
  mismatch;
- fail live risk-increasing actions when reconciliation is stale or mismatched.

Exit gate:

- `/hl/account` and CLI readouts are available without exposing secrets;
- reconciliation fixtures cover partial fills, canceled orders, stale mids,
  missing orders, and drift.

Current status:

- `GET /hl/account` returns normalized `zero.hl_account.v1` account snapshots
  from read-only `clearinghouseState` and `openOrders` calls.
- `GET /hl/reconcile` returns `zero.reconciliation.v1` with stale-data,
  local-lag, exchange-rejection, and critical-mismatch classifications.
- The Rust CLI exposes `/hl-account` and `/hl-reconcile` readouts backed by
  typed engine-client models and mock-engine contract coverage.
- Live risk-increasing `POST /execute` now fails closed unless reconciliation
  allows risk increase; reduce-only controls remain available.
- Remaining scope before Cycle 16: richer fill/funding/order-history
  reconciliation fixtures.

### Cycle 16: Live Execution Certification Harness

Target score: 89.

Promote live primitives into a certified operating path:

- exchange adapter conformance tests;
- fake exchange chaos harness;
- tiny-capital live canary runbook;
- dead-man, cancel-all, kill, pause, flatten, and reduce-only drills;
- evidence bundle template for live rehearsal.

Exit gate:

- no live start without passing preflight, reconciliation, durable journal, and
  certification checks;
- a dry-run or tiny-live report can prove each emergency path worked.

Current status:

- `LiveExecutor` turns exchange submit outages into auditable `exchange_error`
  records and does not retry order submissions.
- `GET /live/certification` returns `zero.live_certification.v1` dry-run
  evidence for heartbeat, idempotency, exchange outage, pause, reduce-only
  flatten, kill, dead-man rejection, order-rate, and daily-loss drills.
- The Rust CLI exposes `/live-certify` with typed client coverage and mock
  engine contract coverage.
- [Live Certification](live-certification.md) documents the evidence bundle and
  tiny-capital canary procedure.
- Remaining scope before Cycle 17: execute the tiny-capital canary only after
  explicit operator approval and preserve real exchange-side evidence.

### Cycle 17: Immune System And Circuit Breakers

Target score: 91.

Build the protective layer as first-class runtime code:

- stale data breaker;
- max drawdown breaker;
- daily loss and per-symbol exposure breaker;
- order velocity breaker;
- exchange error breaker;
- operator inactivity breaker;
- manual kill file and terminal kill command priority.

Exit gate:

- every breaker has fixtures, metrics, audit records, and CLI rendering;
- risk-reducing commands continue to work while risk-increasing actions are
  blocked.

Current status:

- `zero.immune.v1` models risk-blocking breaker state for stale market data,
  reconciliation, dead-man freshness, operator pause, kill switch, daily loss,
  order velocity, exchange submit errors, operator inactivity, and max exposure.
- `GET /immune` exposes the packet; `/health`, `/metrics`, `/audit/export`,
  and `/live/preflight` embed it so breaker state is visible in operations and
  evidence bundles.
- Live risk-increasing execution checks the immune packet after reconciliation
  and before order submission. Risk-reducing controls remain available.
- The Rust CLI exposes `/immune` with typed client and mock-engine coverage.
- [Immune System](immune-system.md) documents breaker semantics and live-start
  behavior.
- Remaining scope before Cycle 18: add richer TUI cockpit layout and real
  canary evidence for exchange-side breaker behavior.

### Cycle 18: Operator Terminal Live Cockpit

Target score: 92.

Make the CLI/TUI the safety-preserving operator interface for the real runtime:

- cycle status, exchange status, reconciliation status, breaker state, and
  journal tail;
- live heartbeat visibility;
- preflight and refusal reasons surfaced plainly;
- one-command `pause`, `kill`, `flatten`, and `resume` flows;
- non-interactive `zero run` examples for supervised operations.

Exit gate:

- an operator can diagnose and reduce risk from the terminal without using raw
  HTTP calls.

Current status:

- `GET /live/cockpit` emits `zero.live_cockpit.v1`, joining preflight,
  reconciliation, immune breakers, dry-run certification, heartbeat, recent
  live records, operator actions, and the next required action.
- The Rust client decodes the cockpit packet and `zero run live-cockpit`
  renders the live-mode state, failed checks, open breakers, certification
  count, heartbeat expiry, and risk-reducer commands.
- `/resume-entries` is wired as a friction-gated live resume command; `/kill`,
  `/flatten-all`, and `/pause-entries` remain instant risk reducers.
- Local, Railway, mock-engine, and OpenAPI contract checks cover the cockpit.
- [Live Cockpit](live-cockpit.md) documents the operator workflow and canary
  evidence boundary.
- Remaining scope before Cycle 19: richer full-screen TUI cockpit layout and
  real tiny-capital canary evidence after external live approval.

### Cycle 19: Multi-Operator Foundation

Target score: 94.

Prepare ZERO as a substrate, not a single-operator script:

- `OperatorContext` for all runtime state, custody config, bus paths, and model
  config;
- per-operator local filesystem partitions;
- signed deployment identity and heartbeat protocol;
- local-first deployment claim contract;
- public schema that a hosted control plane can consume later without making
  paper mode depend on it.

Exit gate:

- two operators can run isolated local deployments from the same checkout;
- state, journals, profiles, and credentials cannot cross partitions.

Current status:

- `zero.operator_context.v1` is resolved from `X-Zero-Operator-*` headers,
  `ZERO_OPERATOR_*` environment variables, or the local default.
- `GET /operator/context` exposes the active audit identity.
- `/live/cockpit`, `/audit/export`, live control responses, and live execution
  records include operator context.
- Live control attempts are appended to an in-memory operator action log and
  surfaced in cockpit/audit packets.
- The Rust CLI attaches `identity.handle` as operator context when config is
  present and renders the resolved operator line in `zero run live-cockpit`.
- Local mutable CLI state now lands under
  `<zero_dir>/operators/<operator-slug>/`, including `state.db`, `zero.log`,
  the headless socket, headless state, and daily wraps.
- OS keychain writes use `operator:<operator-slug>` account names for engine
  and Hyperliquid secrets, with legacy `default` reads kept only for migration
  compatibility.
- `zero doctor` includes `operator_partition` and `credential_partition` rows,
  and warns when legacy shared state artifacts remain at the top level.
- Added `zero.deployment.claim.v1` with local deployment metadata, public-safe
  operator identity, aggregate evidence counts, signature-ready claim hash, and
  explicit `unsigned_local` status unless external signing metadata is supplied.
- Added `zero.deployment.heartbeat.v1` with public-safe paper/live liveness,
  dead-man freshness, heartbeat hash, and optional external signature metadata.
- `GET /deployment/claim`, `/deployment/heartbeat`, `/network/profile`,
  `/network/leaderboard`, `/intelligence/snapshot`, and `/audit/export` now
  share deployment claim and heartbeat hashes so a hosted Network or
  Intelligence API can verify packet lineage later.
- Remaining scope before Cycle 20: real hosted verification service design.

### Cycle 20: LLM Gateway

Target score: 95.

Add provider-agnostic intelligence plumbing without making trading depend on a
single model vendor:

- `ModelClient` protocol;
- Anthropic, OpenAI, Ollama, and OpenRouter adapters;
- capability tiers for hard reasoning, fast reasoning, chat, and embeddings;
- structured output validation and retry rules;
- usage and cost event recording;
- provider conformance suite.

Exit gate:

- the runtime can evaluate through mock/local providers in CI;
- live providers are optional and configured per operator;
- model failure degrades safely instead of inventing certainty.

### Cycle 21: ZERO Network Ingestion And Anti-Gaming

Target score: 96.

Make public proof a real open product surface:

- signed proof packets;
- local publish queue;
- anti-gaming rules for duplicate handles, replayed packets, fake volume, sybil
  profiles, and stale publication;
- public-safe identity and verification badges;
- hosted-compatible ingestion contract.

Exit gate:

- profile and leaderboard data can be accepted, rejected, replayed, and audited
  without private runtime data.

### Cycle 22: ZERO Intelligence API

Target score: 97.

Build the commercial data product around verified autonomous behavior:

- hosted API contract for snapshots, cohorts, benchmarks, webhooks, and
  exports;
- bearer API keys, scopes, rate-limit headers, and usage events;
- delayed public snapshots remain open;
- realtime/history/scale/webhooks/export rights are commercial;
- billing-ready plan boundaries.

Exit gate:

- public delayed data and commercial realtime data are separate, tested, and
  documented;
- no exchange credentials or raw private journals are required.

### Cycle 23: Production Deployment And Remote Operations

Target score: 98.

Make Railway the first-class self-custodial deployment path:

- volume and secret checks;
- remote log/doctor automation;
- health, metrics, and recovery checks;
- deployment smoke runbook;
- rollback and incident drills.

Exit gate:

- a new operator can deploy paper mode to Railway, run live-data paper, inspect
  logs, export audit, and recover from restart.

### Cycle 24: Release, Registry, And Supply Chain

Target score: 99.

Finish distribution:

- publish package registry ownership plan;
- Homebrew tap;
- signed artifacts and checksums;
- SBOM/provenance;
- install rollback;
- dependency update and security response policy.

Exit gate:

- release process is reproducible by a maintainer other than the founder.

### Cycle 25: External Review And Real-World Evidence

Target score: 100.

Earn the final points with evidence:

- external security review;
- external operator usability review;
- live exchange chaos rehearsal;
- tiny-capital live canary report;
- public incident-style postmortem template;
- scorecard update with links to evidence artifacts.

Exit gate:

- every 100/100 claim links to tests, docs, CI, release artifacts, drill logs,
  or external review notes.

## Work Order

Do cycles 12 through 18 before hosted Network or Intelligence expansion. A
commercial API built before runtime truth would sell a weak dataset. Runtime
truth, reconciliation, and safety evidence create the verified behavior that
ZERO Intelligence monetizes.

Then do cycles 19 through 22 to turn the single runtime into a multi-operator
substrate and commercial data product.

Finish with cycles 23 through 25 to prove deployment, distribution, and
external trust.

## Non-Negotiables

- Paper mode stays deterministic and free.
- Live mode stays self-custodial.
- Risk-reducing commands are always easier than risk-increasing commands.
- Hosted systems never require custody.
- Public surfaces never leak wallets, private keys, exchange order IDs, raw
  journals, strategy labels, private notes, or per-trade symbols unless a future
  explicit consent contract is designed and reviewed.
- Every live-capable feature needs a refusal path, a test, a runbook entry, and
  CLI visibility.
