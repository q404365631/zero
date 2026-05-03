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
        "zero_get_runtime_status",
        "zero_get_runtime_parity",
        "zero_get_health",
        "zero_get_paper_results",
        "zero_get_position_state",
        "zero_get_journal_tail",
        "zero_get_rejection_audit",
        "zero_get_proof_pack",
        "zero_get_memory_snapshot",
        "zero_get_memory_stats",
        "zero_get_genesis_proposals",
        "zero_get_evolve_status",
        "zero_get_research_report",
        "zero_get_decision_stack",
        "zero_get_immune_status",
        "zero_get_backtest_report",
        "zero_get_evidence_bundle",
        "zero_get_safety_catalog",
    ]
    forbidden = ("execute", "live", "order", "place", "approve", "wallet")
    assert not any(marker in name for name in names for marker in forbidden)
    assert all("Read-only" in tool["description"] for tool in tools)
    for tool in tools:
        assert tool["safetyClass"] == "read_only_public"
        assert tool["riskDirection"] == "none"
        assert tool["requiresOperatorApproval"] is False
        assert tool["canPlaceOrders"] is False
        assert tool["canChangeRuntimeState"] is False
        assert tool["canReadSecrets"] is False
        assert tool["annotations"]["readOnlyHint"] is True
        assert tool["annotations"]["destructiveHint"] is False
        assert tool["annotations"]["idempotentHint"] is True
        assert tool["annotations"]["openWorldHint"] is False


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
    assert len(listed["result"]["tools"]) == 19
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
        "zero://runtime/status",
        "zero://runtime/health",
        "zero://runtime/parity",
        "zero://journal/tail",
        "zero://rejections/audit",
        "zero://proof/demo",
        "zero://memory/snapshot",
        "zero://memory/stats",
        "zero://genesis/proposals",
        "zero://evolve/status",
        "zero://research/report",
        "zero://decision/stack",
        "zero://immune/status",
        "zero://backtest/report",
        "zero://evidence/bundle",
        "zero://mcp/safety",
    }
    assert read is not None
    proof = json.loads(read["result"]["contents"][0]["text"])
    assert proof["claim_boundary"]["live_trading_claimed"] is False
    assert proof["live_correlation"]["status"] == "unavailable"


def test_memory_snapshot_is_public_safe() -> None:
    called = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {"name": "zero_get_memory_snapshot", "arguments": {}},
        }
    )

    assert called is not None
    payload = json.loads(called["result"]["content"][0]["text"])
    assert payload["schema_version"] == "zero.mcp.memory_snapshot.v1"
    assert payload["paper_only"] is True
    assert payload["stats"]["active_entries"] == 4
    assert payload["stats"]["privacy"]["contains_live_prices"] is False
    assert not any("price" in entry["summary"].lower() for entry in payload["entries"])


def test_runtime_status_and_health_are_public_safe() -> None:
    status_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {"name": "zero_get_runtime_status", "arguments": {}},
        }
    )
    health_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {"name": "zero_get_health", "arguments": {}},
        }
    )

    assert status_call is not None
    status = json.loads(status_call["result"]["content"][0]["text"])
    assert status["schema_version"] == "zero.mcp.runtime_status.v1"
    assert status["mode"] == "paper"
    assert status["paper_only"] is True

    assert health_call is not None
    health = json.loads(health_call["result"]["content"][0]["text"])
    assert health["schema_version"] == "zero.mcp.health_status.v1"
    assert health["mode"] == "paper"
    assert health["paper_only"] is True


def test_runtime_parity_is_public_safe_and_fail_closed() -> None:
    parity_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 131,
            "method": "tools/call",
            "params": {"name": "zero_get_runtime_parity", "arguments": {}},
        }
    )
    resource_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 132,
            "method": "resources/read",
            "params": {"uri": "zero://runtime/parity"},
        }
    )

    assert parity_call is not None
    parity = json.loads(parity_call["result"]["content"][0]["text"])
    assert parity["schema_version"] == "zero.mcp.runtime_parity.v1"
    assert parity["paper_only"] is True
    assert parity["places_live_orders"] is False
    assert parity["live_shadow"]["adapter_orders_placed"] == 0
    assert parity["claim_boundary"]["live_trading_claimed"] is False

    assert resource_call is not None
    resource = json.loads(resource_call["result"]["contents"][0]["text"])
    assert resource["schema_version"] == "zero.mcp.runtime_parity.v1"
    assert resource["ok"] is True


def test_journal_tail_and_rejection_audit_are_paper_only() -> None:
    journal_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": {"name": "zero_get_journal_tail", "arguments": {}},
        }
    )
    audit_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 15,
            "method": "tools/call",
            "params": {"name": "zero_get_rejection_audit", "arguments": {}},
        }
    )

    assert journal_call is not None
    journal = json.loads(journal_call["result"]["content"][0]["text"])
    assert journal["schema_version"] == "zero.mcp.journal_tail.v1"
    assert journal["paper_only"] is True
    assert journal["count"] == 4
    assert len(journal["decisions"]) == 4

    assert audit_call is not None
    audit = json.loads(audit_call["result"]["content"][0]["text"])
    assert audit["schema_version"] == "zero.mcp.rejection_audit.v1"
    assert audit["paper_only"] is True
    assert audit["summary"]["rejections"] == 2
    assert audit["summary"]["by_reason"] == {"order notional exceeds limit": 2}


def test_immune_backtest_evidence_and_safety_catalog_are_read_only() -> None:
    immune_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 16,
            "method": "tools/call",
            "params": {"name": "zero_get_immune_status", "arguments": {}},
        }
    )
    backtest_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 17,
            "method": "tools/call",
            "params": {"name": "zero_get_backtest_report", "arguments": {}},
        }
    )
    evidence_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 18,
            "method": "tools/call",
            "params": {"name": "zero_get_evidence_bundle", "arguments": {}},
        }
    )
    catalog_call = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 19,
            "method": "tools/call",
            "params": {"name": "zero_get_safety_catalog", "arguments": {}},
        }
    )

    assert immune_call is not None
    immune = json.loads(immune_call["result"]["content"][0]["text"])
    assert immune["schema_version"] == "zero.mcp.immune_status.v1"
    assert immune["paper_only"] is True

    assert backtest_call is not None
    backtest = json.loads(backtest_call["result"]["content"][0]["text"])
    assert backtest["schema_version"] == "zero.mcp.backtest_report.v1"
    assert backtest["paper_only"] is True
    assert backtest["claim_boundary"]["live_trading_claimed"] is False
    assert backtest["claim_boundary"]["pnl_claimed"] is False

    assert evidence_call is not None
    evidence = json.loads(evidence_call["result"]["content"][0]["text"])
    assert evidence["schema_version"] == "zero.mcp.evidence_bundle.v1"
    assert evidence["paper_only"] is True
    assert evidence["privacy"]["contains_exchange_credentials"] is False
    assert evidence["canary_rule"]["default_public_runtime_places_live_orders"] is False

    assert catalog_call is not None
    catalog = json.loads(catalog_call["result"]["content"][0]["text"])
    assert catalog["schema_version"] == "zero.mcp.safety_catalog.v1"
    assert catalog["default"] == "read_only_public"
    assert catalog["risk_increasing_tools"] == []
    assert catalog["risk_reducing_tools"] == []
    assert len(catalog["read_only_tools"]) == 19
    assert all(tool["canPlaceOrders"] is False for tool in catalog["read_only_tools"])


def test_genesis_proposals_are_plan_only() -> None:
    called = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {"name": "zero_get_genesis_proposals", "arguments": {}},
        }
    )

    assert called is not None
    payload = json.loads(called["result"]["content"][0]["text"])
    assert payload["schema_version"] == "zero.mcp.genesis_proposals.v1"
    assert payload["paper_only"] is True
    assert payload["applies_code_changes"] is False
    assert payload["stats"]["by_decision"] == {
        "accepted": 1,
        "escalated": 1,
        "rejected": 1,
    }


def test_evolve_status_is_paper_only_and_local() -> None:
    called = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {"name": "zero_get_evolve_status", "arguments": {}},
        }
    )

    assert called is not None
    payload = json.loads(called["result"]["content"][0]["text"])
    assert payload["schema_version"] == "zero.mcp.evolve_status.v1"
    assert payload["paper_only"] is True
    assert payload["pushes_to_remote"] is False
    assert payload["promotion"]["requires_human_approval"] is True
    assert payload["promotion_plan"]["pushes_to_remote"] is False
    assert payload["rollback_plan"]["rollback_ready"] is True
    assert payload["promotion_verification"]["ok"] is True


def test_research_report_is_paper_only_and_read_only() -> None:
    called = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {"name": "zero_get_research_report", "arguments": {}},
        }
    )

    assert called is not None
    payload = json.loads(called["result"]["content"][0]["text"])
    assert payload["schema_version"] == "zero.mcp.research_report.v1"
    assert payload["paper_only"] is True
    assert payload["applies_code_changes"] is False
    assert payload["pushes_to_remote"] is False
    assert payload["claims_live_pnl"] is False
    assert payload["summary"]["sample_size"] == 2


def test_decision_stack_is_paper_only_and_read_only() -> None:
    called = mcp.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {"name": "zero_get_decision_stack", "arguments": {}},
        }
    )

    assert called is not None
    payload = json.loads(called["result"]["content"][0]["text"])
    assert payload["schema_version"] == "zero.mcp.decision_stack.v1"
    assert payload["paper_only"] is True
    assert payload["decision"]["allowed_to_execute_live"] is False
    assert payload["lenses"][0]["lens"] == "price_action"
    assert payload["layers"][0]["layer"] == "data_freshness"
    assert payload["modifiers"][0]["modifier"] == "rejection_first"


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
    memory = mcp.get_memory_snapshot()
    memory_stats = mcp.get_memory_stats()
    genesis = mcp.get_genesis_proposals()
    evolve = mcp.get_evolve_status()
    research = mcp.get_research_report()
    decision_stack = mcp.get_decision_stack()
    runtime = mcp.get_runtime_status()
    parity = mcp.get_runtime_parity()
    health = mcp.get_health_status()
    journal = mcp.get_journal_tail()
    audit = mcp.get_rejection_audit()
    immune = mcp.get_immune_status()
    backtest = mcp.get_backtest_report()
    evidence = mcp.get_evidence_bundle()
    catalog = mcp.safety_catalog()
    scenario_text = mcp.read_resource("zero://paper/scenario")

    assert paper["mode"] == "paper"
    assert paper["fills"] == 2
    assert proof["claim_boundary"]["live_trading_claimed"] is False
    assert memory["paper_only"] is True
    assert memory_stats["paper_only"] is True
    assert genesis["paper_only"] is True
    assert evolve["paper_only"] is True
    assert research["paper_only"] is True
    assert research["pushes_to_remote"] is False
    assert decision_stack["paper_only"] is True
    assert decision_stack["decision"]["allowed_to_execute_live"] is False
    assert runtime["paper_only"] is True
    assert parity["paper_only"] is True
    assert parity["places_live_orders"] is False
    assert parity["claim_boundary"]["live_trading_claimed"] is False
    assert health["paper_only"] is True
    assert journal["paper_only"] is True
    assert audit["paper_only"] is True
    assert immune["paper_only"] is True
    assert backtest["paper_only"] is True
    assert evidence["paper_only"] is True
    assert catalog["risk_increasing_tools"] == []
    assert "paper-launch-smoke" in scenario_text
