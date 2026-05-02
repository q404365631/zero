from __future__ import annotations

import json
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import Any
from urllib import request

from zero_engine.reconciliation import HyperliquidAccountSnapshot, parse_hyperliquid_account


DEFAULT_INFO_URL = "https://api.hyperliquid.xyz/info"
Transport = Callable[[str, Mapping[str, Any], float], Any]


@dataclass(frozen=True)
class HyperliquidMarketStatus:
    endpoint: str
    mids: dict[str, float]

    @property
    def coins(self) -> list[str]:
        return sorted(self.mids)

    def price_for(self, symbol: str) -> float | None:
        return self.mids.get(symbol.upper())

    def to_dict(self, symbols: list[str] | None = None) -> dict[str, Any]:
        selected = self.mids if symbols is None else {symbol: self.mids[symbol] for symbol in symbols if symbol in self.mids}
        return {
            "endpoint": self.endpoint,
            "coins": len(self.mids),
            "mids": selected,
        }


class HyperliquidInfoClient:
    """Read-only client for Hyperliquid's public info endpoint."""

    def __init__(
        self,
        endpoint: str = DEFAULT_INFO_URL,
        *,
        timeout_s: float = 5.0,
        transport: Transport | None = None,
    ) -> None:
        self.endpoint = endpoint
        self.timeout_s = timeout_s
        self.transport = transport or post_json

    def all_mids(self) -> dict[str, float]:
        data = self._post({"type": "allMids"})
        if not isinstance(data, dict):
            raise ValueError("Hyperliquid allMids response must be an object")
        mids: dict[str, float] = {}
        for symbol, raw_price in data.items():
            if not isinstance(symbol, str):
                continue
            mids[symbol.upper()] = parse_positive_float(raw_price, f"mid price for {symbol}")
        return mids

    def market_status(self) -> HyperliquidMarketStatus:
        return HyperliquidMarketStatus(endpoint=self.endpoint, mids=self.all_mids())

    def clearinghouse_state(self, user: str) -> dict[str, Any]:
        if not is_hex_address(user):
            raise ValueError("user must be a 42-character hex address")
        data = self._post({"type": "clearinghouseState", "user": user})
        if not isinstance(data, dict):
            raise ValueError("Hyperliquid clearinghouseState response must be an object")
        return data

    def open_orders(self, user: str) -> list[dict[str, Any]]:
        if not is_hex_address(user):
            raise ValueError("user must be a 42-character hex address")
        data = self._post({"type": "openOrders", "user": user})
        if not isinstance(data, list):
            raise ValueError("Hyperliquid openOrders response must be a list")
        return [dict(order) for order in data if isinstance(order, dict)]

    def account_snapshot(self, user: str) -> HyperliquidAccountSnapshot:
        return parse_hyperliquid_account(
            user=user,
            clearinghouse_state=self.clearinghouse_state(user),
            open_orders=self.open_orders(user),
        )

    def _post(self, payload: Mapping[str, Any]) -> Any:
        return self.transport(self.endpoint, payload, self.timeout_s)


def redact_secret(value: str | None) -> str:
    if not value:
        return "<unset>"
    value = value.strip()
    if len(value) <= 10:
        return "<redacted>"
    return f"{value[:6]}...{value[-4:]}"


def post_json(endpoint: str, payload: Mapping[str, Any], timeout_s: float) -> Any:
    body = json.dumps(payload, separators=(",", ":")).encode("utf-8")
    req = request.Request(
        endpoint,
        data=body,
        headers={"content-type": "application/json"},
        method="POST",
    )
    with request.urlopen(req, timeout=timeout_s) as response:
        return json.loads(response.read().decode("utf-8"))


def parse_positive_float(value: Any, label: str) -> float:
    try:
        parsed = float(value)
    except (TypeError, ValueError) as exc:
        raise ValueError(f"{label} must be numeric") from exc
    if parsed <= 0:
        raise ValueError(f"{label} must be positive")
    return parsed


def is_hex_address(value: str) -> bool:
    if len(value) != 42 or not value.startswith("0x"):
        return False
    return all(char in "0123456789abcdefABCDEF" for char in value[2:])


def is_private_key(value: str | None) -> bool:
    if value is None:
        return False
    normalized = value.removeprefix("0x")
    return len(normalized) == 64 and all(char in "0123456789abcdefABCDEF" for char in normalized)


def validate_dry_run_order(payload: Mapping[str, Any]) -> dict[str, Any]:
    symbol = str(payload.get("coin") or payload.get("symbol") or "").upper()
    side = str(payload.get("side") or "").lower()
    try:
        size = float(payload.get("size") or payload.get("quantity") or 0)
    except (TypeError, ValueError) as exc:
        raise ValueError("dry-run order size must be numeric") from exc
    if not symbol:
        raise ValueError("dry-run order requires a symbol")
    if side not in {"buy", "sell"}:
        raise ValueError("dry-run order side must be buy or sell")
    if size <= 0:
        raise ValueError("dry-run order size must be positive")
    return {"coin": symbol, "side": side, "size": size, "dry_run": True}
