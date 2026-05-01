from __future__ import annotations

from dataclasses import dataclass, field
from time import time
from typing import Callable

from zero_engine.journal import DecisionJournal
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


@dataclass(frozen=True)
class DecisionRecord:
    intent: OrderIntent
    decision: RiskDecision
    as_of: float
    source: str

    def to_dict(self) -> dict:
        return {
            "as_of": self.as_of,
            "source": self.source,
            "symbol": self.intent.symbol,
            "side": self.intent.side.value,
            "quantity": self.intent.quantity,
            "price": self.intent.price,
            "notional_usd": round(self.intent.notional_usd, 2),
            "confidence": self.intent.confidence,
            "reduce_only": self.intent.reduce_only,
            "allowed": self.decision.allowed,
            "reason": self.decision.reason,
        }


@dataclass
class PaperEngine:
    limits: RiskLimits = field(default_factory=RiskLimits)
    clock: Callable[[], float] = time
    positions: dict[str, Position] = field(default_factory=dict)
    fills: list[Fill] = field(default_factory=list)
    rejections: list[tuple[OrderIntent, RiskDecision]] = field(default_factory=list)
    decisions: list[DecisionRecord] = field(default_factory=list)
    journal: DecisionJournal | None = None

    def submit(self, intent: OrderIntent, source: str = "manual") -> RiskDecision:
        current = self.positions.get(intent.symbol)
        decision = evaluate_order(intent, self.limits, current)
        decided_at = self.clock()
        record = DecisionRecord(
            intent=intent,
            decision=decision,
            as_of=decided_at,
            source=source,
        )
        self.decisions.append(record)
        if self.journal is not None:
            self.journal.append(record.to_dict())
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
                as_of=decided_at,
            )
        )
        return decision
