# Launch Backlog

This is the public launch backlog index. Use
[Contributor Issue Board](contributor-issue-board.md) for live public issues
and [launch issues](launch-issues.md) for the source-controlled seed bodies.

Maintainers should validate and create the launch issue seed with:

```bash
just launch-issue-config-check
just github-launch-issue-sync
```

The GitHub sync is title-idempotent: it creates missing seed issues and leaves
existing issues untouched.

## Active Launch Issue Seed

The active launch issue seed now lives in
[docs/launch-issues.md](launch-issues.md). It keeps the completed first wave
for auditability and opens a second wave with three scoped good-first issues
and two help-wanted issues for agentic and human contributors.

The corresponding live issues are listed in
[docs/contributor-issue-board.md](contributor-issue-board.md).

Completed seed issues move to the completed section on the board and keep their
source-controlled acceptance criteria in [launch issues](launch-issues.md).

## Example Contribution Shapes

### Add a paper-first strategy plugin

Labels: `good first issue`, `strategy`, `examples`

Status: delivered in
[Paper Momentum Strategy Plugin](../examples/momentum-strategy-plugin/README.md).

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

## Maintainer Tasks

### Completed: first public release

The first public release is complete as `v0.1.1`; the current published
release is `v0.1.2`.

Evidence:

- [Release notes](releases/v0.1.1.md)
- [Clean-download release evidence](releases/v0.1.1-evidence.md)
- [Current v0.1.2 release notes](releases/v0.1.2.md)
- [Current v0.1.2 clean-download evidence](releases/v0.1.2-evidence.md)
- [Release verification guide](release-verification.md)
- [CLI doctor troubleshooting guide](cli-doctor-troubleshooting.md)
- [Read-only MCP contributor docs resources](mcp.md)
- [Deterministic market-data adapter fixture](../examples/market-data-adapter/README.md)
- [Proof-pack privacy regression fixtures](proof/privacy-regression/README.md)
- [ZERO Network stale profile fixture](../examples/network-stale-profile/README.md)
- [Paper Momentum Strategy Plugin](../examples/momentum-strategy-plugin/README.md)

Do not create new first-release tasks unless a future release target changes
artifact requirements or public safety claims.

### Completed: Homebrew formula

The public repo now includes `Formula/zero.rb`, generated from the `v0.1.2`
GitHub Release checksum manifest. Operators can install it with:

```bash
brew tap zero-intel/zero https://github.com/zero-intel/zero
brew install zero
```
