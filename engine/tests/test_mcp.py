from __future__ import annotations

import json

from zero_engine import mcp


def test_initialize_exposes_read_only_capabilities() -> None:
    response = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"},
        }
    )

    assert response is not None
    assert response["result"]["protocolVersion"] == "2025-06-18"
    assert "tools" in response["result"]["capabilities"]
    assert "resources" in response["result"]["capabilities"]


def test_tools_are_read_only() -> None:
    tools = mcp.tool_definitions()
    names = [tool["name"] for tool in tools]

    assert names == [
        "zero_list_strategies",
        "zero_get_paper_results",
        "zero_get_position_state",
        "zero_get_proof_pack",
    ]
    assert not any("execute" in name or "live" in name or "order" in name for name in names)
    assert all("Read-only" in tool["description"] for tool in tools)


def test_tools_list_and_call_paper_results() -> None:
    listed = mcp.handle_request({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
    called = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "zero_get_paper_results", "arguments": {}},
        }
    )

    assert listed is not None
    assert len(listed["result"]["tools"]) == 4
    assert called is not None
    payload = json.loads(called["result"]["content"][0]["text"])
    assert payload["schema_version"] == "zero.mcp.paper_results.v1"
    assert payload["mode"] == "paper"
    assert payload["paper_only"] is True
    assert payload["fills"] >= 1


def test_resources_list_and_read() -> None:
    listed = mcp.handle_request({"jsonrpc": "2.0", "id": 4, "method": "resources/list"})
    read = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/read",
            "params": {"uri": "zero://proof/demo"},
        }
    )

    assert listed is not None
    assert {resource["uri"] for resource in listed["result"]["resources"]} == {
        "zero://paper/scenario",
        "zero://paper/results",
        "zero://proof/demo",
    }
    assert read is not None
    proof = json.loads(read["result"]["contents"][0]["text"])
    assert proof["claim_boundary"]["live_trading_claimed"] is False
    assert proof["live_correlation"]["status"] == "unavailable"


def test_unknown_tool_returns_json_rpc_error() -> None:
    response = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {"name": "zero_execute_live", "arguments": {}},
        }
    )

    assert response is not None
    assert response["error"]["code"] == -32602
    assert "unknown read-only ZERO tool" in response["error"]["message"]


def test_installed_package_fallback_stays_read_only(monkeypatch) -> None:
    monkeypatch.setattr(mcp, "find_repo_root", lambda: None)

    paper = mcp.get_paper_results()
    proof = mcp.get_proof_pack()
    scenario_text = mcp.read_resource("zero://paper/scenario")

    assert paper["mode"] == "paper"
    assert paper["fills"] == 2
    assert proof["claim_boundary"]["live_trading_claimed"] is False
    assert "paper-launch-smoke" in scenario_text
