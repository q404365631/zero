# ZERO Agent Commands

These are reusable task recipes for coding agents working in ZERO. They are
plain Markdown so any agent or engineer can use them; they do not grant extra
permissions and they must preserve the repository safety invariants in
`AGENTS.md`.

Use these when assigning small, reviewable agent work:

- [`paper-backtest.md`](paper-backtest.md) - replay deterministic paper fixtures.
- [`verify-schema.md`](verify-schema.md) - check public API and generated contracts.
- [`proof-pack.md`](proof-pack.md) - rebuild and verify public-safe proof artifacts.
- [`mcp-transcript.md`](mcp-transcript.md) - regenerate and validate agent MCP output.
- [`new-strategy.md`](new-strategy.md) - add a paper-first strategy example.

Every command recipe should end with the smallest relevant check, then `just ci`
before handoff when the change touches public contracts, packaging, or CI.
