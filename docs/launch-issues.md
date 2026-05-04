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

## Good First Issue: Add Docker Daemon Troubleshooting Note

Labels: `good first issue`, `docs`, `containers`

Add a short troubleshooting note to `docs/local-development.md` for
`just container-smoke` when Docker is installed but the daemon is not running.

Acceptance:

- The note explains the daemon requirement without assuming Docker Desktop.
- The note keeps the container path paper-only.
- The note does not weaken CI expectations.

## Help Wanted: Homebrew Tap

Labels: `help wanted`, `release`, `cli`, `packaging`

Add a Homebrew tap or formula after public release artifact names stabilize.

Acceptance:

- `README.md` links the Homebrew install command.
- The formula installs the checksummed GitHub Release binary or builds from source.
- `docs/release.md` names Homebrew as a supported distribution path.
- The path works without private package registry access.

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
