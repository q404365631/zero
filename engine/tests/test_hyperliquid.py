from __future__ import annotations

from typing import Any

from zero_engine.hyperliquid import HyperliquidInfoClient, is_hex_address


def test_all_mids_normalizes_numeric_string_prices() -> None:
    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> dict[str, str]:
        assert payload == {"type": "allMids"}
        return {"BTC": "40500.5", "eth": "2850"}

    client = HyperliquidInfoClient(transport=transport)

    assert client.all_mids() == {"BTC": 40500.5, "ETH": 2850.0}


def test_all_mids_rejects_non_positive_price() -> None:
    client = HyperliquidInfoClient(transport=lambda *_args: {"BTC": "0"})

    try:
        client.all_mids()
    except ValueError as exc:
        assert str(exc) == "mid price for BTC must be positive"
    else:
        raise AssertionError("expected invalid mid price to fail")


def test_market_status_filters_symbols_for_wire_payload() -> None:
    client = HyperliquidInfoClient(transport=lambda *_args: {"BTC": "40500", "ETH": "2850"})

    payload = client.market_status().to_dict(symbols=["BTC"])

    assert payload["coins"] == 2
    assert payload["mids"] == {"BTC": 40500.0}


def test_clearinghouse_state_requires_master_or_subaccount_address() -> None:
    client = HyperliquidInfoClient(transport=lambda *_args: {})

    try:
        client.clearinghouse_state("not-an-address")
    except ValueError as exc:
        assert str(exc) == "user must be a 42-character hex address"
    else:
        raise AssertionError("expected invalid user address to fail")


def test_clearinghouse_state_posts_read_only_info_request() -> None:
    seen: dict[str, Any] = {}

    def transport(_endpoint: str, payload: dict[str, Any], _timeout_s: float) -> dict[str, Any]:
        seen.update(payload)
        return {"assetPositions": []}

    client = HyperliquidInfoClient(transport=transport)
    state = client.clearinghouse_state("0x0000000000000000000000000000000000000000")

    assert seen == {
        "type": "clearinghouseState",
        "user": "0x0000000000000000000000000000000000000000",
    }
    assert state == {"assetPositions": []}


def test_is_hex_address() -> None:
    assert is_hex_address("0x0000000000000000000000000000000000000000")
    assert not is_hex_address("0x000000000000000000000000000000000000000")
