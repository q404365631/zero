# ZERO MCP Server

`zero-mcp` is a dependency-free, stdio MCP-compatible server for coding agents
and operator tooling. It exposes only public-safe, read-only ZERO surfaces:
bundled strategies, deterministic paper results, paper position state, runtime
status, health, journal tail, rejection audit, local memory snapshots and stats,
genesis proposal classifications, evolve gate status, research command-chain
reports, the public decision stack, immune status, deterministic backtest
summary, hash-only evidence bundle, safety catalog, and the demo proof-pack
manifest. It also exposes contributor-facing strategy runner, strategy plugin,
and market-data adapter docs as markdown resources so coding agents can find
the right extension path without scanning the full repository.

It does not expose live execution, order placement, approval, wallet, secret, or
venue-write tools. The server is for inspection and local development until live
operator capabilities have passed the public readiness gates.

## Run

From a source checkout, the server reads the checked-in scenario and proof-pack
artifacts:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.mcp --smoke
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.mcp
```

After installing the engine package, the same command works with an embedded
public demo fallback. Use a source checkout when you need file-hash verification
against `docs/proof/demo`.

```bash
zero-mcp --smoke
zero-mcp
```

The server reads newline-delimited JSON-RPC messages from stdin and writes
newline-delimited JSON-RPC responses to stdout.

## Transcript

The public transcript fixture is generated from the same handlers used by
`zero-mcp`:

```bash
PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py
PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py --check
```

See [`docs/mcp/transcript.jsonl`](mcp/transcript.jsonl). It proves that a
coding agent can initialize the server, list tools, inspect bundled strategies,
replay deterministic paper results, inspect redacted local memory and aggregate
stats, inspect plan-only genesis proposals, inspect paper-only evolve gates,
inspect paper-only research reports, inspect the lens/layer/modifier decision
stack, inspect production-parity OODA reports, inspect
journal/rejection/immune/evidence/backtest surfaces, list resources, and read
the proof pack and contributor docs without gaining any live execution
capability.

## Tools

| Tool | Safety class | Purpose |
| --- | --- | --- |
| `zero_list_strategies` | `read_only_public` | Lists bundled paper strategies and contributor examples. |
| `zero_get_runtime_status` | `read_only_public` | Returns paper runtime status derived from the bundled scenario. |
| `zero_get_runtime_parity` | `read_only_public` | Returns production-parity OODA evidence with disabled live shadow execution. |
| `zero_get_health` | `read_only_public` | Returns paper runtime health, dependencies, and breaker status. |
| `zero_get_paper_results` | `read_only_public` | Replays the deterministic bundled paper scenario. |
| `zero_get_position_state` | `read_only_public` | Returns paper position state derived from the scenario. |
| `zero_get_journal_tail` | `read_only_public` | Returns the paper decision journal tail from the bundled scenario. |
| `zero_get_rejection_audit` | `read_only_public` | Returns rejection counts grouped by paper stage and reason. |
| `zero_get_proof_pack` | `read_only_public` | Returns the public-safe demo proof-pack manifest. |
| `zero_get_network_proof_pack` | `read_only_public` | Returns the public-safe ZERO Network proof-chain manifest. |
| `zero_get_memory_snapshot` | `read_only_public` | Returns public-safe memory extracted from bundled paper decisions. |
| `zero_get_memory_stats` | `read_only_public` | Returns aggregate memory stats without entry bodies. |
| `zero_get_genesis_proposals` | `read_only_public` | Returns plan-only genesis proposal classifications. |
| `zero_get_evolve_status` | `read_only_public` | Returns paper-first builder, red-team, canary, calibration, promotion-plan, and rollback evidence. |
| `zero_get_research_report` | `read_only_public` | Returns paper-only hunt/edge/convergence/thesis/score/meta/sharpen reports. |
| `zero_get_decision_stack` | `read_only_public` | Returns the public paper-only lens/layer/modifier decision stack. |
| `zero_get_immune_status` | `read_only_public` | Returns paper immune breaker and risk-allowance status. |
| `zero_get_backtest_report` | `read_only_public` | Returns a deterministic paper backtest summary without PnL claims. |
| `zero_get_evidence_bundle` | `read_only_public` | Returns a hash-only public-safe evidence bundle. |
| `zero_get_safety_catalog` | `read_only_public` | Returns the MCP safety classification for every public tool. |

All tools declare `canPlaceOrders=false`, `canChangeRuntimeState=false`, and
`canReadSecrets=false`. None can place, approve, cancel, or route live orders.

## Resources

| URI | Purpose |
| --- | --- |
| `zero://paper/scenario` | Bundled deterministic paper scenario. |
| `zero://paper/results` | Generated paper replay result. |
| `zero://runtime/status` | Paper runtime status. |
| `zero://runtime/health` | Paper runtime health, dependencies, and breakers. |
| `zero://runtime/parity` | Production-parity OODA report with live shadow fail-closed evidence. |
| `zero://journal/tail` | Paper decision journal tail. |
| `zero://rejections/audit` | Paper rejection audit grouped by stage and reason. |
| `zero://proof/demo` | Demo proof-pack manifest. |
| `zero://proof/network` | ZERO Network proof-chain manifest. |
| `zero://memory/snapshot` | Public-safe local memory extracted from bundled paper decisions. |
| `zero://memory/stats` | Aggregate memory stats without entry bodies. |
| `zero://genesis/proposals` | Plan-only genesis proposal classifications. |
| `zero://evolve/status` | Paper-only evolve gate status. |
| `zero://research/report` | Paper-only research command-chain report. |
| `zero://decision/stack` | Public paper-only lens/layer/modifier decision stack. |
| `zero://immune/status` | Paper immune breaker status. |
| `zero://backtest/report` | Deterministic paper backtest report without PnL claims. |
| `zero://evidence/bundle` | Hash-only public-safe evidence bundle. |
| `zero://mcp/safety` | Safety classification for every public MCP tool. |
| `zero://docs/strategy-runner` | Markdown docs for declarative paper strategy runners. |
| `zero://docs/strategy-plugin` | Markdown docs for deterministic paper strategy plugins. |
| `zero://docs/market-data-adapters` | Markdown docs for deterministic market-data adapters. |

## Smoke Contract

`zero-mcp --smoke` verifies that:

- The exposed tool set contains no live execution tools.
- Every tool declares `read_only_public` safety metadata and cannot place
  orders, change runtime state, or read secrets.
- Every tool returns a JSON object.
- Memory output does not expose prices, wallet material, exchange order ids, or
  keys.
- Genesis output never applies code changes and protected path proposals are
  escalated for human review.
- Evolve output never pushes or promotes; human approval is still required.
- Research output never claims live PnL, mutates the checkout, or pushes.
- Decision-stack output never grants live execution authority.
- The demo and Network proof packs do not claim live trading or paper/live correlation.
- The public source checkout contains the bundled proof and paper artifacts.

The smoke command is part of the public readiness gates so this agent surface
cannot silently drift into a write-capable trading interface.
