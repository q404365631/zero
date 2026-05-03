# Contributing to ZERO

Thanks for helping improve ZERO.

## Start Here

1. Read the README.
2. Run the paper-mode demo.
3. Pick an issue labeled `good first issue`, `good-first-strategy`,
   `agent-eligible`, or `help wanted`.
4. Open a small pull request with tests.

## Local Setup

```bash
just bootstrap
just test
just lint
```

The default contributor path must not require real trading credentials.

## Pull Requests

Every PR should include:

- clear problem statement
- implementation summary
- tests or a reason tests are not applicable
- safety impact if the change touches execution, risk, credentials, or operator commands

Large rewrites should start as an issue or proposal.

## Issue Lanes

Use the issue template that matches the work:

- `Agent task`: bounded work for coding, review, documentation, or design
  agents. Must include owner boundary, out-of-scope list, safety invariant,
  acceptance criteria, and checks.
- `Strategy example`: paper-only strategy, runner, plugin, market-data adapter,
  runtime loop, or proof-pack example. Must be deterministic and runnable
  without secrets.
- `Safety review`: execution, sizing, risk gates, kill switches, auth, secret
  handling, operator friction, live evidence, or public proof contracts.
- `Design review`: README, generated public pages, CLI/TUI copy, docs IA, or
  launch surfaces.
- `Documentation gap`: missing, stale, misleading, or agent-hostile docs.

Labels are defined in [docs/label-taxonomy.md](docs/label-taxonomy.md). If a
new issue lane needs a new label, update `.github/labels.yml` and the taxonomy
checker in the same pull request. Maintainers can apply the configured labels
to GitHub with `just github-label-sync`.

## Safety-Critical Changes

Changes to sizing, order execution, risk gates, kill switches, authentication, secret handling, or operator friction require:

- focused regression tests
- explicit paper-mode validation
- maintainer review
- documentation update when behavior changes

## Commit Style

Prefer clear conventional-style prefixes:

- `feat:`
- `fix:`
- `docs:`
- `test:`
- `refactor:`
- `chore:`

## License

By contributing, you agree that your contribution is licensed under Apache-2.0.
