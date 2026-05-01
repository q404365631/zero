from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from zero_engine.models import OrderIntent, RiskLimits, Side


@dataclass(frozen=True)
class PaperScenario:
    name: str
    mode: str
    limits: RiskLimits
    orders: tuple[OrderIntent, ...]


def load_scenario(path: str | Path) -> PaperScenario:
    data = json.loads(Path(path).read_text())
    return parse_scenario(data)


def parse_scenario(data: dict[str, Any]) -> PaperScenario:
    mode = str(data.get("mode", "paper"))
    if mode != "paper":
        raise ValueError("public scenarios must use paper mode")

    limits = _parse_limits(data.get("limits", {}))
    orders = tuple(_parse_order(raw) for raw in data.get("orders", []))
    if not orders:
        raise ValueError("scenario must contain at least one order")

    return PaperScenario(
        name=str(data.get("name", "paper-scenario")),
        mode=mode,
        limits=limits,
        orders=orders,
    )


def _parse_limits(raw: dict[str, Any]) -> RiskLimits:
    return RiskLimits(
        max_notional_usd=float(raw.get("max_notional_usd", RiskLimits.max_notional_usd)),
        max_position_notional_usd=float(
            raw.get("max_position_notional_usd", RiskLimits.max_position_notional_usd)
        ),
        max_leverage=float(raw.get("max_leverage", RiskLimits.max_leverage)),
        min_confidence=float(raw.get("min_confidence", RiskLimits.min_confidence)),
    )


def _parse_order(raw: dict[str, Any]) -> OrderIntent:
    return OrderIntent(
        symbol=str(raw["symbol"]).upper(),
        side=Side(str(raw["side"]).lower()),
        quantity=float(raw["quantity"]),
        price=float(raw["price"]),
        confidence=float(raw["confidence"]),
        reduce_only=bool(raw.get("reduce_only", False)),
    )
