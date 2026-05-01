from __future__ import annotations

from dataclasses import dataclass

from zero_engine import MarketDataAdapter, Side, StrategyPluginMetadata, StrategySignal
from zero_engine.strategy import candle_move_pct


@dataclass(frozen=True)
class CloseStrengthPlugin:
    metadata: StrategyPluginMetadata = StrategyPluginMetadata(
        name="close-strength",
        version="0.1.0",
        description="Paper-only example plugin that buys when the latest candle closes strongly.",
    )
    min_move_pct: float = 0.01
    quantity: float = 0.01
    confidence: float = 0.8

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
            reason=f"{self.metadata.name}: close moved {move_pct:.2%}",
        )
