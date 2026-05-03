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
| Self-evolution | Memory extracts rules, research explains what to study, genesis proposes/builds changes, red-team attacks diffs, canary/calibration gates promotion. | Memory core, research command chain, genesis proposal classification, and paper-only evolve gates are now present as public subsystems. | Add real mutation/promotion and rollback. |
| Research command chain | Hunt, edge, convergence, thesis, score, meta, and sharpen form a learning/research loop. | Public docs mention autonomous OS, but not the full command chain. | Add public command contracts and deterministic fixture-backed reports. |
| Real decision engine | Multi-lens evaluation, layered signals, risk gates, sizing modifiers, and rejection learning. | Public runtime has paper engine, runners, safety, and live-readiness primitives. | Port lens/layer/modifier interfaces and fixtures before porting live behavior. |
| MCP surface | Internal MCP can inspect and operate many engine surfaces. | Public MCP exposes a minimal read-only paper demo. | Expand read-only and risk-reducing local MCP tools with explicit safety classes. |
| Live canary lifecycle | Readiness, policy, launch, evidence, report, qualification, shadow review, follow-through. | Public has rehearsal, evidence, verification, and operator report flows. | Add policy/follow-through/qualification contracts and fixtures. |
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

### Cycle 32: Public Intelligence Engine Parity

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

## Revised Score

Public repo readiness remains **100/100** as a launch artifact.

Full ZERO operating-system readiness is **94/100** after Cycle 31. The score
increased because public research-chain evidence now exists as code, tests,
docs, API readouts, and MCP snapshots. The remaining self-evolution loop is
still core product architecture, not polish.

The path back to 100/100 is now clearer:

1. real mutation/promotion and rollback loop;
2. real lens/layer/modifier decision interfaces;
3. expanded agent/MCP operation surface;
4. live canary policy parity and operator-owned exchange evidence.
