from __future__ import annotations

from dataclasses import dataclass, field
from time import time

from zero_engine.models import OrderIntent, Position, RiskLimits
from zero_engine.safety import RiskDecision, evaluate_order, projected_position


@dataclass(frozen=True)
class Fill:
    symbol: str
    side: str
    quantity: float
    price: float
    notional_usd: float
    as_of: float


@dataclass
class PaperEngine:
    limits: RiskLimits = field(default_factory=RiskLimits)
    positions: dict[str, Position] = field(default_factory=dict)
    fills: list[Fill] = field(default_factory=list)
    rejections: list[tuple[OrderIntent, RiskDecision]] = field(default_factory=list)

    def submit(self, intent: OrderIntent) -> RiskDecision:
        current = self.positions.get(intent.symbol)
        decision = evaluate_order(intent, self.limits, current)
        if not decision.allowed:
            self.rejections.append((intent, decision))
            return decision

        self.positions[intent.symbol] = projected_position(intent, current)
        self.fills.append(
            Fill(
                symbol=intent.symbol,
                side=intent.side.value,
                quantity=intent.quantity,
                price=intent.price,
                notional_usd=intent.notional_usd,
                as_of=time(),
            )
        )
        return decision

