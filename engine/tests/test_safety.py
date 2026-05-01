from zero_engine.models import OrderIntent, Position, RiskLimits, Side
from zero_engine.safety import evaluate_order, projected_position


def test_low_confidence_rejected() -> None:
    decision = evaluate_order(
        OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.2),
        RiskLimits(min_confidence=0.7),
    )

    assert not decision.allowed
    assert decision.reason == "confidence below minimum"


def test_reduce_only_is_allowed_even_with_low_confidence() -> None:
    decision = evaluate_order(
        OrderIntent("BTC", Side.SELL, quantity=0.01, price=40_000, confidence=0.0, reduce_only=True),
        RiskLimits(min_confidence=0.7),
    )

    assert decision.allowed


def test_projected_position_averages_same_direction_entries() -> None:
    current = Position("BTC", quantity=1, avg_price=100)
    projected = projected_position(
        OrderIntent("BTC", Side.BUY, quantity=1, price=200, confidence=1),
        current,
    )

    assert projected.quantity == 2
    assert projected.avg_price == 150

