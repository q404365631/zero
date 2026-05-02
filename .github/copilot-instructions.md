# ZERO Copilot Instructions

ZERO is an autonomous operating system for self-custodial onchain operations.
Keep the public runtime paper-first, local-first, and useful without hosted ZERO
infrastructure.

When suggesting code:

- preserve the open-core boundary in `docs/open-core-boundary.md`;
- preserve the safety model in `docs/safety-model.md`;
- use `docs/autonomous-os-plan.md` for roadmap context;
- prefer small changes with tests;
- keep public examples deterministic and secret-free;
- never add sample credentials or live trading shortcuts;
- make live-capable behavior fail closed with explicit refusal reasons;
- update docs and `justfile` gates when adding new examples or required docs.

Useful checks:

```bash
just docs-check
cd engine && PYTHONPATH="$PWD/src" pytest
cd cli && cargo test --workspace
just ci
```
