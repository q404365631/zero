# Launch Issues

These issues are the source-controlled seed for the public contribution board.
They are intentionally small and scoped so a new contributor can land a useful
first PR without private context.

The current live board is [Contributor Issue Board](contributor-issue-board.md).

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

## Good First Issue: Add a paper-only momentum strategy plugin

Labels: `good first issue`, `good-first-strategy`, `strategy`, `examples`, `agent-eligible`

GitHub: [#18](https://github.com/zero-intel/zero/issues/18)

Add a deterministic paper-only strategy plugin under `examples/strategy-plugin/`
or a new sibling example. The plugin should be easy for a new engineer to read
and must never submit orders directly.

Acceptance:

- The plugin declares `StrategyPluginMetadata` with `paper_only=true`.
- The plugin returns `StrategySignal` or `None`; execution still flows through
  the paper engine.
- The example runs without network access, exchange credentials, or live mode.
- Tests or a smoke command cover one accepted signal and one rejected/no-signal
  path.

## Good First Issue: Add a deterministic market-data adapter fixture

Labels: `good first issue`, `market-data`, `examples`, `agent-eligible`

GitHub: [#19](https://github.com/zero-intel/zero/issues/19)

Add a market-data adapter example that reads a small local fixture and exposes
chronological candles through the public adapter interface.

Acceptance:

- The adapter declares `MarketDataAdapterMetadata`.
- The adapter returns deterministic `Candle` objects in chronological order.
- The example requires no secrets, network access, or exchange account.
- Tests cover unknown symbol, positive limit validation, and `latest`.

## Good First Issue: Add CLI doctor troubleshooting examples

Labels: `good first issue`, `cli`, `docs`, `agent-eligible`

GitHub: [#20](https://github.com/zero-intel/zero/issues/20)

Status: delivered in [CLI Doctor Troubleshooting](cli-doctor-troubleshooting.md).

Improve the public CLI docs with copy-paste troubleshooting examples for common
`zero doctor` warnings: missing API token, paper API not running, and live
preflight refusing closed.

Acceptance:

- The docs show exact commands and expected safe output snippets.
- The examples do not imply live trading is ready by default.
- The first-10-minutes path links to the new troubleshooting section.
- `just docs-check` passes.

## Good First Issue: Add a stale ZERO Network profile fixture

Labels: `good first issue`, `network`, `examples`, `agent-eligible`

GitHub: [#21](https://github.com/zero-intel/zero/issues/21)

Add a deterministic public-safe fixture that shows how a ZERO Network profile
looks when proof is valid but freshness has expired.

Acceptance:

- The fixture contains no wallet material, raw trades, private notes, or raw
  exchange order IDs.
- The generated page or JSON clearly separates proof validity from freshness.
- Existing Network smoke tests still pass.
- The docs explain that stale proof is archive evidence, not active operator
  status.

## Good First Issue: Add proof-pack privacy regression fixtures

Labels: `good first issue`, `proof-pack`, `security`, `agent-eligible`

GitHub: [#22](https://github.com/zero-intel/zero/issues/22)

Add negative fixtures for the proof-pack verifier that demonstrate refusal when
public proof artifacts contain private-looking fields.

Acceptance:

- The negative fixtures are synthetic and contain no real secrets.
- The verifier rejects at least one wallet-like field and one raw exchange ID
  field.
- The docs explain why proof packs are proof-of-process, not custody or PnL
  proof.
- `just public-proof` still passes for committed valid proof packs.

## Help Wanted: Expand read-only MCP strategy resources

Labels: `help wanted`, `mcp`, `docs`, `agent-eligible`

GitHub: [#23](https://github.com/zero-intel/zero/issues/23)

Status: delivered in [ZERO MCP Server](mcp.md).

Improve the read-only MCP resources so coding agents can discover strategy
runner docs, strategy plugin docs, and market-data adapter docs without reading
the entire repository.

Acceptance:

- New resources are read-only and cannot change runtime state.
- The MCP safety catalog still reports no risk-increasing tools.
- The transcript fixture is updated and deterministic.
- `PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py --check` passes.

## Help Wanted: Design public Network empty and stale states

Labels: `help wanted`, `network`, `design`, `design-review`

GitHub: [#24](https://github.com/zero-intel/zero/issues/24)

Improve the generated public ZERO Network pages for empty, stale, and active
states so viewers can understand what is verified, what is stale, and what is
not claimed.

Acceptance:

- Empty, stale, and active states are visually distinct.
- Copy never implies PnL, guaranteed returns, hosted custody, or live trading by
  default.
- The generated pages remain static, escaped, and public-safe.
- `scripts/network_pages_smoke.py` passes.

## Help Wanted: Add release evidence reader docs

Labels: `help wanted`, `release`, `docs`, `packaging`

GitHub: [#25](https://github.com/zero-intel/zero/issues/25)

Status: delivered in [Release Verification Guide](release-verification.md).

Add a short guide that explains how a user verifies a ZERO release from scratch:
checksums, GitHub artifact attestations, SBOM/provenance metadata, Homebrew
formula checks, and clean-download evidence.

Acceptance:

- The guide starts from a fresh clone or a downloaded release asset.
- It includes the exact `gh attestation verify` and checksum commands.
- It explains what the Homebrew formula drift check proves.
- It links to `docs/releases/v0.1.1-evidence.md` without claiming future
  releases have already been verified.

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
