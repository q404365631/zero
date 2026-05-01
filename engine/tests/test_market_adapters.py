from pathlib import Path

import pytest

from zero_engine import (
    Candle,
    MarketDataAdapterMetadata,
    latest_close,
    validate_market_data_adapter,
)


class ExampleAdapter:
    metadata = MarketDataAdapterMetadata(
        name="example",
        version="0.1.0",
        description="test adapter",
        source="fixture",
    )

    def __init__(self) -> None:
        self._candles = (
            Candle("BTC", "2026-05-01T00:00:00Z", 100, 105, 99, 104, 1),
            Candle("BTC", "2026-05-01T00:05:00Z", 104, 110, 103, 108, 1),
        )

    def candles(self, symbol: str, limit: int | None = None):
        if symbol.upper() != "BTC":
            return ()
        if limit is not None:
            if limit <= 0:
                raise ValueError("limit must be positive")
            return self._candles[-limit:]
        return self._candles

    def latest(self, symbol: str):
        candles = self.candles(symbol)
        if not candles:
            raise KeyError(symbol)
        return candles[-1]


class SecretAdapter(ExampleAdapter):
    metadata = MarketDataAdapterMetadata(
        name="secret",
        version="0.1.0",
        description="bad public adapter",
        source="private",
        requires_secrets=True,
    )


def test_market_data_adapter_validation_accepts_public_adapter() -> None:
    validate_market_data_adapter(ExampleAdapter())


def test_market_data_adapter_validation_rejects_secret_adapter() -> None:
    with pytest.raises(ValueError, match="must not require secrets"):
        validate_market_data_adapter(SecretAdapter())


def test_latest_close_validates_adapter_and_returns_close() -> None:
    assert latest_close(ExampleAdapter(), "BTC") == 108


def test_market_data_adapter_example_runs_from_repo_root() -> None:
    import json
    import subprocess
    import sys

    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/market-data-adapter/run.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    payload = json.loads(result.stdout)
    assert payload["mode"] == "paper"
    assert payload["adapter"]["name"] == "memory-candles"
    assert payload["adapter"]["requires_secrets"] is False
    assert payload["latest_close"] == 40550
    assert payload["proposed"] is True
    assert payload["allowed"] is True
    assert payload["decisions"][0]["source"] == "market-adapter:memory-candles"
