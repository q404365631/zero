# Public Upgrade Plan

This document is the launch control plan for turning the public repository into
the main ZERO product surface.

ZERO is an autonomous operating system for self-custodial onchain operations.
The public repo must prove that category through runnable software, not
marketing copy. The first public impression should be:

- a serious engineer can run paper mode in minutes;
- the engine and terminal are real, testable, and inspectable;
- live-capable behavior fails closed until custody, risk, reconciliation, and
  emergency controls are coherent;
- the open-core boundary is obvious;
- agentic contributors can find safe, scoped work immediately.

## Non-Negotiables

- Do not publish this private monorepo wholesale.
- Keep the public repo built from a positive allowlist.
- Keep paper mode as the default first-run path.
- Keep public examples secret-free and deterministic.
- Keep live execution self-custodial and explicit.
- Keep ZERO Intelligence as the commercial data product created by verified
  autonomous behavior.
- Keep local memory, research, genesis, evolve, guardian review, paper canary,
  and calibration loops open because they are part of the self-custodial runtime.
  Local memory is already open in [Memory Core](memory-core.md), and genesis
  proposal classification is open in [Genesis](genesis.md). Paper-only research
  is open in [Research Command Chain](research.md). Paper-only evolve gates are
  open in [Evolve Harness](evolve.md).
- Do not imply guaranteed returns, hosted custody, or unattended live safety.

## Public Launch Surface

| Surface | Public launch requirement |
| --- | --- |
| Runtime | Paper engine, local API, durable journal, runtime bus, strategy runners, safety gates, read-only Hyperliquid data, and live-readiness contracts. |
| Terminal | Doctor, status, risk, replay, cockpit/readiness views, and friction-preserving risk controls. |
| Evolution | Local memory, research command chain, genesis proposal core, and paper-only evolve gates now exist; real mutation, promotion, and rollback remain. |
| Network | Redacted local proof packets, profile contracts, leaderboard contracts, and static page examples. |
| Intelligence | Delayed public snapshots, catalog contracts, commercial API contracts, rate-limit and webhook fixtures, and clear subscription boundary. |
| Contribution | Agent guide, scoped backlog, issue forms, PR template, safety review path, and one-command gates. |

## Upgrade Cycles

### Cycle A: Public Truth Pass

Goal: make the public repo impossible to misunderstand.

- Root README states the category, current runnable surface, safety defaults,
  open-core boundary, and current live-capital limitations.
- `docs/production-readiness.md` separates public launch readiness from full
  autonomous-capital readiness.
- `docs/open-core-boundary.md` defines what stays open and what becomes ZERO
  Intelligence.
- `scripts/public_readiness_gate.sh` rejects cache artifacts, private markers,
  private infrastructure, and generated binary/runtime files.

Exit gate:

```bash
just public-readiness
just docs-check
```

### Cycle B: Real Engine Publicization

Goal: move the real engine capabilities into the public runtime without private
state, private deployment topology, or commercial data.

Publicize:

- OODA loop controller interfaces;
- strategy/lens runner interfaces;
- risk and immune gates;
- live-readiness and reconciliation contracts;
- hash-only signed live evidence bundles;
- local journals and runtime bus;
- Hyperliquid venue adapter interfaces;
- paper/live separation tests;
- operator-safe CLI workflows, including friction-gated engine-backed
  `/execute <coin> <buy|sell> <size>`.

Keep private:

- private operator state;
- production wallets, addresses, keys, journals, and fills;
- private deployment topology;
- proprietary datasets, calibrations, and hosted scoring feeds;
- commercial fleet management, billing, and customer operations.

Exit gate:

```bash
just ci
just public-readiness
```

Current progress: `/live/evidence` now packages preflight, cockpit, live
execution receipts, reconciliation, immune, certification, audit, deployment
claim, and deployment heartbeat hashes into `zero.live_evidence.v1`, with
optional local HMAC-SHA256 signing via `ZERO_LIVE_EVIDENCE_SIGNING_KEY`.

### Cycle C: Launch Proof

Goal: produce the evidence a serious engineer expects before starring,
installing, or contributing.

- Fresh-clone demo transcript.
- Fresh source-tree rehearsal in CI, proving the public gates and paper API
  work outside the maintainer checkout.
- Published `v0.1.1` clean-download release evidence with checksums,
  executable attestations, release verifier output, and Homebrew formula
  rendering.
- Railway paper deployment evidence.
- Release rehearsal evidence.
- OpenSSF Scorecard enabled.
- CodeQL, secret scan, Dependabot, and release attestations green.
- At least five scoped `good first issue` candidates.
- At least three `help wanted` issues for serious contributors.

Exit gate:

```bash
just release-rehearsal
just draft-release-rehearsal
just release-evidence v0.1.1
just fresh-clone-rehearsal
just public-readiness
```

### Cycle D: Self-Evolution Publicization

Goal: expose the adaptive loop that makes ZERO a complete autonomous operating
system, without publishing private operator data or commercial intelligence.

Publicize:

- memory taxonomy and append-only knowledge extraction, now implemented as the
  first open component;
- genesis proposal schema, journal, and guardian policy;
- research reports for hunt, edge, convergence, thesis, score, meta, and sharpen;
- builder, red-team, paper canary, calibration, and local promotion gates;
- read-only and risk-reducing MCP/API surfaces for memory, research, and genesis.

Keep private:

- private journals, production trades, production wallets, private deployment
  details, proprietary datasets, and hosted aggregate intelligence.

Exit gate:

```bash
just ci
just public-readiness
```

## Launch Score

Use two scores, always:

- **Public repo readiness:** how ready this repository is for engineers and
  contributors.
- **Full ZERO operating-system readiness:** how close the public runtime is to
  the complete autonomous live-capital system.

Do not collapse those into one number. A repo can be excellent for public
contribution while the full live-capital system still needs canary evidence,
external review, and hosted infrastructure.
