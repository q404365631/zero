from __future__ import annotations

from zero_engine.models import OrderIntent, RiskLimits, Side
from zero_engine.paper import PaperEngine


def main() -> None:
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=500, min_confidence=0.7))
    orders = [
        OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.82),
        OrderIntent("ETH", Side.BUY, quantity=1.0, price=3_000, confidence=0.91),
        OrderIntent("BTC", Side.SELL, quantity=0.005, price=40_500, confidence=0.2, reduce_only=True),
    ]

    for order in orders:
        decision = engine.submit(order)
        print(f"{order.symbol} {order.side.value} ${order.notional_usd:.2f}: {decision.reason}")

    print(f"fills={len(engine.fills)} rejections={len(engine.rejections)}")


if __name__ == "__main__":
    main()

