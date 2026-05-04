from __future__ import annotations

from pathlib import Path

from zero_engine import Candle, JsonlCandleAdapter, MarketDataAdapterMetadata


class FixtureCandleAdapter:
    metadata: MarketDataAdapterMetadata = MarketDataAdapterMetadata(
        name="fixture-candles",
        version="0.1.0",
        description="Paper-only example adapter backed by a checked-in OHLCV JSONL fixture.",
        source="local-jsonl-fixture",
    )

    def __init__(self, path: str | Path):
        self.path = Path(path)
        self._adapter = JsonlCandleAdapter(self.path)

    def candles(self, symbol: str, limit: int | None = None) -> tuple[Candle, ...]:
        return self._adapter.candles(symbol, limit=limit)

    def latest(self, symbol: str) -> Candle:
        return self._adapter.latest(symbol)


def example_adapter(path: str | Path | None = None) -> FixtureCandleAdapter:
    fixture_path = Path(path) if path is not None else Path(__file__).with_name("candles.jsonl")
    return FixtureCandleAdapter(fixture_path)
