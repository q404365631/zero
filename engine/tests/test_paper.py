from zero_engine.models import OrderIntent, RiskLimits, Side
from zero_engine.paper import PaperEngine


def test_paper_engine_records_fill() -> None:
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=1_000))
    decision = engine.submit(OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9))

    assert decision.allowed
    assert len(engine.fills) == 1
    assert engine.positions["BTC"].quantity == 0.01


def test_paper_engine_records_rejection() -> None:
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=100))
    decision = engine.submit(OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9))

    assert not decision.allowed
    assert len(engine.rejections) == 1
    assert not engine.fills

