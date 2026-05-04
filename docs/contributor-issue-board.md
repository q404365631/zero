# Contributor Issue Board

These issues are the first public contribution lanes for ZERO. They are scoped
so a human contributor or coding agent can make progress without private
operator context, exchange credentials, or live trading.

Use the GitHub labels when choosing work:

- [`good first issue`](https://github.com/zero-intel/zero/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22good%20first%20issue%22)
- [`agent-eligible`](https://github.com/zero-intel/zero/issues?q=is%3Aissue%20is%3Aopen%20label%3Aagent-eligible)
- [`help wanted`](https://github.com/zero-intel/zero/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22help%20wanted%22)
- [`design-review`](https://github.com/zero-intel/zero/issues?q=is%3Aissue%20is%3Aopen%20label%3Adesign-review)

## Good First Issues

- [#18 Add a paper-only momentum strategy plugin](https://github.com/zero-intel/zero/issues/18)
- [#21 Add a stale ZERO Network profile fixture](https://github.com/zero-intel/zero/issues/21)
- [#22 Add proof-pack privacy regression fixtures](https://github.com/zero-intel/zero/issues/22)

## Help Wanted

- [#24 Design public Network empty and stale states](https://github.com/zero-intel/zero/issues/24)

## Completed Seed Issues

- [#19 Add a deterministic market-data adapter fixture](https://github.com/zero-intel/zero/issues/19) -
  delivered in [Market Data Adapter Example](../examples/market-data-adapter/README.md).
- [#23 Expand read-only MCP strategy resources](https://github.com/zero-intel/zero/issues/23) -
  delivered in [ZERO MCP Server](mcp.md).
- [#25 Add release evidence reader docs](https://github.com/zero-intel/zero/issues/25) -
  delivered in [Release Verification Guide](release-verification.md).
- [#20 Add CLI doctor troubleshooting examples](https://github.com/zero-intel/zero/issues/20) -
  delivered in [CLI Doctor Troubleshooting](cli-doctor-troubleshooting.md).

## Contribution Rules

- Keep the default path paper-first.
- Do not add exchange credentials, wallet material, raw fills, private journals,
  or production deployment details.
- Keep examples deterministic unless an issue explicitly asks for read-only
  live market data.
- Include a test, smoke command, or verification note with every PR.
- For agent-authored work, follow [AGENTS.md](../AGENTS.md) and
  [Agentic Contribution](agentic-contribution.md).
