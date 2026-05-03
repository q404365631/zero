# Verify Schema

Use this recipe when an agent changes public contracts, OpenAPI, JSON fixtures,
CLI/API response shapes, or generated docs.

## Context

Public contracts are load-bearing. Breaking changes need explicit compatibility
notes, updated tests, and release notes.

## Steps

1. Read `AGENTS.md`, `docs/api-compatibility.md`, and the contract file being
   changed.
2. Run contract checks:

```bash
python3 scripts/openapi_contract_check.py
PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py --check
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py --check
scripts/generate_llms_full.py --check
```

3. If output is stale, regenerate only the relevant artifact and inspect the
   diff.
4. Run:

```bash
just docs-check
scripts/hardening_gate.sh
```

## Handoff

List every changed schema or fixture and say whether it is backward compatible.
If it is not backward compatible, include the migration note and release-note
path.
