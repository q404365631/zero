from pathlib import Path

from zero_engine import Candle, JsonlCandleAdapter


def test_candle_validates_ohlcv_bounds() -> None:
    try:
        Candle("BTC", "2026-05-01T00:00:00Z", open=100, high=99, low=98, close=100, volume=1)
    except ValueError as exc:
        assert str(exc) == "high must be greater than or equal to open, close, and low"
    else:
        raise AssertionError("expected invalid candle to fail")


def test_jsonl_candle_adapter_returns_latest_and_limit() -> None:
    path = Path(__file__).resolve().parents[2] / "examples/paper-trading/candles.jsonl"
    adapter = JsonlCandleAdapter(path)

    btc = adapter.latest("btc")

    assert btc.symbol == "BTC"
    assert btc.close == 40500
    assert len(adapter.candles("BTC", limit=1)) == 1


def test_jsonl_candle_adapter_requires_positive_limit() -> None:
    path = Path(__file__).resolve().parents[2] / "examples/paper-trading/candles.jsonl"
    adapter = JsonlCandleAdapter(path)

    try:
        adapter.candles("BTC", limit=0)
    except ValueError as exc:
        assert str(exc) == "limit must be positive"
    else:
        raise AssertionError("expected invalid limit to fail")
