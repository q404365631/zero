from pathlib import Path
import sys

import pytest

from zero_engine import (
    Candle,
    MarketDataAdapterMetadata,
    latest_close,
    validate_market_data_adapter,
)


REPO_ROOT = Path(__file__).resolve().parents[2]
EXAMPLE_ADAPTER_DIR = REPO_ROOT / "examples" / "market-data-adapter"
sys.path.insert(0, str(EXAMPLE_ADAPTER_DIR))

from adapter import example_adapter  # noqa: E402


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


def test_market_data_adapter_fixture_returns_chronological_candles() -> None:
    adapter = example_adapter()

    candles = adapter.candles("btc")

    assert adapter.metadata.name == "fixture-candles"
    assert adapter.metadata.requires_secrets is False
    assert [candle.ts for candle in candles] == [
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:05:00Z",
    ]
    assert [candle.close for candle in candles] == [40050, 40550]


def test_market_data_adapter_fixture_validates_limits_and_unknown_symbols() -> None:
    adapter = example_adapter()

    assert adapter.candles("BTC", limit=1)[0].close == 40550
    with pytest.raises(ValueError, match="limit must be positive"):
        adapter.candles("BTC", limit=0)
    with pytest.raises(KeyError, match="no candles for SOL"):
        adapter.latest("SOL")


def test_market_data_adapter_example_runs_from_repo_root() -> None:
    import json
    import subprocess
    import sys

    result = subprocess.run(
        [sys.executable, "examples/market-data-adapter/run.py"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )

    payload = json.loads(result.stdout)
    assert payload["mode"] == "paper"
    assert payload["adapter"]["name"] == "fixture-candles"
    assert payload["adapter"]["requires_secrets"] is False
    assert payload["adapter"]["source"] == "local-jsonl-fixture"
    assert payload["latest_close"] == 40550
    assert payload["proposed"] is True
    assert payload["allowed"] is True
    assert payload["decisions"][0]["source"] == "market-adapter:fixture-candles"
