from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from zero_engine.api import PaperApi, PaperApiState, websocket_accept_key, websocket_text_frame
from zero_engine.hyperliquid import HyperliquidInfoClient
from zero_engine.journal import DecisionJournal
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
