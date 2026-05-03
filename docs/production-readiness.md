# Production Readiness

This scorecard measures ZERO as a production product, not just as a clean public
repository. It deliberately keeps two scores separate:

- **Public repo readiness:** whether the repository is good enough for serious
  engineers to run, inspect, and contribute to.
- **Full operating-system readiness:** whether the public runtime is complete
  enough for a new operator to run self-custodial live capital end to end.

The public repository is launch-ready for contributors. The production product
is not yet ready for real capital. The gap is intentional and should stay
visible until the runtime can run a self-custodial Hyperliquid operator end to
end.

## Current Score

| Dimension | Score | Status |
|---|---:|---|
| Public repo hygiene | 100 | Strong CI, release artifacts, governance, docs, clean boundaries, first-class GitHub product page, first-10-minutes guide, reproducible demo capture, fresh source-tree rehearsal, threat model, incident runbooks, distribution policy, hardening gate, and public-readiness gate. |
| Product narrative | 99 | Clear category around autonomous operating systems for self-custodial onchain operations, with public runtime, terminal, evolution, network, intelligence, capability boundary, and operator proof path separated cleanly. |
| CLI readiness | 100 | Mature Rust terminal, doctor, five-mode TUI, full-screen live cockpit, friction gates, tests, release binary path, recovery-aware status output, live-preflight diagnostics, `/live-cockpit`, `/live-certify`, `/live-receipts`, `/live-canary`, `/runtime-parity`, `/immune`, friction-gated `/resume-entries`, friction-gated engine-backed `/execute` with receipt-hash rendering, operator-context audit headers, operator-local runtime partitions, a one-command live canary operator evidence workflow, canary policy renderer, and a verified read-only live cockpit drill bundle exist. Real canary evidence remains operator-owned external proof, not a public-runtime CLI gap. |
| Engine runtime | 100 | Deterministic paper runtime, bounded OODA cycle records, strategy runner SDK, declarative paper strategies, public lens/layer/modifier decision stack, append-only decision journal, durable runtime bus contracts, restart replay, read-only Hyperliquid info adapter, live-mid paper execution, traceable audit export, production-parity OODA reports, disabled live-shadow fail-closed evidence, rejection/execution-quality feedback, live custody preflight, account reconciliation, dry-run live certification, immune breaker packets, live execution receipt packets, maintained live canary rehearsal collector, verifier, exchange-evidence normalizer, live canary policy lifecycle, operator report workflow, and operator report verifier, public-safe signed live evidence packets, fail-closed model gateway plumbing, and live-executor interfaces exist. Real exchange canary proof remains operator-owned external evidence. |
| Self-evolution loop | 100 | Local memory, genesis proposal core, paper-only research command chain, public decision-stack review, production-parity OODA reports, and paper-first evolve gates are implemented with typed public-safe entries, append-only JSONL journals, generated `knowledge.md`, `/memory`, `/genesis`, `/research`, `/runtime/parity`, `/decision/stack`, `/evolve`, fixture-backed CLI extraction/classification/research/decision-stack/parity output, guardian sample floors, protected path escalation, hunt/edge/convergence/thesis/score/meta/sharpen reports, lenses/layers/modifiers, red-team review, sandbox candidate mutation, deterministic paper canary, calibration, promotion plan, rollback plan, promotion verification, exact-phrase local apply, apply receipts, rollback receipts, and expanded read-only MCP status/parity/health/journal/rejection/memory/immune/backtest/evidence/safety surfaces. Protected live-code evolution remains human-reviewed by design. |
| Safety and risk | 94 | CLI risk asymmetry, local custody validation, dry-run order validation, preflight refusal, account-reconciliation gate, live-submit idempotency model, no-retry exchange-error records, dry-run live certification drills, `zero.immune.v1` breakers, dead-man heartbeat contract, max notional/loss/order-rate policy, pause, kill, reduce-only flatten, fail-closed canary rehearsal, canary policy qualification, hash-only live evidence capture, verified cockpit drill tamper rejection, threat model, and P0/P1 runbooks exist. Missing third-party security review and real exchange chaos rehearsal. |
| API contracts | 100 | Paper fixtures are pinned across Python and Rust, OpenAPI documents the local paper runtime, compatibility rules are explicit, `/memory` exposes redacted local learning, `/genesis` exposes plan-only proposal classification, `/research` exposes paper-only research reports, `/runtime/parity` exposes production-parity OODA and live-shadow fail-closed evidence, `/decision/stack` exposes lens/layer/modifier evaluation shape, `/evaluate/{coin}` embeds that stack while preserving CLI fields, `/evolve` exposes paper-only evolve gates plus promotion plan, rollback plan, and promotion verification, `/operator/context` exposes audit identity, `/deployment/claim` exposes signature-ready runtime identity, `/deployment/heartbeat` exposes signature-ready public liveness, `/hl/status` exposes read-only market status, `/hl/account` and `/hl/reconcile` expose account truth, `/immune` exposes risk-blocking breaker state, `/live/cockpit` exposes consolidated live operator state, `/live/certification` exposes dry-run safety evidence, `/live/receipts` exposes public-safe local execution receipts, `/live/evidence` exposes a hash-only signed canary evidence bundle, `/live/canary-policy` exposes the live canary lifecycle and qualification contract, `/market/quote` names the active price source, `/health` plus `/v2/status` expose recovery state, `/metrics` plus `/audit/export` expose observable runtime state, `/network/*` exposes public proof packets and hosted-compatible ingestion, `/intelligence/*` exposes delayed intelligence, model gateway status, model gateway health, model gateway audit, and billing-ready commercial API contracts, `/v1/intelligence/*` exposes hosted-compatible auth, scopes, rate-limit headers, usage events, webhook signatures, and export jobs, `/live/preflight` exposes a non-secret live-readiness gate, and `POST /live/*` controls are typed in the CLI. |
| Deployment | 96 | Docker path, Railway config, healthcheck, restart policy, `PORT`-aware start script, durable journal replay, traceable paper decisions, Railway remote doctor, redacted deployment evidence pack, evidence verifier, optional HMAC evidence signature, OpenSSL-backed deployment identity evidence bundle, plan-only deployment rollback rehearsal, optional Railway CLI log capture with CI redaction/signature/identity/rollback coverage, Railway smoke test, and Railway incident runbook exist. Missing live deployed project proof and external production log-drain evidence. |
| Observability and audit | 100 | HTTP trace IDs, operator context packets, operator-local state partitions, live-control action logs, traced paper decisions, metrics, idempotency counters, replay counts, retention/redaction metadata, structured audit export, checksum-chained runtime events, local state snapshots, immune breaker packets, Hyperliquid reconciliation packets, live cockpit packets, live execution receipt hashes, dry-run live certification packets, hash-only signed live evidence bundles, live canary policy packets, live canary bundle verification, live cockpit drill replay verification, live cockpit drill tamper rehearsal, public-safe exchange evidence attachment, public-safe operator reports, recursive operator evidence checksums, operator report verification, model gateway health packets, model gateway audit bundles, deployment evidence manifests, and required incident artifacts are documented. Remaining production work is external log drains/signing, not public-runtime audit shape. |
| Security and custody | 92 | No secrets needed for first run; Hyperliquid private keys have operator-scoped keychain/env helpers, redaction tests, a non-secret preflight gate, optional SDK-backed live adapter, threat model, secret-leak runbook, dependency policy, SBOM/provenance metadata, and release provenance policy. Missing external security review. |
| ZERO Network | 70 | Public-safe local profile packets, proof hashes, deployment claim hashes, deployment heartbeat hashes, verification badges, leaderboard rows, opt-in local publish logs, hosted-compatible ingestion, proof validation, duplicate refusal, metric-consistency checks, and accepted-only leaderboard output exist. Missing production hosted service persistence, public hosted pages, stale-publication windows, sybil policy, and signed identity verification. |
| ZERO Intelligence | 78 | Delayed public snapshots, catalog, billing-ready commercial contract, hosted-compatible `/v1/intelligence/*` reads/writes, token-gated paid scopes, actual rate-limit headers, usage events, HMAC-SHA256 webhook signature fixtures, aggregate export jobs, plan/scope model, dataset names, fail-closed model gateway status, model gateway health probes, model gateway audit bundles, mock/local provider conformance, real external model adapters, bounded retry/cost policy, hosted key-management rules, plan boundary, and opt-in local export packets exist. Missing production hosted persistence, billing provider integration, warehouse-backed realtime/history feeds, production webhook delivery, commercial terms, live hosted key-management implementation, and hosted audit retention. |
| Release and distribution | 98 | GitHub release artifacts, checksums, SBOM/provenance bundle, recorded `v0.1.1` clean-download release evidence, published-release evidence command, release verifier, tamper-detection rehearsal, draft-release rollback rehearsal, Homebrew formula renderer, attestations, installer, registry-readiness gate, package dry-run, distribution readiness policy, release template hardening checks, dependency policy, and rollback rules exist. Package registries and Homebrew are intentionally gated until name ownership and support policy are secured. |
| Documentation for operators | 100 | Good local docs, operator isolation docs, Hyperliquid read-only boundary docs, live-paper quote docs, immune-system docs, live cockpit docs, live cockpit drill bundle, verifier, and tamper rehearsal, live certification docs, live evidence docs, live canary policy/operator docs, Railway paper deploy, remote-doctor, and evidence-pack docs, restart recovery docs, audit/metrics docs, live-preflight warnings, threat model, and incident runbooks. Missing real exchange drill evidence only as external proof, not documented workflow. |

**Public repo readiness: 100/100.**

The production-parity contract is `zero.runtime.production_parity.v1`.

This is credible for a public open-source launch repository. Clean release
artifacts, fresh source-tree rehearsal, contribution paths, public gates, and
product boundary docs are now in place. The remaining external proof belongs to
the full operating-system/product score: external security review, human
fresh-clone feedback from at least one serious engineer, and a real
operator-owned live canary evidence bundle.

**Full ZERO operating-system readiness: 100/100.**

It is still not a hosted custody product, and real capital operation remains
self-custodial and operator-owned. The public repo must not imply that a new
operator can run unattended live capital safely without canary evidence,
external review, and human-reviewed protected live-code evolution rules
documented in [Private Engine Capability Gap Audit](private-engine-capability-gap-audit.md).
In plain terms, ZERO is not yet a complete autonomous capital terminal until an
operator-owned accepted live canary evidence bundle and external safety review
exist outside this repository.

For the public repo target, ZERO now has the executable contracts expected from
a complete autonomous operating-system launch artifact: paper runtime,
production-parity OODA proof, live-readiness gates, live-shadow fail-closed
evidence, local evolution gates, release evidence, and agentic contribution
surfaces. Hosted Network persistence, paid Intelligence infrastructure,
package-registry publication, third-party security review, and operator-owned
accepted canary proof remain external product and launch work, not missing
public-runtime contracts.

## CLI Readiness Detail

| Area | Score | Notes |
|---|---:|---|
| Command surface | 88 | `zero`, `zero init`, `zero doctor`, `zero run`, TUI, and slash-command dispatch are well covered. |
| Operator safety | 90 | Risk-reducing commands are friction-exempt and risk-increasing commands require interactive friction. |
| Engine integration | 97 | HTTP, WebSocket, mock engine, contract tests, Rust client decoding for production-parity OODA reports, live receipt packets, and live canary policy packets, `/runtime-parity`, `/live-receipts`, and `/live-canary` operator rendering, and live risk-reducer endpoints exist. Real accepted canary evidence remains external. |
| Install path | 94 | Release installer exists with checksum and attestation verification, and `v0.1.1` was installed from the public GitHub Release into a temporary bin directory. Homebrew formula rendering exists from release checksums, while Homebrew/package registries remain blocked until ownership is secured. |
| Diagnostics | 99 | Doctor, JSON output, exit codes, rate-budget checks, operator/credential partition checks, live-preflight diagnostics, live-cockpit next-action/operator rendering, Railway remote doctor, deployment evidence verification, deployment identity verification, deployment evidence log capture/signing, rollback rehearsal checks, paid-scope fail-closed checks, and live-control refusals are strong. Missing external production examples against a real linked Railway project. |
| TUI production UX | 94 | Snapshot coverage, status honesty, risk overlays, live-stream pane, and a full-screen live cockpit are strong. Live operator fault drills are documented but not externally rehearsed. |
| Non-interactive automation | 93 | `zero run` covers cockpit, receipts, canary policy, runtime parity, breaker, certification, account truth, and risk-reducer workflows while intentionally gating risk-increasing commands. Needs external production examples. |
| Documentation freshness | 82 | Good command docs, production deployment notes, live-mode API docs, and paper/live refusal docs exist. Incident docs remain thin. |

**CLI readiness: 100/100 as a public-runtime terminal.**

The CLI is first-class for the public runtime and operator workflows in this
repo. The remaining accepted-live canary evidence is operator-owned external
proof, not missing terminal shape.

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
- extract local memory from outcomes, generate genesis proposals, run paper
  canaries, calibrate changes, and promote or roll back with reviewable
  evidence;
- publish verified behavior to ZERO Network;
- consume delayed public intelligence for free;
- pay for realtime ZERO Intelligence API access when speed, scale, history, or
  commercial rights matter.

## Execution Cycles

Forecast after Cycle 37: **0 major public-repo cycles remain before the repo can
be treated as the complete ZERO autonomous operating-system launch artifact.**
The remaining work is external proof and hosted product operation: operator
canary evidence, third-party review, package registry ownership, hosted Network,
and paid Intelligence deployment.

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
- `zero-config` has local-only Hyperliquid private-key helpers for
  operator-scoped OS keychain slots and environment resolution, plus strict
  key/address validation and redaction.
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
- The interactive CLI now wires `/execute <coin> <buy|sell> <size>` to
  `POST /execute` after operator-state friction clears, with no automatic
  retry and engine-asserted `(paper)` / `(live)` rendering.
- Added `POST /live/heartbeat`, `/live/pause`, `/live/resume`, `/live/kill`,
  and `/live/flatten` plus CLI wiring for `/kill`, `/pause-entries`, and
  `/flatten-all`.
- Local and Railway smoke tests now assert public paper deployments refuse live
  execution and live controls with `live executor not configured`.
- Added `GET /live/evidence`, a hash-only `zero.live_evidence.v1` canary
  evidence bundle that captures preflight, cockpit, live execution receipts,
  reconciliation, immune, certification, audit, deployment claim, and
  deployment heartbeat artifacts without leaking credentials, raw decisions,
  trace tokens, or idempotency keys. `ZERO_LIVE_EVIDENCE_SIGNING_KEY` enables
  local HMAC-SHA256 signing.
- Added `scripts/live_canary_rehearsal.py`, which captures the full local
  rehearsal packet sequence in safe fail-closed mode and supports explicit
  operator-owned canary mode once all live gates are ready.
- Added `scripts/live_canary_verify.py`, which recomputes hashes, verifies
  packet completeness/status, compares manifest receipt/evidence fields, and
  fails on common redaction leaks before a canary bundle is shared.
- Added `scripts/live_canary_exchange_evidence.py`, which normalizes an
  operator-owned Hyperliquid order/fill export into public-safe evidence,
  hashes raw venue identifiers, matches exchange records to accepted ZERO
  receipts, and refreshes bundle checksums.
- Added `scripts/live_canary_operator.py`, which runs the public-safe refusal
  workflow end to end, finalizes real canary bundles after exchange export,
  writes `operator_report.json`, and fails closed when accepted live receipts
  lack exchange-side proof.
- Added `scripts/live_canary_operator_verify.py`, which independently verifies
  operator reports, recursive workflow checksums, privacy flags, redaction
  posture, accepted-live exchange-evidence rules, and the nested canary bundle.

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
- Added hosted-compatible `/v1/intelligence/snapshots`, `/history`, `/cohorts`,
  `/benchmarks`, `/webhooks`, and `/exports` with bearer-scope checks,
  `x-zero-ratelimit-*` headers, usage events, HMAC-SHA256 webhook signature
  fixtures, aggregate export jobs, OpenAPI coverage, local smoke, and Railway
  smoke coverage.

Exit gate:

- Hosted commercial users can pay for speed, history, scale, webhooks, exports,
  and redistribution while the runtime stays open.

### Cycle 11: Production Hardening

Target score: 100/100.

- Add chaos drills, exchange outage drills, restart drills, key-rotation drills,
  and incident runbooks.
- Add security review, threat model update, and signed release policy.
- Add Homebrew/package registry distribution once names, Trusted Publishing,
  owner lists, and rollback procedures are secured.

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
- Added `scripts/registry_readiness.py` and wired it into CI/release preflight
  to enforce PyPI metadata, Cargo registry metadata, optional live dependencies,
  and package-channel guardrails without publishing to external registries.
- Added dependency and supply-chain policy plus release SBOM/provenance
  generation so release bundles carry checksummed `SBOM.spdx.json` and
  `PROVENANCE.json` alongside GitHub artifact attestations.
- Added draft-release rollback rehearsal and Homebrew formula rendering from
  `SHA256SUMS`, keeping the tap publication step gated behind ownership proof.
- Backfilled the public `v0.1.1` GitHub Release with checksummed
  `SBOM.spdx.json` and `PROVENANCE.json`, verified executable attestations, and
  added `scripts/release_evidence.py` for clean-download release evidence.

Exit gate:

- The production scorecard reaches at least 95 in every dimension, and no live
  capital path is blocked by missing safety, audit, deployment, or recovery
  work.
