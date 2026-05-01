from __future__ import annotations

import json
from pathlib import Path

from zero_engine import JsonlCandleAdapter, MomentumStrategy, PaperEngine, RiskLimits, propose_order

PAPER_TS = 1777646400.0


def main() -> None:
    base_dir = Path(__file__).parent
    market = JsonlCandleAdapter(base_dir / "candles.jsonl")
    strategy = MomentumStrategy(min_move_pct=0.01, quantity=0.01, confidence=0.8)
    engine = PaperEngine(
        limits=RiskLimits(max_notional_usd=500, min_confidence=0.7),
        clock=lambda: PAPER_TS,
    )

    order = propose_order(strategy, market, "BTC")
    decision = engine.submit(order, source=f"strategy:{strategy.name}") if order is not None else None

    print(
        json.dumps(
            {
                "mode": "paper",
                "strategy": strategy.name,
                "symbol": "BTC",
                "proposed": order is not None,
                "allowed": decision.allowed if decision else None,
                "reason": decision.reason if decision else "no setup",
                "fills": len(engine.fills),
                "rejections": len(engine.rejections),
                "decisions": [record.to_dict() for record in engine.decisions],
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
