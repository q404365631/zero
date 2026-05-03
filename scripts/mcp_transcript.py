#!/usr/bin/env python3
"""Generate a deterministic public MCP transcript."""

from __future__ import annotations

import argparse
import difflib
import json
from pathlib import Path
import sys
from typing import Any

from zero_engine import mcp

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs" / "mcp" / "transcript.jsonl"

REQUESTS: tuple[dict[str, Any], ...] = (
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "clientInfo": {"name": "zero-public-transcript", "version": "0.1.0"},
            "capabilities": {},
        },
    },
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "zero_list_strategies", "arguments": {}},
    },
    {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "zero_get_paper_results", "arguments": {}},
    },
    {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {"name": "zero_get_memory_snapshot", "arguments": {}},
    },
    {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {"name": "zero_get_genesis_proposals", "arguments": {}},
    },
    {
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": {"name": "zero_get_evolve_status", "arguments": {}},
    },
    {
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {"name": "zero_get_research_report", "arguments": {}},
    },
    {
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {"name": "zero_get_decision_stack", "arguments": {}},
    },
    {"jsonrpc": "2.0", "id": 12, "method": "resources/list", "params": {}},
    {
        "jsonrpc": "2.0",
        "id": 13,
        "method": "resources/read",
        "params": {"uri": "zero://decision/stack"},
    },
    {
        "jsonrpc": "2.0",
        "id": 14,
        "method": "resources/read",
        "params": {"uri": "zero://research/report"},
    },
    {
        "jsonrpc": "2.0",
        "id": 15,
        "method": "resources/read",
        "params": {"uri": "zero://proof/demo"},
    },
)


def as_json_line(payload: dict[str, Any]) -> str:
    return json.dumps(payload, sort_keys=True, separators=(",", ":"))


def render() -> str:
    entries: list[dict[str, Any]] = []
    for request in REQUESTS:
        response = mcp.handle_request(request)
        if response is None:
            raise RuntimeError(f"request produced no response: {request}")
        entries.append({"request": request, "response": response})

    validate(entries)
    return "".join(f"{as_json_line(entry)}\n" for entry in entries)


def validate(entries: list[dict[str, Any]]) -> None:
    tool_response = entries[1]["response"]
    tools = tool_response["result"]["tools"]
    names = [tool["name"] for tool in tools]
    forbidden = ("execute", "live", "order", "approve", "wallet")
    if any(marker in name for name in names for marker in forbidden):
        raise RuntimeError(f"transcript exposes a forbidden write-capable tool: {names}")

    paper_response = entries[3]["response"]
    paper_text = paper_response["result"]["content"][0]["text"]
    paper = json.loads(paper_text)
    if paper["mode"] != "paper" or paper["paper_only"] is not True:
        raise RuntimeError("transcript paper result must remain paper-only")

    memory_response = entries[4]["response"]
    memory_text = memory_response["result"]["content"][0]["text"]
    memory = json.loads(memory_text)
    if memory["paper_only"] is not True:
        raise RuntimeError("transcript memory snapshot must remain paper-only")
    serialized_memory = json.dumps(memory).lower()
    forbidden_memory = ("40500.0", "0x1234567890", "sk_live_")
    if any(marker in serialized_memory for marker in forbidden_memory):
        raise RuntimeError("transcript memory snapshot leaked private or derivable state")

    genesis_response = entries[5]["response"]
    genesis_text = genesis_response["result"]["content"][0]["text"]
    genesis = json.loads(genesis_text)
    if genesis["applies_code_changes"] is not False or genesis["paper_only"] is not True:
        raise RuntimeError("transcript genesis proposals must remain plan-only and paper-only")

    evolve_response = entries[6]["response"]
    evolve_text = evolve_response["result"]["content"][0]["text"]
    evolve = json.loads(evolve_text)
    if evolve["pushes_to_remote"] is not False or evolve["paper_only"] is not True:
        raise RuntimeError("transcript evolve status must remain paper-only and local-only")

    research_response = entries[7]["response"]
    research_text = research_response["result"]["content"][0]["text"]
    research = json.loads(research_text)
    if (
        research["pushes_to_remote"] is not False
        or research["paper_only"] is not True
        or research["claims_live_pnl"] is not False
    ):
        raise RuntimeError("transcript research report must remain paper-only and local-only")

    decision_response = entries[8]["response"]
    decision_text = decision_response["result"]["content"][0]["text"]
    decision = json.loads(decision_text)
    if (
        decision["paper_only"] is not True
        or decision["decision"]["allowed_to_execute_live"] is not False
    ):
        raise RuntimeError("transcript decision stack must remain paper-only and non-executing")

    decision_resource = entries[10]["response"]
    decision_resource_text = decision_resource["result"]["contents"][0]["text"]
    decision_resource_payload = json.loads(decision_resource_text)
    if decision_resource_payload["decision"]["allowed_to_execute_live"] is not False:
        raise RuntimeError("transcript decision resource must not grant live execution")

    research_resource = entries[11]["response"]
    research_resource_text = research_resource["result"]["contents"][0]["text"]
    research_resource_payload = json.loads(research_resource_text)
    if research_resource_payload["paper_only"] is not True:
        raise RuntimeError("transcript research resource must remain paper-only")

    proof_response = entries[12]["response"]
    proof_text = proof_response["result"]["contents"][0]["text"]
    proof = json.loads(proof_text)
    boundary = proof["claim_boundary"]
    if boundary["live_trading_claimed"] or boundary["paper_vs_live_correlation_claimed"]:
        raise RuntimeError("transcript proof pack must not claim live trading or paper/live correlation")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if transcript is stale")
    args = parser.parse_args()

    expected = render()
    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != expected:
            diff = difflib.unified_diff(
                current.splitlines(),
                expected.splitlines(),
                fromfile=str(OUTPUT),
                tofile="generated",
                lineterm="",
            )
            print("\n".join(list(diff)[:200]), file=sys.stderr)
            print("docs/mcp/transcript.jsonl is stale; run scripts/mcp_transcript.py", file=sys.stderr)
            return 1
        return 0

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(expected, encoding="utf-8")
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
