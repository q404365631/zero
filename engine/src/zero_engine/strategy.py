from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

from zero_engine.market import Candle, MarketDataAdapter
from zero_engine.models import OrderIntent, Side


@dataclass(frozen=True)
class StrategySignal:
    symbol: str
    side: Side
    confidence: float
    quantity: float
    reason: str

    def to_order(self, price: float) -> OrderIntent:
        return OrderIntent(
            symbol=self.symbol,
            side=self.side,
            quantity=self.quantity,
            price=price,
            confidence=self.confidence,
        )


class Strategy(Protocol):
    name: str

    def propose(self, market: MarketDataAdapter, symbol: str) -> StrategySignal | None:
        """Return a proposed paper signal or None when there is no setup."""


@dataclass(frozen=True)
class MomentumStrategy:
    name: str = "momentum-close-above-open"
    min_move_pct: float = 0.005
    quantity: float = 0.01
    confidence: float = 0.75

    def propose(self, market: MarketDataAdapter, symbol: str) -> StrategySignal | None:
        latest = market.latest(symbol)
        move_pct = candle_move_pct(latest)
        if move_pct < self.min_move_pct:
            return None
        return StrategySignal(
            symbol=latest.symbol,
            side=Side.BUY,
            confidence=self.confidence,
            quantity=self.quantity,
            reason=(
                f"close above open by {move_pct:.2%}; "
                f"threshold {self.min_move_pct:.2%}"
            ),
        )


def candle_move_pct(candle: Candle) -> float:
    return (candle.close - candle.open) / candle.open


def propose_order(strategy: Strategy, market: MarketDataAdapter, symbol: str) -> OrderIntent | None:
    signal = strategy.propose(market, symbol)
    if signal is None:
        return None
    return signal.to_order(market.latest(symbol).close)
