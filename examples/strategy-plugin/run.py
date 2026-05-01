from __future__ import annotations

import json
from pathlib import Path

from plugin import CloseStrengthPlugin
from zero_engine import JsonlCandleAdapter, PaperEngine, RiskLimits, propose_plugin_order

PAPER_TS = 1777646400.0


def main() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    market = JsonlCandleAdapter(repo_root / "examples/paper-trading/candles.jsonl")
    plugin = CloseStrengthPlugin()
    engine = PaperEngine(
        limits=RiskLimits(max_notional_usd=500, min_confidence=0.7),
        clock=lambda: PAPER_TS,
    )

    order = propose_plugin_order(plugin, market, "BTC")
    decision = (
        engine.submit(order, source=f"strategy-plugin:{plugin.metadata.name}")
        if order is not None
        else None
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
