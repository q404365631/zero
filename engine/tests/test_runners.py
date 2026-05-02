from pathlib import Path

import pytest

from zero_engine import (
    JsonlCandleAdapter,
    PaperEngine,
    RiskLimits,
    assert_runner_conformance,
    load_strategy_runner,
    propose_runner_order,
)
from zero_engine.runners import parse_declarative_yaml


def _fixture_market() -> JsonlCandleAdapter:
    return JsonlCandleAdapter(
        Path(__file__).resolve().parents[2] / "examples/paper-trading/candles.jsonl"
    )


def test_declarative_yaml_strategy_runner_proposes_order() -> None:
    runner = load_strategy_runner(
        Path(__file__).resolve().parents[2] / "examples/strategy-runner/close-strength.yaml"
    )

    order = propose_runner_order(runner, _fixture_market(), "BTC")

    assert order is not None
    assert order.symbol == "BTC"
    assert order.side.value == "buy"
    assert order.price == 40500
    assert order.confidence == 0.8


def test_declarative_strategy_runner_still_goes_through_safety_gate() -> None:
    runner = load_strategy_runner(
        Path(__file__).resolve().parents[2] / "examples/strategy-runner/close-strength.yaml"
    )
    order = propose_runner_order(runner, _fixture_market(), "BTC")
    assert order is not None

    engine = PaperEngine(limits=RiskLimits(max_notional_usd=100))
    decision = engine.submit(order, source=f"strategy-runner:{runner.metadata.name}")

    assert not decision.allowed
    assert decision.reason == "order notional exceeds limit"
    assert engine.decisions[0].source == "strategy-runner:close-strength-yaml"


def test_strategy_runner_conformance_packet_is_public_and_deterministic() -> None:
    runner = load_strategy_runner(
        Path(__file__).resolve().parents[2] / "examples/strategy-runner/close-strength.yaml"
    )

    packet = assert_runner_conformance(runner, _fixture_market(), "BTC")

    assert packet == {
        "schema_version": "zero.strategy_runner.conformance.v1",
        "runner": {
            "name": "close-strength-yaml",
            "version": "0.1.0",
            "paper_only": True,
        },
        "symbol": "BTC",
        "proposed": True,
        "order": {
            "symbol": "BTC",
            "side": "buy",
            "quantity": 0.01,
            "price": 40500,
            "confidence": 0.8,
            "reduce_only": False,
            "notional_usd": 405.0,
        },
    }


def test_strategy_runner_rejects_non_paper_public_config(tmp_path: Path) -> None:
    path = tmp_path / "unsafe.yaml"
    path.write_text(
        "\n".join(
            [
                "name: unsafe",
                "version: 0.1.0",
                "paper_only: false",
                "symbol: BTC",
                "side: buy",
                "quantity: 0.01",
                "confidence: 0.8",
                "condition:",
                "  type: close_above_open",
            ]
        ),
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="paper_only"):
        load_strategy_runner(path)


def test_strategy_runner_rejects_quoted_false_paper_only(tmp_path: Path) -> None:
    path = tmp_path / "unsafe.yaml"
    path.write_text(
        "\n".join(
            [
                "name: unsafe",
                "version: 0.1.0",
                'paper_only: "false"',
                "symbol: BTC",
                "side: buy",
                "quantity: 0.01",
                "confidence: 0.8",
                "condition:",
                "  type: close_above_open",
            ]
        ),
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="paper_only"):
        load_strategy_runner(path)


def test_strategy_runner_rejects_invalid_declarative_bounds(tmp_path: Path) -> None:
    path = tmp_path / "invalid.yaml"
    path.write_text(
        "\n".join(
            [
                "name: invalid",
                "version: 0.1.0",
                "symbol: BTC",
                "side: buy",
                "quantity: 0.01",
                "confidence: 1.1",
                "condition:",
                "  type: close_above_open",
            ]
        ),
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="confidence"):
        load_strategy_runner(path)


def test_declarative_yaml_subset_rejects_unsupported_shapes() -> None:
    with pytest.raises(ValueError, match="unsupported YAML indentation"):
        parse_declarative_yaml("name: bad\n    version: 0.1.0\n")
