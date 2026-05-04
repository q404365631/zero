from __future__ import annotations

from dataclasses import dataclass

from zero_engine import MarketDataAdapter, Side, StrategyPluginMetadata, StrategySignal


@dataclass(frozen=True)
class PaperMomentumPlugin:
    metadata: StrategyPluginMetadata = StrategyPluginMetadata(
        name="paper-momentum",
        version="0.1.0",
        description="Paper-only plugin that buys when recent closes show momentum.",
    )
    lookback: int = 2
    min_move_pct: float = 0.01
    quantity: float = 0.01
    confidence: float = 0.82

    def propose(self, market: MarketDataAdapter, symbol: str) -> StrategySignal | None:
        candles = market.candles(symbol, limit=self.lookback)
        if len(candles) < self.lookback:
            return None

        first = candles[0]
        latest = candles[-1]
        move_pct = (latest.close - first.close) / first.close
        if move_pct < self.min_move_pct:
            return None

        return StrategySignal(
            symbol=latest.symbol,
            side=Side.BUY,
            confidence=self.confidence,
            quantity=self.quantity,
            reason=(
                f"{self.metadata.name}: close momentum {move_pct:.2%}; "
                f"threshold {self.min_move_pct:.2%}"
            ),
        )
