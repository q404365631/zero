from pathlib import Path

import pytest

from zero_engine import (
    JsonlCandleAdapter,
    PaperEngine,
    RiskLimits,
    StrategyPluginMetadata,
    propose_plugin_order,
    validate_strategy_plugin,
)


class ExamplePlugin:
    metadata = StrategyPluginMetadata(
        name="test-plugin",
        version="0.1.0",
        description="test plugin",
    )

    def propose(self, market, symbol):
        latest = market.latest(symbol)
        from zero_engine import Side, StrategySignal

        return StrategySignal(
            symbol=latest.symbol,
            side=Side.BUY,
            confidence=0.8,
            quantity=0.01,
            reason="test signal",
        )


class NonPublicPlugin(ExamplePlugin):
    metadata = StrategyPluginMetadata(
        name="unsafe",
        version="0.1.0",
        description="not public",
        paper_only=False,
    )


def _fixture_market() -> JsonlCandleAdapter:
    return JsonlCandleAdapter(
        Path(__file__).resolve().parents[2] / "examples/paper-trading/candles.jsonl"
    )


def test_strategy_plugin_proposes_order_through_helper() -> None:
    order = propose_plugin_order(ExamplePlugin(), _fixture_market(), "BTC")

    assert order is not None
    assert order.symbol == "BTC"
    assert order.price == 40500
    assert order.confidence == 0.8


def test_strategy_plugin_still_goes_through_paper_safety() -> None:
    order = propose_plugin_order(ExamplePlugin(), _fixture_market(), "BTC")
    assert order is not None
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=100))

    decision = engine.submit(order, source="strategy-plugin:test-plugin")

    assert not decision.allowed
    assert decision.reason == "order notional exceeds limit"
    assert engine.decisions[0].source == "strategy-plugin:test-plugin"


def test_strategy_plugin_validation_requires_public_paper_only() -> None:
    with pytest.raises(ValueError, match="paper_only"):
        validate_strategy_plugin(NonPublicPlugin())
