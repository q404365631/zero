from __future__ import annotations

from zero_engine.api import PaperApi, websocket_accept_key, websocket_text_frame


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


def test_websocket_helpers_match_rfc_handshake_vector() -> None:
    accept = websocket_accept_key("dGhlIHNhbXBsZSBub25jZQ==")
    frame = websocket_text_frame("ok")

    assert accept == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    assert frame == b"\x81\x02ok"
