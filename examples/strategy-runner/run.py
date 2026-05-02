#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

from zero_engine import (
    JsonlCandleAdapter,
    PaperEngine,
    assert_runner_conformance,
    load_strategy_runner,
    propose_runner_order,
)


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    runner = load_strategy_runner(repo_root / "examples/strategy-runner/close-strength.yaml")
    market = JsonlCandleAdapter(repo_root / "examples/paper-trading/candles.jsonl")
    order = propose_runner_order(runner, market, "BTC")
    engine = PaperEngine()
    decision = engine.submit(order, source=f"strategy-runner:{runner.metadata.name}") if order else None
    packet = assert_runner_conformance(runner, market, "BTC")
    print(
        json.dumps(
            {
                "mode": "paper",
                "runner": {
                    "name": runner.metadata.name,
                    "paper_only": runner.metadata.paper_only,
                    "version": runner.metadata.version,
                },
                "conformance": packet,
                "proposed": order is not None,
                "allowed": decision.allowed if decision is not None else False,
                "reason": decision.reason if decision is not None else "no setup",
                "fills": len(engine.fills),
                "rejections": len(engine.rejections),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
