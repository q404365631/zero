from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any

from zero_engine.models import OrderIntent, Position, RiskLimits, Side
from zero_engine.safety import evaluate_order

STACK_SCHEMA_VERSION = "zero.decision.stack.v1"
EVALUATION_SCHEMA_VERSION = "zero.decision.evaluation.v1"


@dataclass(frozen=True)
class DecisionQuote:
    symbol: str
    price: float
    source: str = "paper:static"


@dataclass(frozen=True)
class DecisionLens:
    lens: str
    family: str
    signal: str
    confidence: float
    weight: float
    evidence: dict[str, Any]
    status: str = "active"
    public_safe: bool = True

    def to_dict(self) -> dict[str, Any]:
        return {
            "lens": self.lens,
            "family": self.family,
            "status": self.status,
            "signal": self.signal,
            "confidence": round(self.confidence, 4),
            "weight": round(self.weight, 4),
            "evidence": self.evidence,
            "public_safe": self.public_safe,
        }


@dataclass(frozen=True)
class DecisionLayer:
    layer: str
    kind: str
    passed: bool
    value: Any
    detail: str
    blocks_entry: bool = True

    def to_dict(self) -> dict[str, Any]:
        return {
            "layer": self.layer,
            "kind": self.kind,
            "passed": self.passed,
            "value": self.value,
            "detail": self.detail,
            "blocks_entry": self.blocks_entry,
        }


@dataclass(frozen=True)
class DecisionModifier:
    modifier: str
    effect: str
    direction: str
    value: float
    reason: str
    bounded: bool = True

    def to_dict(self) -> dict[str, Any]:
        return {
            "modifier": self.modifier,
            "effect": self.effect,
            "direction": self.direction,
            "value": round(self.value, 4),
            "reason": self.reason,
            "bounded": self.bounded,
        }


def isoformat(value: datetime) -> str:
    return value.isoformat().replace("+00:00", "Z")


def build_decision_stack(
    quote: DecisionQuote,
    limits: RiskLimits,
    position: Position | None = None,
    *,
    generated_at: datetime | None = None,
    sample_size: int = 0,
) -> dict[str, Any]:
    generated_at = generated_at or datetime.now(UTC)
    intent = OrderIntent(
        quote.symbol.upper(),
        Side.BUY,
        quantity=1 / quote.price,
        price=quote.price,
        confidence=0.9,
    )
    risk = evaluate_order(intent, limits, position)

    lenses = [
        DecisionLens(
            lens="price_action",
            family="market",
            signal="constructive",
            confidence=0.62,
            weight=0.22,
            evidence={
                "source": quote.source,
                "last_price": round(quote.price, 6),
                "uses_live_exchange_credentials": False,
            },
        ),
        DecisionLens(
            lens="risk_capacity",
            family="risk",
            signal="pass" if risk.allowed else "reject",
            confidence=1.0,
            weight=0.34,
            evidence={
                "paper_exposure_unit": round(intent.notional_usd, 6),
                "reason": risk.reason,
            },
        ),
        DecisionLens(
            lens="memory_context",
            family="memory",
            signal="insufficient_sample" if sample_size < 30 else "calibrated",
            confidence=0.35 if sample_size < 30 else 0.72,
            weight=0.18,
            evidence={
                "public_sample_size": sample_size,
                "requires_more_paper_decisions": sample_size < 30,
            },
        ),
        DecisionLens(
            lens="operator_liveness",
            family="safety",
            signal="ready",
            confidence=0.8,
            weight=0.26,
            evidence={
                "dead_man_switch_required_for_live": True,
                "paper_mode": True,
            },
        ),
    ]
    weighted_confidence = sum(lens.confidence * lens.weight for lens in lenses)
    consensus = int(round(weighted_confidence * 100))

    layers = [
        DecisionLayer(
            layer="data_freshness",
            kind="preflight",
            passed=True,
            value={"source": quote.source},
            detail="price source available for paper evaluation",
        ),
        DecisionLayer(
            layer="risk_bounds",
            kind="risk",
            passed=risk.allowed,
            value={"paper_exposure_unit": round(intent.notional_usd, 6)},
            detail=risk.reason,
        ),
        DecisionLayer(
            layer="sample_floor",
            kind="calibration",
            passed=True,
            value={"public_sample_size": sample_size, "minimum": 30, "met": sample_size >= 30},
            detail=(
                "public sample is below promotion floor; paper evaluation remains non-blocking"
                if sample_size < 30
                else "sample floor met"
            ),
            blocks_entry=False,
        ),
        DecisionLayer(
            layer="paper_boundary",
            kind="custody",
            passed=True,
            value={"live_order_emitted": False},
            detail="evaluation is paper-first and does not submit live orders",
        ),
    ]
    modifiers = [
        DecisionModifier(
            modifier="rejection_first",
            effect="confidence_adjustment",
            direction="down",
            value=0.08 if sample_size < 30 else 0.02,
            reason="insufficient public calibration sample" if sample_size < 30 else "normal caution",
        ),
        DecisionModifier(
            modifier="operator_friction",
            effect="live_mode_gate",
            direction="down",
            value=1.0,
            reason="risk-increasing live action requires explicit local confirmation",
        ),
    ]

    blocking_passed = all(layer.passed for layer in layers if layer.blocks_entry)
    direction = "LONG" if risk.allowed and blocking_passed else "NONE"
    verdict = "PASS" if direction in {"LONG", "SHORT"} else "REJECT"
    if blocking_passed and direction == "NONE":
        verdict = "HOLD"

    payload = {
        "schema_version": STACK_SCHEMA_VERSION,
        "mode": "paper",
        "paper_only": True,
        "coin": intent.symbol,
        "generated_at": isoformat(generated_at),
        "price": {
            "last": quote.price,
            "source": quote.source,
            "uses_live_exchange_credentials": False,
        },
        "lenses": [lens.to_dict() for lens in lenses],
        "layers": [layer.to_dict() for layer in layers],
        "modifiers": [modifier.to_dict() for modifier in modifiers],
        "decision": {
            "verdict": verdict,
            "direction": direction,
            "consensus": consensus if risk.allowed else 0,
            "conviction": round(
                max(
                    0.0,
                    weighted_confidence
                    - sum(m.value for m in modifiers if m.effect == "confidence_adjustment"),
                ),
                4,
            ),
            "allowed_to_execute_live": False,
            "reason": risk.reason,
        },
        "privacy": {
            "contains_exchange_credentials": False,
            "contains_wallet_material": False,
            "contains_venue_order_material": False,
            "contains_private_notes": False,
        },
    }
    assert_decision_stack_safe(payload)
    return payload


def evaluation_from_stack(stack: dict[str, Any], *, trace_id: str | None = None) -> dict[str, Any]:
    decision = stack["decision"]
    payload = {
        "schema_version": EVALUATION_SCHEMA_VERSION,
        "coin": stack["coin"],
        "price": stack["price"]["last"],
        "price_source": stack["price"]["source"],
        "consensus": decision["consensus"],
        "conviction": decision["conviction"],
        "direction": decision["direction"],
        "regime": "PAPER",
        "layers": stack["layers"],
        "lenses": stack["lenses"],
        "modifiers": stack["modifiers"],
        "decision_stack": stack,
        "data_fresh": True,
        "timestamp": stack["generated_at"],
    }
    if trace_id:
        payload["trace_id"] = trace_id
    assert_decision_stack_safe(payload)
    return payload


def assert_decision_stack_safe(payload: dict[str, Any]) -> None:
    body = json.dumps(payload, sort_keys=True).lower()
    forbidden = [
        "private_key",
        "seed phrase",
        "exchange_order_id",
        "wallet_address",
        "idempotency_key",
    ]
    found = [token for token in forbidden if token in body]
    if found:
        raise ValueError("decision stack contains forbidden fields: " + ", ".join(found))


def fixture_stack(*, generated_at: datetime | None = None) -> dict[str, Any]:
    return build_decision_stack(
        DecisionQuote(symbol="BTC", price=40_500.0, source="paper:fixture"),
        RiskLimits(),
        generated_at=generated_at or datetime(2026, 5, 1, tzinfo=UTC),
        sample_size=4,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="ZERO public decision stack fixture")
    parser.add_argument("--coin", default="BTC")
    parser.add_argument("--price", type=float, default=40_500.0)
    parser.add_argument("--source", default="paper:fixture")
    parser.add_argument("--sample-size", type=int, default=4)
    args = parser.parse_args()
    stack = build_decision_stack(
        DecisionQuote(symbol=args.coin, price=args.price, source=args.source),
        RiskLimits(),
        generated_at=datetime(2026, 5, 1, tzinfo=UTC),
        sample_size=args.sample_size,
    )
    print(json.dumps(stack, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
