from __future__ import annotations

import json

from adapter import example_adapter
from zero_engine import MomentumStrategy, PaperEngine, RiskLimits, latest_close, propose_order

PAPER_TS = 1777646400.0


def main() -> None:
    market = example_adapter()
    strategy = MomentumStrategy(min_move_pct=0.01, quantity=0.01, confidence=0.8)
    engine = PaperEngine(
        limits=RiskLimits(max_notional_usd=500, min_confidence=0.7),
        clock=lambda: PAPER_TS,
    )

    order = propose_order(strategy, market, "BTC")
    decision = (
        engine.submit(order, source=f"market-adapter:{market.metadata.name}")
        if order is not None
        else None
    )

    print(
        json.dumps(
            {
                "mode": "paper",
                "adapter": {
                    "name": market.metadata.name,
                    "version": market.metadata.version,
                    "source": market.metadata.source,
                    "requires_secrets": market.metadata.requires_secrets,
                },
                "symbol": "BTC",
                "latest_close": latest_close(market, "BTC"),
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
