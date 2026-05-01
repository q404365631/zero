from __future__ import annotations

from dataclasses import dataclass

from zero_engine import Candle, MarketDataAdapterMetadata


@dataclass(frozen=True)
class MemoryCandleAdapter:
    candles_by_symbol: dict[str, tuple[Candle, ...]]
    metadata: MarketDataAdapterMetadata = MarketDataAdapterMetadata(
        name="memory-candles",
        version="0.1.0",
        description="Paper-only example adapter backed by in-memory OHLCV candles.",
        source="example-fixture",
    )

    def candles(self, symbol: str, limit: int | None = None) -> tuple[Candle, ...]:
        normalized = symbol.upper()
        candles = self.candles_by_symbol.get(normalized, ())
        if limit is not None:
            if limit <= 0:
                raise ValueError("limit must be positive")
            return candles[-limit:]
        return candles

    def latest(self, symbol: str) -> Candle:
        candles = self.candles(symbol)
        if not candles:
            raise KeyError(f"no candles for {symbol.upper()}")
        return candles[-1]


def example_adapter() -> MemoryCandleAdapter:
    return MemoryCandleAdapter(
        {
            "BTC": (
                Candle(
                    symbol="BTC",
                    ts="2026-05-01T00:00:00Z",
                    open=40000,
                    high=40100,
                    low=39900,
                    close=40050,
                    volume=1000,
                ),
                Candle(
                    symbol="BTC",
                    ts="2026-05-01T00:05:00Z",
                    open=40050,
                    high=40650,
                    low=40000,
                    close=40550,
                    volume=1200,
                ),
            )
        }
    )
