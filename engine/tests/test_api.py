from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from zero_engine.api import PaperApi, PaperApiState, websocket_accept_key, websocket_text_frame
from zero_engine.hyperliquid import HyperliquidInfoClient
from zero_engine.journal import DecisionJournal
from zero_engine.live import LiveExecutor, RecordingExchangeAdapter
from zero_engine.paper import PaperEngine


CONTRACT_DIR = Path(__file__).resolve().parents[2] / "contracts" / "paper-api"
FIXED_DT = datetime(2026, 5, 1, tzinfo=UTC)
FIXED_TS = FIXED_DT.timestamp()


def contract_api() -> PaperApi:
    return PaperApi(
        PaperApiState(
            engine=PaperEngine(clock=lambda: FIXED_TS),
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )


def read_contract(name: str) -> dict[str, Any]:
    return json.loads((CONTRACT_DIR / name).read_text())


def test_paper_api_status_matches_cli_contract() -> None:
    status, payload = PaperApi().get("/v2/status", {})

    assert status == 200
    assert payload["confidence"]["level"] == "paper"
    assert payload["market"]["regime"].startswith("PAPER MARKET")
    assert payload["positions"]["open"] == 0


def test_paper_api_execute_records_paper_fill_and_position() -> None:
    api = PaperApi()

    status, payload = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "abc-123"},
    )
    positions_status, positions = api.get("/positions", {})
    pulse_status, pulse = api.get("/pulse", {"limit": ["10"]})

    assert status == 200
    assert payload["accepted"] is True
    assert payload["simulated"] is True
    assert payload["fill_id"] == "paper-abc-123"
    assert positions_status == 200
    assert positions["positions"][0]["symbol"] == "BTC"
    assert pulse_status == 200
    assert pulse["events"][0]["message"] == "allowed"


def test_paper_api_execute_is_idempotent_by_key() -> None:
    api = PaperApi()
    payload = {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "same-key"}

    first_status, first = api.post("/execute", payload)
    second_status, second = api.post("/execute", payload)

    assert first_status == 200
    assert second_status == 200
    assert first == second
    assert len(api.state.engine.fills) == 1


def test_paper_api_rejections_feed_matches_cli_contract() -> None:
    api = PaperApi()

    status, payload = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 1.0, "idempotency_key": "too-large"},
    )
    rejections_status, rejections = api.get("/rejections", {"limit": ["5"]})

    assert status == 200
    assert payload["accepted"] is False
    assert payload["reason"] == "order notional exceeds limit"
    assert rejections_status == 200
    assert rejections["rejections"][0]["coin"] == "BTC"
    assert rejections["rejections"][0]["stage"] == "risk"


def test_paper_api_journal_reads_persisted_decisions(tmp_path) -> None:
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    api = PaperApi(PaperApiState(engine=PaperEngine(clock=lambda: FIXED_TS, journal=journal)))

    execute_status, _ = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "journal-fill"},
    )
    journal_status, payload = api.get("/journal", {"limit": ["10"]})

    assert execute_status == 200
    assert journal_status == 200
    assert payload["count"] == 1
    assert payload["decisions"][0]["symbol"] == "BTC"
    assert payload["decisions"][0]["source"] == "api:/execute"
    assert payload["decisions"][0]["allowed"] is True
    assert payload["decisions"][0]["idempotency_key"] == "journal-fill"
    assert payload["decisions"][0]["trace_id"].startswith("trace-")


def test_paper_api_metrics_tracks_requests_and_execute_outcomes() -> None:
    api = PaperApi(PaperApiState(clock=lambda: FIXED_DT, started_at=FIXED_DT))

    execute_status, execute = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "metrics-fill"},
        trace_id="trace-test-metrics",
        expose_trace=True,
    )
    duplicate_status, duplicate = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "metrics-fill"},
        trace_id="trace-test-duplicate",
        expose_trace=True,
    )
    metrics_status, metrics = api.get("/metrics", {}, trace_id="trace-test-read")

    assert execute_status == 200
    assert duplicate_status == 200
    assert execute["trace_id"] == "trace-test-metrics"
    assert duplicate["trace_id"] == "trace-test-metrics"
    assert metrics_status == 200
    assert metrics["schema_version"] == "zero.metrics.v1"
    assert metrics["api"]["request_count"] == 2
    assert metrics["api"]["by_path"]["/execute"] == 2
    assert metrics["api"]["execute_count"] == 2
    assert metrics["api"]["execute_accepted"] == 2
    assert metrics["api"]["idempotency_hits"] == 1
    assert metrics["engine"]["decisions"] == 1
    assert metrics["engine"]["fills"] == 1
    assert metrics["engine"]["acceptance_rate"] == 1.0


def test_paper_api_audit_export_includes_traceable_decisions(tmp_path) -> None:
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    api = PaperApi(
        PaperApiState(
            engine=PaperEngine(clock=lambda: FIXED_TS, journal=journal),
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )

    api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "audit-fill"},
        trace_id="trace-test-audit",
    )
    audit_status, audit = api.get("/audit/export", {"limit": ["10"]}, trace_id="trace-test-export")

    assert audit_status == 200
    assert audit["schema_version"] == "zero.audit.v1"
    assert audit["source"] == "journal"
    assert audit["summary"]["decisions"] == 1
    assert audit["retention"]["format"] == "append-only-jsonl"
    assert audit["decisions"][0]["symbol"] == "BTC"
    assert audit["decisions"][0]["trace_id"] == "trace-test-audit"


def test_paper_api_recovers_runtime_state_and_idempotency_from_journal(tmp_path) -> None:
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    first = PaperApi(PaperApiState(engine=PaperEngine(clock=lambda: FIXED_TS, journal=journal)))

    execute_status, first_payload = first.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "recover-fill"},
    )
    recovered_engine = PaperEngine.recover_from_journal(journal, clock=lambda: FIXED_TS)
    recovered = PaperApi(
        PaperApiState(
            engine=recovered_engine,
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )
    replay_status, replay_payload = recovered.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "recover-fill"},
    )
    health_status, health = recovered.get("/health", {})
    status_status, status = recovered.get("/v2/status", {})

    assert execute_status == 200
    assert replay_status == 200
    assert first_payload == replay_payload
    assert len(recovered.state.engine.fills) == 1
    assert recovered.state.engine.positions["BTC"].quantity == 0.01
    assert health_status == 200
    assert health["dependencies"]["journal"] == "durable"
    assert health["recovery"]["status"] == "recovered"
    assert health["recovery"]["current_decisions"] == 1
    assert health["recovery"]["current_positions"] == 1
    assert status_status == 200
    assert status["recovery"]["durable"] is True
    assert status["recovery"]["decisions_recovered"] == 1


def test_paper_api_hl_status_is_disabled_by_default() -> None:
    status, payload = PaperApi().get("/hl/status", {})

    assert status == 200
    assert payload["enabled"] is False
    assert payload["exchange"] == "hyperliquid"


def test_paper_api_hl_status_uses_read_only_adapter() -> None:
    client = HyperliquidInfoClient(transport=lambda *_args: {"BTC": "40500", "ETH": "2850"})
    api = PaperApi(PaperApiState(hyperliquid=client))

    status, payload = api.get("/hl/status", {"symbol": ["BTC"]})
    health_status, health = api.get("/health", {})

    assert status == 200
    assert payload["enabled"] is True
    assert payload["secrets_required"] is False
    assert payload["mids"] == {"BTC": 40500.0}
    assert health_status == 200
    assert health["dependencies"]["exchange"] == "hyperliquid"
    assert health["immune"]["schema_version"] == "zero.immune.v1"


def test_live_preflight_refuses_without_local_custody_controls() -> None:
    api = PaperApi(PaperApiState(clock=lambda: FIXED_DT, started_at=FIXED_DT))

    status, payload = api.get("/live/preflight", {})

    assert status == 200
    assert payload["schema_version"] == "zero.live_preflight.v1"
    assert payload["ready"] is False
    assert payload["live_mode"] == "refused"
    checks = {check["name"]: check for check in payload["checks"]}
    assert checks["live_executor"]["status"] == "fail"
    assert checks["wallet_address"]["status"] == "fail"
    assert checks["api_private_key"]["note"] == "store key locally; never commit it"
    assert checks["immune_breakers"]["status"] == "fail"
    assert payload["immune"]["risk_increasing_allowed"] is False


def test_live_preflight_verifies_controls_without_leaking_private_key(tmp_path) -> None:
    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> Any:
        if payload["type"] == "clearinghouseState":
            return {"assetPositions": []}
        if payload["type"] == "openOrders":
            return []
        return {"BTC": "40500"}

    kill = tmp_path / "kill-switch"
    kill.write_text("armed\n")
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    api = PaperApi(
        PaperApiState(
            engine=PaperEngine(clock=lambda: FIXED_TS, journal=journal),
            hyperliquid=HyperliquidInfoClient(transport=transport),
            live_wallet_address="0x0000000000000000000000000000000000000000",
            live_api_private_key="0x" + ("1" * 64),
            live_kill_switch_path=str(kill),
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )

    status, payload = api.get("/live/preflight", {})

    assert status == 200
    assert payload["ready"] is False
    assert payload["controls_ready"] is True
    body = json.dumps(payload)
    assert "1111111111111111111111111111111111111111111111111111111111111111" not in body
    checks = {check["name"]: check for check in payload["checks"]}
    assert checks["api_private_key"]["status"] == "ok"
    assert checks["account_read"]["status"] == "ok"
    assert checks["reconciliation"]["status"] == "ok"
    assert checks["journal"]["status"] == "ok"
    assert checks["emergency_controls"]["status"] == "ok"
    assert checks["immune_breakers"]["status"] == "fail"


def test_live_preflight_can_pass_when_executor_and_controls_are_ready(tmp_path) -> None:
    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> Any:
        if payload["type"] == "clearinghouseState":
            return {"assetPositions": []}
        if payload["type"] == "openOrders":
            return []
        return {"BTC": "40500"}

    kill = tmp_path / "kill-switch"
    kill.write_text("armed\n")
    executor = LiveExecutor(adapter=RecordingExchangeAdapter(), enabled=True)
    executor.heartbeat()
    api = PaperApi(
        PaperApiState(
            engine=PaperEngine(clock=lambda: FIXED_TS, journal=DecisionJournal(tmp_path / "d.jsonl")),
            hyperliquid=HyperliquidInfoClient(transport=transport),
            live_wallet_address="0x0000000000000000000000000000000000000000",
            live_api_private_key="0x" + ("1" * 64),
            live_kill_switch_path=str(kill),
            live_executor=executor,
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )

    status, payload = api.get("/live/preflight", {})

    assert status == 200
    assert payload["ready"] is True
    assert payload["live_mode"] == "ready"
    assert payload["immune"]["risk_increasing_allowed"] is True


def test_immune_endpoint_exposes_risk_blocking_breakers() -> None:
    status, payload = PaperApi(PaperApiState(clock=lambda: FIXED_DT)).get("/immune", {})

    assert status == 200
    assert payload["schema_version"] == "zero.immune.v1"
    assert payload["risk_increasing_allowed"] is False
    breakers = {breaker["name"]: breaker for breaker in payload["breakers"]}
    assert breakers["dead_man"]["status"] == "open"
    assert breakers["reconciliation"]["status"] == "open"
    assert breakers["stale_market_data"]["status"] == "closed"


def test_live_certification_endpoint_returns_dry_run_evidence() -> None:
    status, payload = PaperApi(PaperApiState(clock=lambda: FIXED_DT)).get("/live/certification", {})

    assert status == 200
    assert payload["schema_version"] == "zero.live_certification.v1"
    assert payload["passed"] is True
    assert payload["summary"]["orders_placed_live"] == 0
    drills = {drill["name"]: drill for drill in payload["drills"]}
    assert drills["risk_increase_requires_heartbeat"]["status"] == "pass"
    assert drills["exchange_submit_outage_fails_closed_without_retry"]["status"] == "pass"


def test_hl_account_and_reconciliation_expose_read_only_account_truth() -> None:
    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> Any:
        if payload["type"] == "clearinghouseState":
            return {
                "marginSummary": {"accountValue": "10000", "totalMarginUsed": "0"},
                "assetPositions": [],
            }
        if payload["type"] == "openOrders":
            return [{"coin": "BTC", "oid": 123}]
        return {"BTC": "40500"}

    api = PaperApi(
        PaperApiState(
            hyperliquid=HyperliquidInfoClient(transport=transport),
            live_wallet_address="0x0000000000000000000000000000000000000000",
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )

    account_status, account = api.get("/hl/account", {})
    reconcile_status, reconcile = api.get("/hl/reconcile", {})

    assert account_status == 200
    assert account["schema_version"] == "zero.hl_account.v1"
    assert account["user"] == "0x0000...0000"
    assert account["counts"]["open_orders"] == 1
    assert reconcile_status == 200
    assert reconcile["schema_version"] == "zero.reconciliation.v1"
    assert reconcile["status"] == "ok"
    assert reconcile["risk_increasing_allowed"] is True


def test_live_execute_uses_live_executor_and_preserves_idempotency() -> None:
    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> Any:
        if payload["type"] == "clearinghouseState":
            return {"assetPositions": []}
        if payload["type"] == "openOrders":
            return []
        return {"BTC": "40500"}

    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True, clock=lambda: FIXED_TS)
    executor.heartbeat()
    api = PaperApi(
        PaperApiState(
            hyperliquid=HyperliquidInfoClient(transport=transport),
            live_wallet_address="0x0000000000000000000000000000000000000000",
            live_executor=executor,
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )
    body = {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "live-fill"}

    first_status, first = api.post("/execute", body, trace_id="trace-live", expose_trace=True, mode="live")
    second_status, second = api.post(
        "/execute",
        {**body, "size": 0.02},
        trace_id="trace-live-dup",
        expose_trace=True,
        mode="live",
    )

    assert first_status == 200
    assert second_status == 200
    assert first["accepted"] is True
    assert first["simulated"] is False
    assert first["trace_id"] == "trace-live"
    assert second == first
    assert len(adapter.placed) == 1


def test_live_execute_blocks_risk_increase_when_reconciliation_drift_exists() -> None:
    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> Any:
        if payload["type"] == "clearinghouseState":
            return {
                "assetPositions": [
                    {"position": {"coin": "BTC", "szi": "0.01", "entryPx": "50000"}}
                ]
            }
        if payload["type"] == "openOrders":
            return []
        return {"BTC": "50000"}

    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True, clock=lambda: FIXED_TS)
    executor.heartbeat()
    api = PaperApi(
        PaperApiState(
            hyperliquid=HyperliquidInfoClient(transport=transport),
            live_wallet_address="0x0000000000000000000000000000000000000000",
            live_executor=executor,
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
        )
    )

    status, payload = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "blocked-drift"},
        mode="live",
    )

    assert status == 200
    assert payload["accepted"] is False
    assert payload["reason"].startswith("reconciliation local_lag:")
    assert adapter.placed == []


def test_live_kill_blocks_later_live_execute() -> None:
    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> Any:
        if payload["type"] == "clearinghouseState":
            return {"assetPositions": []}
        if payload["type"] == "openOrders":
            return []
        return {"BTC": "40500"}

    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True, clock=lambda: FIXED_TS)
    executor.heartbeat()
    api = PaperApi(
        PaperApiState(
            hyperliquid=HyperliquidInfoClient(transport=transport),
            live_wallet_address="0x0000000000000000000000000000000000000000",
            live_executor=executor,
            clock=lambda: FIXED_DT,
        )
    )

    kill_status, kill = api.post("/live/kill", {}, mode="live")
    execute_status, execute = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "after-kill"},
        mode="live",
    )

    assert kill_status == 200
    assert kill["state"] == "killed"
    assert execute_status == 200
    assert execute["accepted"] is False
    assert execute["reason"] == "kill switch active"


def test_paper_api_market_quote_uses_fixture_prices_by_default() -> None:
    status, payload = PaperApi(PaperApiState(clock=lambda: FIXED_DT)).get(
        "/market/quote",
        {"symbol": ["BTC"]},
    )

    assert status == 200
    assert payload == {
        "symbol": "BTC",
        "price": 40500.0,
        "source": "paper:static",
        "as_of": "2026-05-01T00:00:00Z",
        "mode": "paper",
        "live": False,
    }


def test_live_hyperliquid_prices_feed_paper_execute_and_journal(tmp_path) -> None:
    client = HyperliquidInfoClient(transport=lambda *_args: {"BTC": "50000", "ETH": "3000"})
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    api = PaperApi(
        PaperApiState(
            engine=PaperEngine(clock=lambda: FIXED_TS, journal=journal),
            hyperliquid=client,
            use_live_hyperliquid_prices=True,
            clock=lambda: FIXED_DT,
        )
    )

    status, payload = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "live-fill"},
    )
    quote_status, quote = api.get("/market/quote", {"symbol": ["BTC"]})
    journal_status, journal_payload = api.get("/journal", {"limit": ["5"]})

    assert status == 200
    assert payload["accepted"] is True
    assert api.state.engine.fills[0].price == 50000.0
    assert api.state.engine.positions["BTC"].avg_price == 50000.0
    assert quote_status == 200
    assert quote["source"] == "hyperliquid:allMids"
    assert quote["price"] == 50000.0
    assert journal_status == 200
    assert journal_payload["decisions"][0]["source"] == "api:/execute:hyperliquid:allMids"
    assert journal_payload["decisions"][0]["price"] == 50000.0


def test_live_hyperliquid_positions_mark_to_cached_mid() -> None:
    mids = iter([{"BTC": "50000"}, {"BTC": "51000"}])
    client = HyperliquidInfoClient(transport=lambda *_args: next(mids))
    api = PaperApi(
        PaperApiState(
            hyperliquid=client,
            use_live_hyperliquid_prices=True,
            price_cache_ttl_s=-1,
            clock=lambda: FIXED_DT,
        )
    )

    execute_status, _ = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "mark-fill"},
    )
    positions_status, positions = api.get("/positions", {})

    assert execute_status == 200
    assert positions_status == 200
    assert positions["positions"][0]["entry"] == 50000.0
    assert positions["positions"][0]["mark"] == 51000.0
    assert positions["positions"][0]["unrealized_pnl"] == 10.0
    assert positions["total_unrealized_pnl"] == 10.0


def test_live_hyperliquid_unknown_symbol_fails_without_fixture_fallback() -> None:
    client = HyperliquidInfoClient(transport=lambda *_args: {"BTC": "50000"})
    api = PaperApi(
        PaperApiState(
            hyperliquid=client,
            use_live_hyperliquid_prices=True,
        )
    )

    status, payload = api.post(
        "/execute",
        {"coin": "NOTREAL", "side": "buy", "size": 1, "idempotency_key": "missing"},
    )

    assert status == 400
    assert payload == {"error": "NOTREAL missing from Hyperliquid allMids"}


def test_paper_api_matches_shared_contract_fixtures() -> None:
    api = contract_api()

    accepted_status, accepted = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 0.01, "idempotency_key": "contract-fill"},
    )
    rejected_status, rejected = api.post(
        "/execute",
        {"coin": "BTC", "side": "buy", "size": 1.0, "idempotency_key": "contract-reject"},
    )

    assert accepted_status == 200
    assert rejected_status == 200
    assert accepted == read_contract("execute_accepted.json")
    assert rejected == read_contract("execute_rejected.json")

    fixtures = [
        ("/v2/status", {}, "v2_status.json"),
        ("/positions", {}, "positions.json"),
        ("/risk", {}, "risk.json"),
        ("/brief", {}, "brief.json"),
        ("/rejections", {"limit": ["5"]}, "rejections.json"),
    ]
    for endpoint, query, fixture in fixtures:
        status, payload = api.get(endpoint, query)

        assert status == 200
        assert payload == read_contract(fixture)


def test_websocket_helpers_match_rfc_handshake_vector() -> None:
    accept = websocket_accept_key("dGhlIHNhbXBsZSBub25jZQ==")
    frame = websocket_text_frame("ok")

    assert accept == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    assert frame == b"\x81\x02ok"
