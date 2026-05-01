from __future__ import annotations

import json
from pathlib import Path

from zero_engine import JsonlCandleAdapter, PaperEngine, load_scenario


def main() -> None:
    base_dir = Path(__file__).parent
    scenario = load_scenario(base_dir / "scenario.json")
    market = JsonlCandleAdapter(base_dir / "candles.jsonl")
    engine = PaperEngine(limits=scenario.limits)

    decisions = []
    for order in scenario.orders:
        decision = engine.submit(order)
        decisions.append(
            {
                "symbol": order.symbol,
                "side": order.side.value,
                "notional_usd": round(order.notional_usd, 2),
                "allowed": decision.allowed,
                "reason": decision.reason,
            }
        )

    print(
        json.dumps(
            {
                "scenario": scenario.name,
                "mode": scenario.mode,
                "market": {
                    symbol: {
                        "as_of": market.latest(symbol).ts,
                        "last": market.latest(symbol).close,
                    }
                    for symbol in sorted({order.symbol for order in scenario.orders})
                },
                "fills": len(engine.fills),
                "rejections": len(engine.rejections),
                "positions": {
                    symbol: {
                        "quantity": position.quantity,
                        "avg_price": position.avg_price,
                        "notional_usd": position.notional_usd,
                    }
                    for symbol, position in sorted(engine.positions.items())
                },
                "decisions": decisions,
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
