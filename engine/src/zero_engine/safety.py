from __future__ import annotations

from dataclasses import dataclass

from zero_engine.models import OrderIntent, Position, RiskLimits, Side


@dataclass(frozen=True)
class RiskDecision:
    allowed: bool
    reason: str


def projected_position(intent: OrderIntent, current: Position | None = None) -> Position:
    current = current or Position(symbol=intent.symbol)
    signed_quantity = intent.quantity if intent.side is Side.BUY else -intent.quantity
    next_quantity = current.quantity + signed_quantity

    if next_quantity == 0:
        return Position(symbol=intent.symbol)

    if current.quantity == 0 or (current.quantity > 0) == (signed_quantity > 0):
        total_cost = current.avg_price * abs(current.quantity) + intent.price * intent.quantity
        avg_price = total_cost / abs(next_quantity)
    else:
        avg_price = current.avg_price if abs(next_quantity) < abs(current.quantity) else intent.price

    return Position(symbol=intent.symbol, quantity=next_quantity, avg_price=avg_price)


def evaluate_order(
    intent: OrderIntent,
    limits: RiskLimits,
    current: Position | None = None,
) -> RiskDecision:
    if intent.reduce_only:
        return RiskDecision(True, "reduce-only orders bypass risk-increasing friction")

    if intent.confidence < limits.min_confidence:
        return RiskDecision(False, "confidence below minimum")

    if intent.notional_usd > limits.max_notional_usd:
        return RiskDecision(False, "order notional exceeds limit")

    projected = projected_position(intent, current)
    if projected.notional_usd > limits.max_position_notional_usd:
        return RiskDecision(False, "projected position exceeds limit")

    return RiskDecision(True, "allowed")

