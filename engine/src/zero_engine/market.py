from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol


@dataclass(frozen=True)
class MarketDataAdapterMetadata:
    name: str
    version: str
    description: str
    source: str
    deterministic: bool = True
    requires_secrets: bool = False


@dataclass(frozen=True)
class Candle:
    symbol: str
    ts: str
    open: float
    high: float
    low: float
    close: float
    volume: float

    def __post_init__(self) -> None:
        if not self.symbol:
            raise ValueError("symbol is required")
        if not self.ts:
            raise ValueError("ts is required")
        if self.open <= 0 or self.high <= 0 or self.low <= 0 or self.close <= 0:
            raise ValueError("candle prices must be positive")
        if self.high < max(self.open, self.close, self.low):
            raise ValueError("high must be greater than or equal to open, close, and low")
        if self.low > min(self.open, self.close, self.high):
            raise ValueError("low must be less than or equal to open, close, and high")
        if self.volume < 0:
            raise ValueError("volume must be non-negative")


class MarketDataAdapter(Protocol):
    metadata: MarketDataAdapterMetadata

    def candles(self, symbol: str, limit: int | None = None) -> tuple[Candle, ...]:
        """Return candles in chronological order."""

    def latest(self, symbol: str) -> Candle:
        """Return the latest candle for a symbol."""


class JsonlCandleAdapter:
    metadata = MarketDataAdapterMetadata(
        name="jsonl-candles",
        version="0.1.0",
        description="Deterministic OHLCV candles loaded from a local JSONL file.",
        source="local-jsonl",
    )

    def __init__(self, path: str | Path):
        self.path = Path(path)
        self._candles = _load_candles(self.path)

    def candles(self, symbol: str, limit: int | None = None) -> tuple[Candle, ...]:
        normalized = symbol.upper()
        matches = tuple(candle for candle in self._candles if candle.symbol == normalized)
        if limit is not None:
            if limit <= 0:
                raise ValueError("limit must be positive")
            return matches[-limit:]
        return matches

    def latest(self, symbol: str) -> Candle:
        matches = self.candles(symbol)
        if not matches:
            raise KeyError(f"no candles for {symbol.upper()}")
        return matches[-1]


def _load_candles(path: Path) -> tuple[Candle, ...]:
    candles = []
    for line_number, line in enumerate(path.read_text().splitlines(), start=1):
        if not line.strip():
            continue
        try:
            candles.append(_parse_candle(json.loads(line)))
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
            raise ValueError(f"invalid candle at {path}:{line_number}: {exc}") from exc
    if not candles:
        raise ValueError(f"no candles found in {path}")
    return tuple(candles)


def _parse_candle(raw: dict[str, Any]) -> Candle:
    return Candle(
        symbol=str(raw["symbol"]).upper(),
        ts=str(raw["ts"]),
        open=float(raw["open"]),
        high=float(raw["high"]),
        low=float(raw["low"]),
        close=float(raw["close"]),
        volume=float(raw.get("volume", 0)),
    )


def validate_market_data_adapter(adapter: MarketDataAdapter) -> None:
    metadata = adapter.metadata
    if not metadata.name.strip():
        raise ValueError("market data adapter metadata.name is required")
    if not metadata.version.strip():
        raise ValueError("market data adapter metadata.version is required")
    if metadata.requires_secrets:
        raise ValueError("public market data adapters must not require secrets")


def latest_close(adapter: MarketDataAdapter, symbol: str) -> float:
    validate_market_data_adapter(adapter)
    return adapter.latest(symbol).close
