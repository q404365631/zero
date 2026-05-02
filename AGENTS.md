# Agent Guide

This repository is intended to be easy for engineers to work on with coding
agents and design agents. Agents should optimize for correctness, safety, and
small reviewable changes.

## First Read

Before editing, read:

- `README.md`
- `docs/architecture.md`
- `docs/open-core-boundary.md`
- `docs/safety-model.md`
- `docs/autonomous-os-plan.md`
- the nearest docs for the files you are changing

## Product Boundary

ZERO is an autonomous operating system for self-custodial onchain operations.

Open runtime work belongs in this repo:

- paper runtime
- local API
- operator terminal
- safety gates
- self-custodial venue adapters
- local journals and audit exports
- strategy and market-data extension contracts
- public Network contracts and delayed public Intelligence snapshots

Commercial product work should stay behind contracts and docs unless explicitly
asked:

- realtime Intelligence API
- history, cohorts, webhooks, exports, redistribution rights, support, SLAs
- hosted ingestion and commercial reliability infrastructure

Do not make the open runtime depend on a ZERO-hosted control plane.

## Safety Rules

- Default to paper mode.
- Public examples must not need real funds or secrets.
- Never add sample private keys, wallet secrets, API tokens, cookies, or
  exchange credentials.
- Live-capable changes need refusal paths, tests, docs, and CLI/operator
  visibility.
- Risk-reducing actions should stay easy; risk-increasing actions should keep
  deliberate friction.
- Public Network and Intelligence outputs must not leak wallets, private keys,
  exchange order IDs, raw journals, strategy labels, private notes, or
  per-trade symbols.

## Engineering Workflow

Use the existing stack:

- Python engine in `engine/src/zero_engine`
- Rust CLI in `cli/crates`
- deterministic fixtures in `contracts` and `examples`
- docs in `docs`
- automation in `justfile` and `.github/workflows`

Preferred checks:

```bash
just docs-check
cd engine && PYTHONPATH="$PWD/src" pytest
cd cli && cargo test --workspace
just ci
```

Run the smallest relevant check while iterating, then `just ci` before final
handoff when feasible.

## Design Agent Rules

Design contributions should make operational surfaces clearer, not more
decorative.

- Prioritize scanability, fault visibility, and fast risk reduction.
- Do not hide dangerous state behind optimistic copy.
- Use restrained UI patterns for terminal, dashboard, docs, and generated
  public pages.
- Keep public profile and leaderboard pages aggregate-only and redacted.
- Avoid marketing-only screens when a usable operator surface is possible.

## Current Strategic Path

The controlling plan is `docs/autonomous-os-plan.md`.

The next implementation priorities are:

1. autonomous runtime OODA loop;
2. strategy runner SDK;
3. durable runtime bus;
4. Hyperliquid account reconciliation;
5. live execution certification harness;
6. immune system and circuit breakers;
7. operator terminal live cockpit.

Each cycle should update tests, docs, and scorecards with the behavior it
actually lands.
