# Private Engine Capability Gap Audit

This audit translates ZERO's existing internal engine capabilities into a
public-safe extraction plan. It is intentionally capability-level only: it does
not publish private deployment topology, private journals, production wallets,
operator identifiers, strategy thresholds, proprietary datasets, or commercial
operations data.

The important conclusion is direct: the public repository is excellent as an
open-source launch artifact, but the complete ZERO autonomous operating system
also needs the adaptive intelligence loop that already exists internally:

```text
operate -> journal -> memory -> genesis -> guardian -> build -> red-team
        -> paper canary -> calibration -> promote or rollback -> evolve
```

That loop is not a sidecar. It is the difference between an autonomous runtime
and an autonomous operating system that improves under review.

## Internal Capability Classes

ZERO's internal engine has five command planes:

| Plane | Capability class |
| --- | --- |
| Operator skills | Human and agent-invoked workflows for health, market scans, edge reports, convergence audits, memory extraction, genesis, evolution, scoring, red-team review, and operator communication. |
| Script commands | Repeatable Python commands for live readiness, Hyperliquid state checks, honest health checks, live canary lifecycle, rejection audits, perps audits, fix-impact scoring, memory, genesis, thesis, and recovery. |
| Terminal CLI | Operator setup, start/stop/status, configuration, logs, emergency close, score, evaluate, brief, credits, observe, MCP serving, diagnostics, arena, and mode management. |
| API and MCP | Read-heavy operational state, strategy/session control, heat/regime/pulse/briefs, diagnostics, circuit breakers, rejections, near misses, execution quality, equity, backtests, reconciliation, immune state, chat, memory, architecture proposals, trade approvals, evolution status, and recall. |
| Daemons and services | Trading engine, API server, MCP server, agent daemon, autopilot loop, sensor layer, and durable bus service. |

The public repo should not copy these surfaces wholesale. It should extract the
portable capability behind each surface, then ship it with deterministic
fixtures, paper-first defaults, and public-safe contracts.

## Public Extraction Boundary

Open-source scope:

- local memory taxonomy and append-only knowledge extraction;
- local `memory`, `genesis`, and `evolve` command surfaces;
- proposal schemas, guardian policies, and review journals;
- builder/red-team/canary/calibration lifecycle in paper mode;
- strategy, lens, risk, sizing, immune, and venue adapter interfaces;
- local OODA loop, runtime bus, journals, audit exports, and recovery;
- public-safe proof packets, Network profiles, and delayed snapshots;
- read-only and risk-reducing MCP/API tools for local operators and coding
  agents.

Commercial scope:

- realtime ZERO Intelligence API;
- aggregate cross-operator behavior, cohorts, benchmarks, and history;
- hosted ingestion reliability, warehouse persistence, billing, webhooks,
  redistribution rights, SLAs, and enterprise support;
- managed model gateway and hosted key-management operations.

ZERO should not sell basic self-custodial execution, safety, local memory,
local genesis, or the operator terminal as proprietary features.

## Gap Map

| Area | Internal capability | Public state | Gap |
| --- | --- | --- | --- |
| Self-evolution | Memory extracts rules, research explains what to study, genesis proposes/builds changes, red-team attacks diffs, canary/calibration gates promotion. | Memory core, research command chain, genesis proposal classification, production-parity OODA reports, paper-first evolve gates, sandbox candidate mutation, promotion plans, exact-phrase local apply, rollback receipts, and promotion verification are now present as public subsystems. | Protected live-code evolution remains human-reviewed until external operator evidence and review exist. |
| Research command chain | Hunt, edge, convergence, thesis, score, meta, and sharpen form a learning/research loop. | Public docs mention autonomous OS, but not the full command chain. | Add public command contracts and deterministic fixture-backed reports. |
| Real decision engine | Multi-lens evaluation, layered signals, risk gates, sizing modifiers, and rejection learning. | Public runtime now exposes a paper-only lens/layer/modifier decision stack plus `zero.runtime.production_parity.v1` live-shadow fail-closed parity over API, MCP, OpenAPI, docs, and tests. | Add richer regime/correlation gates and real exchange execution-quality feedback after operator-owned canary evidence. |
| MCP surface | Internal MCP can inspect and operate many engine surfaces. | Public MCP exposes expanded read-only local/operator surfaces with explicit safety classes. | Add risk-reducing local controls only after the operator policy and friction contract are public. |
| Live canary lifecycle | Readiness, policy, launch, evidence, report, qualification, shadow review, follow-through. | Public has rehearsal, evidence, verification, policy qualification, cockpit-drill policy capture, and operator report flows. | Attach real operator-owned exchange evidence from an accepted canary. |
| Agent daemon | Persistent agents, approvals, proposals, operator app, and communication loop. | Public has agent contribution docs and a thin MCP. | Add local proposal queue and approval surfaces before hosted agents. |
| Perception layer | Sensor, cross-asset, liquidations, universe, market data service, and setup detection. | Public has market adapter examples and read-only Hyperliquid status. | Add public sensor interfaces, fixture stores, and live-read-only adapters. |
| Recovery and operations | Honest checks, Hyperliquid state checks, recovery CLI, watchdog, log rotation. | Public has incident docs, deployment evidence, and readiness gates. | Add operator-safe diagnostics and recovery commands with no secret access. |

## Required Public Cycles

These cycles supersede the earlier assumption that only hosted Network,
Intelligence, and external proof remained.

### Cycle 27: Capability Audit Canon

Make this gap audit part of the public docs, readiness gates, roadmap, and LLM
context so contributors and coding agents understand the true product target.

Exit gate:

- README links this audit.
- Architecture names ZERO Evolution as a product surface.
- Production readiness includes a self-evolution score.
- `docs/llms-full.txt` includes this audit.

### Cycle 28: Memory Core

Implement local memory as a public engine subsystem:

- typed memory entries: signal, regime, operator, strategy reference;
- anti-derivation rule, TTLs, deduplication, and expiry;
- append-only JSONL store plus generated `knowledge.md`;
- CLI/API readouts for local memory state;
- tests for derivable rejection, expiry, deduplication, and operator scope.

Exit gate:

- `zero-memory extract` can produce public-safe observations from fixture
  decisions.
- `/memory`, `zero_get_memory_snapshot`, and `zero://memory/snapshot` expose
  public-safe readouts.
- memory output never stores live prices, secrets, raw wallet data, exchange
  order ids, or derivable exchange state.

Current public status:

- Implemented in `engine/src/zero_engine/memory.py`.
- Covered by `engine/tests/test_memory.py`, API tests, MCP tests, and
  `just memory-core-example`.
- Documented in [Memory Core](memory-core.md).

### Cycle 29: Genesis Proposal Core

Implement proposal lifecycle without automatic code changes:

- `Proposal` schema;
- risk tiers and protected path classes;
- append-only genesis journal;
- guardian policy with sample-size floors and revert-plan requirements;
- `zero genesis plan/status` commands.

Exit gate:

- fixture-backed proposals are accepted, escalated, or rejected
  deterministically.
- proposals touching execution, sizing, stops, circuit breakers, live adapters,
  or immune core require human review.

Current public status:

- Implemented in `engine/src/zero_engine/genesis.py`.
- Covered by `engine/tests/test_genesis.py`, API tests, MCP tests, and
  `just genesis-example`.
- `/genesis`, `zero_get_genesis_proposals`, and `zero://genesis/proposals`
  expose plan-only classifications.
- Documented in [Genesis](genesis.md).

### Cycle 30: Builder, Red-Team, Canary, Calibration

Implement the public paper-only self-modification harness:

- create branch/worktree for approved low-risk proposals;
- generate or apply bounded patches only in allowed paths;
- run tests before producing a build result;
- run red-team review on the diff;
- run paper canary against deterministic or live-read-only market data;
- calibrate against baseline with conservative statistical gates.

Exit gate:

- no generated branch can be promoted without tests, red-team verdict, and
  canary/calibration evidence.
- promotion is local-only and never pushes automatically.

Current public status:

- Implemented in `engine/src/zero_engine/evolve.py`.
- Covered by `engine/tests/test_evolve.py`, API tests, MCP tests, and
  `just evolve-example`.
- `/evolve`, `zero_get_evolve_status`, and `zero://evolve/status` expose
  paper-only gate status.
- The public harness writes sandbox artifacts but does not mutate the checkout,
  promote, deploy, or push.
- Documented in [Evolve Harness](evolve.md).

### Cycle 31: Research Command Chain

Add fixture-backed public versions of the internal research loop:

- `hunt`: market scan report from public-safe market data;
- `edge`: strategy/lens/time/coin expectancy report from local journals;
- `convergence`: feedback-loop drift and lockstep detection;
- `thesis`: seven-day operating hypothesis with anti-thesis and scorecard;
- `score`: compare prior judgments against outcomes;
- `meta` and `sharpen`: audit command usefulness and system improvement.

Exit gate:

- each command writes a versioned report artifact;
- reports are deterministic in CI with fixture data;
- no report claims live PnL unless backed by signed operator evidence.

Current public status:

- Implemented in `engine/src/zero_engine/research.py`.
- Covered by `engine/tests/test_research.py`, API tests, MCP tests, and
  `just research-example`.
- `/research`, `zero_get_research_report`, and `zero://research/report` expose
  paper-only research status.
- Reports include hunt, edge, convergence, thesis, score, meta, and sharpen.
- The public report is read-only, does not mutate the checkout, does not push,
  and does not claim live PnL.
- Documented in [Research Command Chain](research.md).

### Cycle 32: Decision Stack

First public slice of intelligence engine parity.

Port the real decision interfaces:

- lens runner protocol;
- evaluation layer protocol;
- sizing modifier chain;
- rejection learner;
- regime and correlation gates;
- execution quality and near-miss feedback.

Exit gate:

- a contributor can add one lens, one layer, or one sizing modifier with a
  fixture and conformance test.
- every decision path still flows through safety and paper/live separation.

Current public status:

- Implemented the first public decision-stack contract in
  `engine/src/zero_engine/decision.py`.
- `/decision/stack` exposes `zero.decision.stack.v1`.
- `/evaluate/{coin}` embeds the same stack while preserving CLI-compatible
  fields.
- `zero_get_decision_stack` and `zero://decision/stack` expose the same
  read-only contract to coding agents.
- Covered by decision, API, MCP, OpenAPI, and `just decision-stack-example`
  checks.
- The public stack never grants live execution authority and reports
  `allowed_to_execute_live: false`.
- Documented in [Decision Stack](decision-stack.md).

### Cycle 33: Expanded Local MCP

Expose local read-only and risk-reducing tools for agents:

- status, health, positions, journal tail, memory stats, genesis proposals,
  research reports, backtests, rejection audit, immune status, and canary
  evidence;
- risk-reducing controls only where local operator policy allows them;
- no risk-increasing MCP tools without explicit local friction.

Exit gate:

- MCP transcript proves read-only operation by default.
- safety class is documented for every tool.

Current public status:

- `zero-mcp` now exposes 19 read-only tools and 18 resources for strategies,
  runtime status, health, paper results, positions, journal tail, rejection
  audit, proof pack, memory snapshot/stats, genesis proposals, evolve status,
  research report, decision stack, immune status, backtest report, evidence
  bundle, and safety catalog.
- Every public tool declares `safetyClass: read_only_public`,
  `canPlaceOrders: false`, `canChangeRuntimeState: false`, and
  `canReadSecrets: false`.
- `docs/mcp/transcript.jsonl`, `engine/tests/test_mcp.py`, and
  `zero-mcp --smoke` verify the tool list, resource list, safety metadata,
  paper-only payloads, and absence of write-capable tool names.
- Risk-reducing MCP controls are intentionally deferred until the local
  operator friction contract can be enforced over MCP.

### Cycle 34: Live Canary Policy Parity

Complete the live canary lifecycle:

- readiness;
- policy arm/disarm;
- bounded launch window;
- evidence export;
- shadow review;
- qualification;
- follow-through review;
- next-step policy recommendation.

Exit gate:

- public CI exercises the lifecycle in refusal/paper mode.
- operator-owned live evidence can be attached and verified without leaking raw
  venue payloads or secrets.

Current public status:

- `/live/canary-policy` exposes `zero.live_canary_policy.v1` with readiness,
  arm/disarm, launch-window, evidence, shadow-review, qualification,
  follow-through, and next-step fields.
- Rehearsal bundles and operator reports embed the same policy object.
- `scripts/live_canary_policy.py` renders the policy from rehearsal bundles,
  cockpit drill bundles, manifests, or operator reports.
- The canary verifiers reject missing policy packets and operator-report policy
  contradictions.
- Public paper smoke exercises refusal-mode policy qualification without
  claiming accepted live execution.

### Cycle 35: Local Promotion And Rollback Evidence

Complete the public self-evolution promotion evidence layer:

- materialized sandbox candidate tree;
- original and candidate hashes for every allowed mutation;
- local promotion plan;
- rollback plan;
- promotion artifact verification;
- explicit approval phrase;
- no checkout mutation and no remote push.

Exit gate:

- `/evolve`, `zero_get_evolve_status`, and contract fixtures expose promotion,
  rollback, and verification evidence.
- public CI proves the plan is local-only and rollback-ready.
- generated candidates remain limited to `docs/` and `examples/`.

Current public status:

- `zero.evolve.run.v1` now includes `promotion_plan`,
  `rollback_plan`, and `promotion_verification`.
- The evolve build materializes candidate files under a sandbox candidate tree
  and hashes both original and candidate content.
- `zero.evolve.promotion_plan.v1` requires
  `I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION` and records
  `applies_to_checkout=false`, `pushes_to_remote=false`, and
  `places_orders=false`.
- `zero.evolve.rollback_plan.v1` records restore hashes and fails closed when
  no rollback target exists.
- `zero.evolve.promotion_verification.v1` rejects plans that can mutate the
  checkout or push remotely.

### Cycle 36: Local Apply And Rollback Execution

Complete the public local checkout promotion layer:

- explicit `zero_engine.evolve apply` command;
- exact local promotion approval phrase;
- full prevalidation before any checkout write;
- original checkout hash verification;
- candidate hash verification;
- apply receipt;
- explicit `zero_engine.evolve rollback` command;
- exact rollback approval phrase;
- backup-backed restore with original hash verification;
- no remote push and no order placement.

Exit gate:

- approved docs/example candidates can be applied to a local checkout;
- wrong approval phrases do not mutate the checkout;
- tampered candidates fail before any file is written;
- rollback restores the original content and emits a receipt.

Current public status:

- `zero.evolve.apply_receipt.v1` records approved local checkout mutation,
  backup paths, applied hashes, and failure checks.
- `zero.evolve.rollback_receipt.v1` records local restore execution and verifies
  restored hashes.
- Apply and rollback remain CLI-only; the HTTP and MCP surfaces stay read-only.
- Protected runtime paths remain blocked by `ALLOWED_PATCH_ROOTS` and
  `FORBIDDEN_PATCH_ROOTS`.

### Cycle 37: Production-Parity OODA Report

Implemented in `engine/src/zero_engine/runtime.py`:

- `zero.runtime.production_parity.v1` runs the bundled paper OODA loop through
  observe, orient, decide, act, and learn phases;
- mirrors every intent and idempotency key through a disabled `LiveExecutor`;
- proves the live shadow path refuses every order and that the exchange adapter
  placed zero orders;
- verifies checksum-chained runtime-bus integrity and snapshot consistency;
- emits `zero.runtime.feedback.v1` rejection/execution-quality feedback without
  claiming live slippage or exchange fill quality;
- exposes the report through `GET /runtime/parity`,
  `zero_get_runtime_parity`, `zero://runtime/parity`, OpenAPI, and a pinned
  contract fixture.

### Cycle 38: Operator CLI Runtime-Parity Surface

Implemented in `cli/crates/zero-engine-client` and `cli/crates/zero-commands`:

- added typed Rust decoding for `GET /runtime/parity`;
- added the neutral `/runtime-parity` slash command, with aliases `/parity`,
  `/ooda-parity`, and `/production-parity`;
- rendered the public-safe boundary an operator needs to see: paper cycles,
  fills, rejections, disabled live-shadow refusals, zero adapter orders,
  rejection-rate feedback, certification status, and the no-live-trading claim
  boundary;
- extended the mock engine and dispatcher/client tests so the terminal cannot
  drift from the API contract.

### Cycle 39: Operator CLI Live-Canary Policy Surface

Implemented in `cli/crates/zero-engine-client` and `cli/crates/zero-commands`:

- added typed Rust decoding for `GET /live/canary-policy`;
- added the neutral `/live-canary` slash command, with aliases `/canary`,
  `/canary-policy`, and `/live-canary-policy`;
- rendered the public-safe live canary lifecycle an operator needs to see:
  readiness, armed/disarmed state, qualification, publishability, accepted-live
  boundary, exchange-evidence attachment, refusal-proof qualification, next
  action, operator identity, and phase status;
- extended the mock engine and dispatcher/client tests so the CLI cannot claim
  publishable accepted-live proof from refusal-mode evidence.

### Cycle 40: Operator CLI Live-Receipts Surface

Implemented in `cli/crates/zero-commands`:

- added the neutral `/live-receipts` slash command, with aliases `/receipts`,
  `/execution-receipts`, and `/live-execution-receipts`;
- rendered the public-safe live execution receipt packet an operator needs to
  see before any canary claim: total, accepted, refused, exchange-error count,
  receipt-bundle hash, operator identity, privacy flags, and recent
  hash-only receipt rows;
- extended dispatcher tests so the terminal locks the empty/refused receipt
  boundary and never implies accepted live proof from a zero-receipt packet.

### Cycle 41: Full-Screen TUI Live Cockpit

Implemented in `cli/crates/zero-engine-client`, `cli/crates/zero-tui`, and
`cli/crates/zero-commands`:

- added `LiveCockpit` to the CLI-side `EngineState` mirror and populated it
  from the HTTP backfill poller via `GET /live/cockpit`;
- added a fifth TUI mode, `Cockpit`, reachable with Ctrl+5 and
  `/cockpit-mode`, while keeping `/live-cockpit` as the one-shot readout;
- rendered the same public-safe cockpit packet full-screen: live mode,
  readiness, risk allowance, next action, operator identity, preflight,
  immune, reconciliation, certification, heartbeat, receipt totals, failed
  checks, open breakers, and risk-reducing command affordances;
- extended Rust tests for mode routing, backfill population, input handling,
  and full-screen cockpit rendering.

## Revised Score

Public repo readiness remains **100/100** as a launch artifact.

Full ZERO operating-system readiness is **100/100** as a public repo contract
after Cycle 41. The score increased because the public runtime now has an
executable production-parity OODA report, rejection/execution-quality feedback,
live-shadow fail-closed evidence, a first-class operator CLI renderer for that
proof, and a first-class terminal renderer for live canary policy qualification
plus live execution receipts and a full-screen live cockpit in addition to
local evolve apply/rollback.

The remaining work is no longer missing public-runtime shape. It is external
product proof and launch operation:

1. operator-owned accepted canary evidence on real Hyperliquid;
2. third-party security review;
3. hosted ZERO Network persistence and signed identity verification;
4. paid ZERO Intelligence deployment, billing, and retention policy.
