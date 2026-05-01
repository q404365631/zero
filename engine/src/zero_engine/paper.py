from __future__ import annotations

from dataclasses import dataclass, field
from time import time
from typing import Callable

from zero_engine.journal import DecisionJournal
from zero_engine.models import OrderIntent, Position, RiskLimits, Side
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
    idempotency_key: str | None = None

    def to_dict(self) -> dict:
        payload = {
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
        if self.idempotency_key:
            payload["idempotency_key"] = self.idempotency_key
        return payload

    @classmethod
    def from_dict(cls, payload: dict) -> "DecisionRecord":
        intent = OrderIntent(
            symbol=str(payload["symbol"]).upper(),
            side=Side(str(payload["side"]).lower()),
            quantity=float(payload["quantity"]),
            price=float(payload["price"]),
            confidence=float(payload["confidence"]),
            reduce_only=bool(payload.get("reduce_only", False)),
        )
        decision = RiskDecision(
            allowed=bool(payload["allowed"]),
            reason=str(payload["reason"]),
        )
        key = payload.get("idempotency_key")
        return cls(
            intent=intent,
            decision=decision,
            as_of=float(payload["as_of"]),
            source=str(payload.get("source") or "journal"),
            idempotency_key=str(key) if key else None,
        )


@dataclass(frozen=True)
class RecoveryState:
    status: str = "ephemeral"
    source: str = "memory"
    durable: bool = False
    journal_path: str | None = None
    decisions_recovered: int = 0
    fills_recovered: int = 0
    rejections_recovered: int = 0
    positions_recovered: int = 0
    last_decision_ts: float | None = None

    def to_dict(self) -> dict:
        return {
            "status": self.status,
            "source": self.source,
            "durable": self.durable,
            "journal_path": self.journal_path,
            "decisions_recovered": self.decisions_recovered,
            "fills_recovered": self.fills_recovered,
            "rejections_recovered": self.rejections_recovered,
            "positions_recovered": self.positions_recovered,
            "last_decision_ts": self.last_decision_ts,
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
    recovery: RecoveryState = field(default_factory=RecoveryState)

    @classmethod
    def recover_from_journal(
        cls,
        journal: DecisionJournal,
        *,
        limits: RiskLimits | None = None,
        clock: Callable[[], float] = time,
    ) -> "PaperEngine":
        engine = cls(limits=limits or RiskLimits(), clock=clock, journal=journal)
        for payload in journal.read_all():
            engine.replay_decision(DecisionRecord.from_dict(payload))
        engine.recovery = RecoveryState(
            status="recovered",
            source="journal",
            durable=True,
            journal_path=str(journal.path),
            decisions_recovered=len(engine.decisions),
            fills_recovered=len(engine.fills),
            rejections_recovered=len(engine.rejections),
            positions_recovered=len([p for p in engine.positions.values() if p.quantity != 0]),
            last_decision_ts=engine.decisions[-1].as_of if engine.decisions else None,
        )
        return engine

    def replay_decision(self, record: DecisionRecord) -> None:
        self.decisions.append(record)
        if not record.decision.allowed:
            self.rejections.append((record.intent, record.decision))
            return

        current = self.positions.get(record.intent.symbol)
        self.positions[record.intent.symbol] = projected_position(record.intent, current)
        self.fills.append(
            Fill(
                symbol=record.intent.symbol,
                side=record.intent.side.value,
                quantity=record.intent.quantity,
                price=record.intent.price,
                notional_usd=record.intent.notional_usd,
                as_of=record.as_of,
            )
        )

    def submit(
        self,
        intent: OrderIntent,
        source: str = "manual",
        idempotency_key: str | None = None,
    ) -> RiskDecision:
        current = self.positions.get(intent.symbol)
        decision = evaluate_order(intent, self.limits, current)
        decided_at = self.clock()
        record = DecisionRecord(
            intent=intent,
            decision=decision,
            as_of=decided_at,
            source=source,
            idempotency_key=idempotency_key,
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
