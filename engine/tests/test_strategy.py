from pathlib import Path

from zero_engine import JsonlCandleAdapter, MomentumStrategy, PaperEngine, RiskLimits, propose_order


def _fixture_market() -> JsonlCandleAdapter:
    return JsonlCandleAdapter(
        Path(__file__).resolve().parents[2] / "examples/paper-trading/candles.jsonl"
    )


def test_momentum_strategy_proposes_order_from_fixture() -> None:
    market = _fixture_market()
    strategy = MomentumStrategy(min_move_pct=0.01, quantity=0.01, confidence=0.8)

    order = propose_order(strategy, market, "BTC")

    assert order is not None
    assert order.symbol == "BTC"
    assert order.price == 40500
    assert order.confidence == 0.8


def test_momentum_strategy_returns_none_below_threshold() -> None:
    market = _fixture_market()
    strategy = MomentumStrategy(min_move_pct=0.10)

    assert propose_order(strategy, market, "BTC") is None


def test_strategy_proposal_still_goes_through_safety_gate() -> None:
    market = _fixture_market()
    strategy = MomentumStrategy(min_move_pct=0.01, quantity=1, confidence=0.8)
    order = propose_order(strategy, market, "BTC")
    assert order is not None

    engine = PaperEngine(limits=RiskLimits(max_notional_usd=500))
    decision = engine.submit(order, source="strategy:test")

    assert not decision.allowed
    assert decision.reason == "order notional exceeds limit"
    assert engine.decisions[0].to_dict()["source"] == "strategy:test"
