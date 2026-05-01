from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

from zero_engine.market import MarketDataAdapter
from zero_engine.models import OrderIntent
from zero_engine.strategy import StrategySignal


@dataclass(frozen=True)
class StrategyPluginMetadata:
    name: str
    version: str
    description: str
    author: str = "ZERO contributors"
    paper_only: bool = True


class StrategyPlugin(Protocol):
    metadata: StrategyPluginMetadata

    def propose(self, market: MarketDataAdapter, symbol: str) -> StrategySignal | None:
        """Return a paper signal or None when there is no setup."""


def validate_strategy_plugin(plugin: StrategyPlugin) -> None:
    metadata = plugin.metadata
    if not metadata.name.strip():
        raise ValueError("strategy plugin metadata.name is required")
    if not metadata.version.strip():
        raise ValueError("strategy plugin metadata.version is required")
    if not metadata.paper_only:
        raise ValueError("public strategy plugins must be paper_only")


def propose_plugin_order(
    plugin: StrategyPlugin,
    market: MarketDataAdapter,
    symbol: str,
) -> OrderIntent | None:
    validate_strategy_plugin(plugin)
    signal = plugin.propose(market, symbol)
    if signal is None:
        return None
    return signal.to_order(market.latest(symbol).close)
