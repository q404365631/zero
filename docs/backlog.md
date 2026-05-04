# Launch Backlog

This is the public launch backlog seed. Use [launch issues](launch-issues.md)
for ready-to-create issue bodies before opening the repository.

Maintainers should validate and create the launch issue seed with:

```bash
just launch-issue-config-check
just github-launch-issue-sync
```

The GitHub sync is title-idempotent: it creates missing seed issues and leaves
existing issues untouched.

## Good First Issues

### Add a paper-first strategy plugin

Labels: `good first issue`, `strategy`, `examples`

Add a deterministic strategy plugin under `examples/strategy-plugin/` or a new
example directory that follows [strategy plugin docs](strategy-plugins.md).

Acceptance:

- The plugin declares `StrategyPluginMetadata` with `paper_only=true`.
- The plugin returns `StrategySignal` or `None`; it never submits orders
  directly.
- The example runs without network access or exchange credentials.
- Tests prove accepted and rejected paths still go through `PaperEngine.submit`.

### Add a deterministic market data adapter

Labels: `good first issue`, `market-data`, `examples`

Add a market data adapter example that follows
[market data adapter docs](market-data-adapters.md).

Acceptance:

- The adapter declares `MarketDataAdapterMetadata`.
- The adapter returns chronological `Candle` objects and implements `latest`.
- The example requires no secrets, network access, or live exchange account.
- Tests cover missing symbols, positive limits, and paper strategy integration.

### Add paper example output summary

Labels: `good first issue`, `docs`, `examples`

Add a short "Expected output" section to `examples/paper-trading/README.md`.
Use abbreviated JSON, not a long transcript.

Acceptance:

- The README explains fills, rejections, and reduce-only behavior.
- The example command remains `just example`.
- The output summary stays deterministic and paper-only.

### Add Docker daemon troubleshooting note

Labels: `good first issue`, `docs`, `containers`

Add a short troubleshooting note to `docs/local-development.md` for
`just container-smoke` when Docker is installed but the daemon is not running.

Acceptance:

- The note explains the daemon requirement without assuming Docker Desktop.
- The note keeps the container path paper-only.
- The note does not weaken CI expectations.

## Help Wanted

### Add Homebrew tap

Labels: `help wanted`, `release`, `cli`, `packaging`

Add a Homebrew tap or formula after public release artifact names stabilize.

Acceptance:

- `README.md` links the Homebrew install command.
- The formula installs the checksummed GitHub Release binary or builds from source.
- `docs/release.md` names Homebrew as a supported distribution path.
- The path works without private package registry access.

## Maintainer Tasks

### Completed: first public release

The first public release is complete as `v0.1.1`.

Evidence:

- [Release notes](releases/v0.1.1.md)
- [Clean-download release evidence](releases/v0.1.1-evidence.md)

Do not create new first-release tasks unless a future release target changes
artifact requirements or public safety claims.
