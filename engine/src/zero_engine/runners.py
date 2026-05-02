from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from zero_engine.market import MarketDataAdapter
from zero_engine.models import OrderIntent, Side
from zero_engine.strategy import StrategySignal, candle_move_pct


@dataclass(frozen=True)
class StrategyRunnerMetadata:
    name: str
    version: str
    description: str
    author: str = "ZERO contributors"
    paper_only: bool = True


class StrategyRunner(Protocol):
    metadata: StrategyRunnerMetadata

    def propose(self, market: MarketDataAdapter, symbol: str) -> StrategySignal | None:
        """Return a proposed paper signal or None when there is no setup."""


@dataclass(frozen=True)
class DeclarativeStrategyRunner:
    metadata: StrategyRunnerMetadata
    condition: str
    side: Side
    quantity: float
    confidence: float
    min_move_pct: float = 0.0
    symbol: str | None = None
    reason: str | None = None

    @classmethod
    def from_mapping(cls, data: dict[str, Any]) -> "DeclarativeStrategyRunner":
        metadata = StrategyRunnerMetadata(
            name=str(data["name"]),
            version=str(data.get("version", "0.1.0")),
            description=str(data.get("description", "Declarative paper strategy.")),
            author=str(data.get("author", "ZERO contributors")),
            paper_only=parse_bool(data.get("paper_only", True), "paper_only"),
        )
        condition = data.get("condition", {})
        if isinstance(condition, str):
            condition_name = condition
            min_move_pct = float(data.get("min_move_pct", 0.0))
        elif isinstance(condition, dict):
            condition_name = str(condition.get("type", "close_above_open"))
            min_move_pct = float(condition.get("min_move_pct", data.get("min_move_pct", 0.0)))
        else:
            raise ValueError("strategy condition must be a string or object")

        return cls(
            metadata=metadata,
            condition=condition_name,
            side=Side(str(data.get("side", "buy")).lower()),
            quantity=float(data["quantity"]),
            confidence=float(data["confidence"]),
            min_move_pct=min_move_pct,
            symbol=str(data["symbol"]).upper() if data.get("symbol") else None,
            reason=str(data["reason"]) if data.get("reason") else None,
        )

    def propose(self, market: MarketDataAdapter, symbol: str) -> StrategySignal | None:
        target_symbol = self.symbol or symbol.upper()
        if target_symbol != symbol.upper():
            return None
        latest = market.latest(target_symbol)
        move_pct = candle_move_pct(latest)
        if self.condition == "close_above_open":
            if move_pct < self.min_move_pct:
                return None
        elif self.condition == "close_below_open":
            if -move_pct < self.min_move_pct:
                return None
        else:
            raise ValueError(f"unsupported declarative strategy condition: {self.condition}")

        return StrategySignal(
            symbol=latest.symbol,
            side=self.side,
            confidence=self.confidence,
            quantity=self.quantity,
            reason=self.reason
            or (
                f"{self.condition} move={move_pct:.2%}; "
                f"threshold={self.min_move_pct:.2%}"
            ),
        )


def load_strategy_runner(path: str | Path) -> DeclarativeStrategyRunner:
    strategy_path = Path(path)
    suffix = strategy_path.suffix.lower()
    body = strategy_path.read_text(encoding="utf-8")
    if suffix == ".json":
        data = json.loads(body)
    elif suffix in {".yaml", ".yml"}:
        data = parse_declarative_yaml(body)
    else:
        raise ValueError("strategy runner files must use .json, .yaml, or .yml")
    if not isinstance(data, dict):
        raise ValueError("strategy runner file must contain an object")
    runner = DeclarativeStrategyRunner.from_mapping(data)
    validate_strategy_runner(runner)
    return runner


def validate_strategy_runner(runner: StrategyRunner) -> None:
    metadata = runner.metadata
    if not metadata.name.strip():
        raise ValueError("strategy runner metadata.name is required")
    if not metadata.version.strip():
        raise ValueError("strategy runner metadata.version is required")
    if not metadata.paper_only:
        raise ValueError("public strategy runners must be paper_only")
    if isinstance(runner, DeclarativeStrategyRunner):
        if runner.condition not in {"close_above_open", "close_below_open"}:
            raise ValueError(f"unsupported declarative strategy condition: {runner.condition}")
        if runner.quantity <= 0:
            raise ValueError("strategy runner quantity must be positive")
        if not 0 <= runner.confidence <= 1:
            raise ValueError("strategy runner confidence must be between 0 and 1")
        if runner.min_move_pct < 0:
            raise ValueError("strategy runner min_move_pct must be >= 0")


def propose_runner_order(
    runner: StrategyRunner,
    market: MarketDataAdapter,
    symbol: str,
) -> OrderIntent | None:
    validate_strategy_runner(runner)
    signal = runner.propose(market, symbol)
    if signal is None:
        return None
    return signal.to_order(market.latest(signal.symbol).close)


def assert_runner_conformance(
    runner: StrategyRunner,
    market: MarketDataAdapter,
    symbol: str,
) -> dict[str, Any]:
    validate_strategy_runner(runner)
    order = propose_runner_order(runner, market, symbol)
    return {
        "schema_version": "zero.strategy_runner.conformance.v1",
        "runner": {
            "name": runner.metadata.name,
            "version": runner.metadata.version,
            "paper_only": runner.metadata.paper_only,
        },
        "symbol": symbol.upper(),
        "proposed": order is not None,
        "order": order_to_dict(order) if order is not None else None,
    }


def order_to_dict(order: OrderIntent) -> dict[str, Any]:
    return {
        "symbol": order.symbol,
        "side": order.side.value,
        "quantity": order.quantity,
        "price": order.price,
        "confidence": order.confidence,
        "reduce_only": order.reduce_only,
        "notional_usd": round(order.notional_usd, 2),
    }


def parse_declarative_yaml(body: str) -> dict[str, Any]:
    """Parse ZERO's small dependency-free declarative strategy YAML subset."""

    root: dict[str, Any] = {}
    current_parent: dict[str, Any] | None = None
    current_parent_key: str | None = None
    for line_number, raw_line in enumerate(body.splitlines(), start=1):
        line = raw_line.split("#", 1)[0].rstrip()
        if not line.strip():
            continue
        indent = len(line) - len(line.lstrip(" "))
        if indent not in {0, 2}:
            raise ValueError(f"unsupported YAML indentation on line {line_number}")
        key, value = parse_yaml_key_value(line.strip(), line_number)
        if indent == 0:
            if value == "":
                child: dict[str, Any] = {}
                root[key] = child
                current_parent = child
                current_parent_key = key
            else:
                root[key] = parse_yaml_scalar(value)
                current_parent = None
                current_parent_key = None
        else:
            if current_parent is None or current_parent_key is None:
                raise ValueError(f"nested YAML value without parent on line {line_number}")
            current_parent[key] = parse_yaml_scalar(value)
    return root


def parse_yaml_key_value(line: str, line_number: int) -> tuple[str, str]:
    if ":" not in line:
        raise ValueError(f"expected key: value on YAML line {line_number}")
    key, value = line.split(":", 1)
    key = key.strip()
    if not key:
        raise ValueError(f"empty YAML key on line {line_number}")
    return key, value.strip()


def parse_yaml_scalar(value: str) -> Any:
    if value == "":
        return ""
    lowered = value.lower()
    if lowered == "true":
        return True
    if lowered == "false":
        return False
    if lowered in {"null", "none"}:
        return None
    if (value.startswith('"') and value.endswith('"')) or (
        value.startswith("'") and value.endswith("'")
    ):
        return value[1:-1]
    try:
        if any(marker in value for marker in (".", "e", "E")):
            return float(value)
        return int(value)
    except ValueError:
        return value


def parse_bool(value: Any, field_name: str) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        lowered = value.strip().lower()
        if lowered == "true":
            return True
        if lowered == "false":
            return False
    raise ValueError(f"{field_name} must be true or false")
