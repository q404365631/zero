# Agentic Contribution Guide

ZERO should be first-class for engineers using coding agents, design agents, or
review agents. This guide explains how to structure agent work so contributions
remain safe and useful.

## Good Agent Tasks

- Add focused tests around an existing behavior.
- Add a deterministic example under `examples/`.
- Extend a public contract fixture.
- Improve CLI diagnostics without changing risk semantics.
- Add docs that clarify operator safety, deployment, or contribution paths.
- Implement one item from `docs/autonomous-os-plan.md` with tests and docs.

## Issue Lanes

The public repo uses issue templates as agent routing contracts:

- `.github/ISSUE_TEMPLATE/agent_task.yml` for scoped agent work. Use label
  `agent-eligible`.
- `.github/ISSUE_TEMPLATE/strategy_example.yml` for deterministic paper-only
  strategy, runner, plugin, market-data adapter, runtime loop, or proof-pack
  examples. Use label `good-first-strategy`.
- `.github/ISSUE_TEMPLATE/safety_review.yml` for execution, risk, credentials,
  operator friction, live evidence, or proof privacy. Use label
  `safety-critical`.
- `.github/ISSUE_TEMPLATE/design_review.yml` for README, CLI/TUI copy, public
  proof pages, generated Network pages, and docs information architecture. Use
  label `design-review`.
- `.github/ISSUE_TEMPLATE/docs_gap.yml` for missing, stale, misleading, or
  agent-hostile docs. Use label `docs-gap`.

`scripts/issue_template_check.py` enforces the lane names, required labels, and
template markers. Update that checker in the same pull request as any template
or label change.

## Command Recipes

Reusable command recipes live in `.claude/commands/`. They are plain Markdown,
not hidden permissions. Use them as scoped prompts for any coding agent:

- `.claude/commands/paper-backtest.md`
- `.claude/commands/verify-schema.md`
- `.claude/commands/proof-pack.md`
- `.claude/commands/mcp-transcript.md`
- `.claude/commands/new-strategy.md`

If a command recipe is stale, update the recipe in the same pull request as the
behavior it describes.

## Bad Agent Tasks

- Add live execution shortcuts without preflight, kill, and refusal paths.
- Add hosted dependencies to paper mode.
- Add broad refactors across engine and CLI without a failing test.
- Add public data fields that could deanonymize an operator.
- Add generated marketing pages that do not improve operator understanding.

## Required Context Packet

When assigning an agent, include:

- the target cycle from `docs/autonomous-os-plan.md`;
- the exact files or module boundary it owns;
- the safety invariant it must preserve;
- the checks it should run;
- whether the work is engine, CLI, docs, Network, Intelligence, or design.
- the operator context to use when testing shared runtimes, usually
  `X-Zero-Operator-Handle: <agent-or-engineer-handle>`.
- the GitHub issue lane and label that matches the work.
- the CODEOWNERS surface that must review the change when touching protected
  paths.

Example:

```text
Implement Cycle 12 runtime-loop tests only. Own engine/tests/test_runtime.py.
Preserve paper-only behavior and no live execution. Run engine pytest and
just docs-check.
```

When an agent touches live-control surfaces, capture `/operator/context`,
`/deployment/claim`, `/live/cockpit`, and `/audit/export?limit=100` in the
review notes. The context packet is audit identity, not permission; agents must
not bypass CLI friction, preflight, immune breakers, or live execution policy.

## Review Checklist

Before merging agent-authored work, verify:

- the change is scoped and understandable;
- public examples run without secrets;
- live-capable code fails closed;
- docs match actual behavior;
- contracts stay deterministic;
- `just ci` passes or the remaining gap is documented;
- `just codeowners-check` passes when review boundaries changed;
- no private repo language, private infrastructure names, or credentials were
  introduced.

## Pull Request Disclosure

Agent-assisted pull requests should fill out `.github/PULL_REQUEST_TEMPLATE.md`
completely, including the AI assistance field and the safety impact field.
Maintainers should ask for a narrower diff when an agent PR changes unrelated
engine, CLI, docs, and release surfaces together.

## Design Review Checklist

For design agent work:

- the first screen should be a usable product surface or clear repo surface;
- operational state should be visible;
- risk-reducing actions should be visually and mechanically easier than
  risk-increasing actions;
- generated public pages should stay aggregate-only;
- no text should imply hosted custody, guaranteed profit, or black-box trading.
