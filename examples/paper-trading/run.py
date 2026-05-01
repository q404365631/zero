from __future__ import annotations

import json

from zero_engine import OrderIntent, PaperEngine, RiskLimits, Side


def main() -> None:
    engine = PaperEngine(
        limits=RiskLimits(
            max_notional_usd=500,
            max_position_notional_usd=900,
            min_confidence=0.70,
        )
    )
    orders = [
        OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.84),
        OrderIntent("ETH", Side.BUY, quantity=1.0, price=3_000, confidence=0.93),
        OrderIntent("BTC", Side.SELL, quantity=0.005, price=40_500, confidence=0.10, reduce_only=True),
        OrderIntent("SOL", Side.BUY, quantity=10.0, price=140, confidence=0.95),
    ]

    decisions = []
    for order in orders:
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
                "mode": "paper",
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
