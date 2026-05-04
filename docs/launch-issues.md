# Launch Issues

Create these issues before opening the repository. They are intentionally small
and scoped so a new contributor can land a useful first PR without private
context.

Validate the seed issues locally:

```bash
just launch-issue-config-check
```

Maintainers can create missing GitHub issues after labels are synced:

```bash
just github-label-sync
just github-launch-issue-sync
```

The sync only creates missing issues with exact matching titles. It does not
edit existing issues, reopen closed issues, or delete anything.

## Completed Maintainer Tasks

These tasks are intentionally not part of the launch issue seed anymore because
the public `v0.1.1` release already exists and has clean-download evidence:

- First public release verification:
  [docs/releases/v0.1.1-evidence.md](releases/v0.1.1-evidence.md)
- First release candidate:
  [docs/releases/v0.1.1.md](releases/v0.1.1.md)

## Completed Contributor Tasks

These tasks are no longer part of the launch issue seed:

- ZERO Network stale publication window docs:
  [docs/network-freshness.md](network-freshness.md)
- Paper example output summary:
  [examples/paper-trading/README.md](../examples/paper-trading/README.md)
- Docker daemon troubleshooting note:
  [docs/local-development.md](local-development.md)
- Homebrew formula and public repo tap:
  [Formula/zero.rb](../Formula/zero.rb)
