from __future__ import annotations

import json
from datetime import UTC, datetime

from zero_engine.decision import DecisionQuote, build_decision_stack, evaluation_from_stack
from zero_engine.models import RiskLimits


FIXED_DT = datetime(2026, 5, 1, tzinfo=UTC)


def test_decision_stack_exposes_lenses_layers_and_modifiers() -> None:
    stack = build_decision_stack(
        DecisionQuote(symbol="BTC", price=40_500.0, source="paper:fixture"),
        RiskLimits(),
        generated_at=FIXED_DT,
        sample_size=4,
    )

    assert stack["schema_version"] == "zero.decision.stack.v1"
    assert stack["paper_only"] is True
    assert stack["coin"] == "BTC"
    assert [lens["lens"] for lens in stack["lenses"]] == [
        "price_action",
        "risk_capacity",
        "memory_context",
        "operator_liveness",
    ]
    assert [layer["layer"] for layer in stack["layers"]] == [
        "data_freshness",
        "risk_bounds",
        "sample_floor",
        "paper_boundary",
    ]
    assert [modifier["modifier"] for modifier in stack["modifiers"]] == [
        "rejection_first",
        "operator_friction",
    ]
    assert stack["decision"]["allowed_to_execute_live"] is False
    assert stack["decision"]["direction"] == "LONG"


def test_evaluation_from_stack_preserves_cli_contract_fields() -> None:
    stack = build_decision_stack(
        DecisionQuote(symbol="SOL", price=150.0, source="paper:fixture"),
        RiskLimits(),
        generated_at=FIXED_DT,
    )

    evaluation = evaluation_from_stack(stack, trace_id="trace-test")

    assert evaluation["schema_version"] == "zero.decision.evaluation.v1"
    assert evaluation["coin"] == "SOL"
    assert evaluation["price"] == 150.0
    assert evaluation["price_source"] == "paper:fixture"
    assert evaluation["regime"] == "PAPER"
    assert evaluation["data_fresh"] is True
    assert evaluation["trace_id"] == "trace-test"
    assert evaluation["decision_stack"] == stack


def test_decision_stack_is_public_safe() -> None:
    stack = build_decision_stack(
        DecisionQuote(symbol="ETH", price=2_850.0, source="paper:fixture"),
        RiskLimits(),
        generated_at=FIXED_DT,
    )

    body = json.dumps(stack)
    assert "private_key" not in body
    assert "wallet_address" not in body
    assert "exchange_order_id" not in body
    assert "idempotency_key" not in body
