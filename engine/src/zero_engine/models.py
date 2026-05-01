from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum


class Side(StrEnum):
    BUY = "buy"
    SELL = "sell"


@dataclass(frozen=True)
class RiskLimits:
    max_notional_usd: float = 1_000.0
    max_position_notional_usd: float = 2_500.0
    max_leverage: float = 1.0
    min_confidence: float = 0.60

    def __post_init__(self) -> None:
        for name, value in (
            ("max_notional_usd", self.max_notional_usd),
            ("max_position_notional_usd", self.max_position_notional_usd),
            ("max_leverage", self.max_leverage),
            ("min_confidence", self.min_confidence),
        ):
            if value <= 0:
                raise ValueError(f"{name} must be positive")
        if self.min_confidence > 1:
            raise ValueError("min_confidence must be <= 1")


@dataclass(frozen=True)
class OrderIntent:
    symbol: str
    side: Side
    quantity: float
    price: float
    confidence: float
    reduce_only: bool = False

    @property
    def notional_usd(self) -> float:
        return self.quantity * self.price

    def __post_init__(self) -> None:
        if not self.symbol:
            raise ValueError("symbol is required")
        if self.quantity <= 0:
            raise ValueError("quantity must be positive")
        if self.price <= 0:
            raise ValueError("price must be positive")
        if not 0 <= self.confidence <= 1:
            raise ValueError("confidence must be between 0 and 1")


@dataclass
class Position:
    symbol: str
    quantity: float = 0.0
    avg_price: float = 0.0

    @property
    def notional_usd(self) -> float:
        return abs(self.quantity * self.avg_price)

