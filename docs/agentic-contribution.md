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
- no private repo language, private infrastructure names, or credentials were
  introduced.

## Design Review Checklist

For design agent work:

- the first screen should be a usable product surface or clear repo surface;
- operational state should be visible;
- risk-reducing actions should be visually and mechanically easier than
  risk-increasing actions;
- generated public pages should stay aggregate-only;
- no text should imply hosted custody, guaranteed profit, or black-box trading.
