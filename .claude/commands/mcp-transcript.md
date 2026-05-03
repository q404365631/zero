# MCP Transcript

Use this recipe when an agent changes `zero-mcp`, MCP docs, tool/resource
surface area, or agent entrypoints.

## Context

`zero-mcp` is read-only. It must not expose live execution, order placement,
approval, cancellation, wallet, credential, or venue-write tools.

## Steps

1. Read `docs/mcp.md`, `engine/src/zero_engine/mcp.py`, and
   `engine/tests/test_mcp.py`.
2. Run:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.mcp --smoke
cd engine && PYTHONPATH="$PWD/src" pytest tests/test_mcp.py
```

3. Regenerate and verify the transcript:

```bash
PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py
PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py --check
```

4. Run:

```bash
just docs-check
scripts/hardening_gate.sh
```

## Handoff

List the exposed MCP tools and resources. Call out any new tool by name and
explain why it remains read-only.
