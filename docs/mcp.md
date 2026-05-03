# ZERO MCP Server

`zero-mcp` is a dependency-free, stdio MCP-compatible server for coding agents
and operator tooling. It exposes only public-safe, read-only ZERO surfaces:
bundled strategies, deterministic paper results, paper position state, local
memory snapshots, genesis proposal classifications, evolve gate status, and
the demo proof-pack manifest.

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
replay deterministic paper results, inspect redacted local memory, inspect
plan-only genesis proposals, inspect paper-only evolve gates, list resources,
and read the proof pack without gaining any live execution
capability.

## Tools

| Tool | Purpose |
| --- | --- |
| `zero_list_strategies` | Lists bundled paper strategies and contributor examples. |
| `zero_get_paper_results` | Replays the deterministic bundled paper scenario. |
| `zero_get_position_state` | Returns paper position state derived from the scenario. |
| `zero_get_proof_pack` | Returns the public-safe demo proof-pack manifest. |
| `zero_get_memory_snapshot` | Returns public-safe memory extracted from bundled paper decisions. |
| `zero_get_genesis_proposals` | Returns plan-only genesis proposal classifications. |
| `zero_get_evolve_status` | Returns paper-only builder/red-team/canary/calibration gate status. |

All tools are read-only. None can place, approve, cancel, or route live orders.

## Resources

| URI | Purpose |
| --- | --- |
| `zero://paper/scenario` | Bundled deterministic paper scenario. |
| `zero://paper/results` | Generated paper replay result. |
| `zero://proof/demo` | Demo proof-pack manifest. |
| `zero://memory/snapshot` | Public-safe local memory extracted from bundled paper decisions. |
| `zero://genesis/proposals` | Plan-only genesis proposal classifications. |
| `zero://evolve/status` | Paper-only evolve gate status. |

## Smoke Contract

`zero-mcp --smoke` verifies that:

- The exposed tool set contains no live execution tools.
- Every tool returns a JSON object.
- Memory output does not expose prices, wallet material, exchange order ids, or
  keys.
- Genesis output never applies code changes and protected path proposals are
  escalated for human review.
- Evolve output never pushes or promotes; human approval is still required.
- The demo proof pack does not claim live trading or paper/live correlation.
- The public source checkout contains the bundled proof and paper artifacts.

The smoke command is part of the public readiness gates so this agent surface
cannot silently drift into a write-capable trading interface.
