from __future__ import annotations

import json
from pathlib import Path

from plugin import PaperMomentumPlugin
from zero_engine import JsonlCandleAdapter, PaperEngine, RiskLimits, propose_plugin_order

PAPER_TS = 1777646400.0
SYMBOLS = ("BTC", "ETH")


def main() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    market = JsonlCandleAdapter(repo_root / "examples/paper-trading/candles.jsonl")
    plugin = PaperMomentumPlugin()
    engine = PaperEngine(
        limits=RiskLimits(max_notional_usd=500, min_confidence=0.7),
        clock=lambda: PAPER_TS,
    )

    signals = []
    for symbol in SYMBOLS:
        order = propose_plugin_order(plugin, market, symbol)
        source = f"strategy-plugin:{plugin.metadata.name}"
        decision = (
            engine.submit(order, source=source)
            if order is not None
            else None
        )
        signals.append(
            {
                "symbol": symbol,
                "proposed": order is not None,
                "allowed": decision.allowed if decision else None,
                "reason": decision.reason if decision else "no setup",
                "source": source if decision else None,
            }
        )

    print(
        json.dumps(
            {
                "mode": "paper",
                "plugin": {
                    "name": plugin.metadata.name,
                    "version": plugin.metadata.version,
                    "paper_only": plugin.metadata.paper_only,
                },
                "symbols": list(SYMBOLS),
                "signals": signals,
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
