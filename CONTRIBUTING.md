# Contributing to ZERO

Thanks for helping improve ZERO.

## Start Here

1. Read the README.
2. Run the paper-mode demo.
3. Pick an issue labeled `good first issue` or `help wanted`.
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

