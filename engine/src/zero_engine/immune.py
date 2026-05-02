from __future__ import annotations

from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any

from zero_engine.live import LiveExecutor
from zero_engine.models import Position
from zero_engine.reconciliation import ReconciliationReport


SCHEMA_VERSION = "zero.immune.v1"


@dataclass(frozen=True)
class ImmuneBreaker:
    name: str
    status: str
    blocks_risk: bool
    severity: str
    reason: str
    evidence: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "status": self.status,
            "blocks_risk": self.blocks_risk,
            "severity": self.severity,
            "reason": self.reason,
            "evidence": self.evidence,
        }


@dataclass(frozen=True)
class ImmuneReport:
    schema_version: str
    generated_at: str
    mode: str
    risk_increasing_allowed: bool
    summary: dict[str, Any]
    breakers: tuple[ImmuneBreaker, ...]

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "generated_at": self.generated_at,
            "mode": self.mode,
            "risk_increasing_allowed": self.risk_increasing_allowed,
            "summary": self.summary,
            "breakers": [breaker.to_dict() for breaker in self.breakers],
        }


def build_immune_report(
    *,
    generated_at: str | None = None,
    mode: str = "paper",
    live_executor: LiveExecutor | None = None,
    market_data_age_s: float | None = None,
    market_data_stale_after_s: float = 10.0,
    reconciliation: ReconciliationReport | None = None,
    operator_inactivity_age_s: float | None = None,
    operator_inactivity_after_s: float = 3_600.0,
    positions: dict[str, Position] | None = None,
    max_exposure_usd: float = 0.0,
    exchange_error_window_s: float = 60.0,
) -> ImmuneReport:
    """Build ZERO's risk-blocking immune state without side effects."""

    generated_at = generated_at or datetime.now(UTC).isoformat().replace("+00:00", "Z")
    positions = positions or {}
    breakers = (
        stale_market_data_breaker(market_data_age_s, market_data_stale_after_s),
        reconciliation_breaker(reconciliation),
        dead_man_breaker(live_executor),
        operator_pause_breaker(live_executor),
        operator_inactivity_breaker(operator_inactivity_age_s, operator_inactivity_after_s),
        kill_switch_breaker(live_executor),
        daily_loss_breaker(live_executor),
        order_velocity_breaker(live_executor),
        exchange_error_breaker(live_executor, exchange_error_window_s),
        exposure_breaker(positions, max_exposure_usd),
    )
    open_breakers = [breaker for breaker in breakers if breaker.blocks_risk]
    return ImmuneReport(
        schema_version=SCHEMA_VERSION,
        generated_at=generated_at,
        mode=mode,
        risk_increasing_allowed=not open_breakers,
        summary={
            "total": len(breakers),
            "open": len(open_breakers),
            "closed": len([breaker for breaker in breakers if breaker.status == "closed"]),
            "warning": len([breaker for breaker in breakers if breaker.status == "warning"]),
            "risk_blocking": len(open_breakers),
        },
        breakers=breakers,
    )


def stale_market_data_breaker(
    market_data_age_s: float | None,
    stale_after_s: float,
) -> ImmuneBreaker:
    if market_data_age_s is None:
        return ImmuneBreaker(
            name="stale_market_data",
            status="closed",
            blocks_risk=False,
            severity="info",
            reason="market data freshness not required for static paper source",
            evidence={"age_s": None, "stale_after_s": stale_after_s},
        )
    stale = market_data_age_s > stale_after_s
    return ImmuneBreaker(
        name="stale_market_data",
        status="open" if stale else "closed",
        blocks_risk=stale,
        severity="critical" if stale else "info",
        reason="market data stale" if stale else "market data fresh",
        evidence={"age_s": round(market_data_age_s, 3), "stale_after_s": stale_after_s},
    )


def reconciliation_breaker(reconciliation: ReconciliationReport | None) -> ImmuneBreaker:
    if reconciliation is None:
        return ImmuneBreaker(
            name="reconciliation",
            status="open",
            blocks_risk=True,
            severity="critical",
            reason="account reconciliation unavailable",
            evidence={"status": "missing"},
        )
    blocked = not reconciliation.risk_increasing_allowed
    return ImmuneBreaker(
        name="reconciliation",
        status="open" if blocked else "closed",
        blocks_risk=blocked,
        severity="critical" if blocked else "info",
        reason=reconciliation.reason,
        evidence={"status": reconciliation.status, "drifts": len(reconciliation.drifts)},
    )


def dead_man_breaker(live_executor: LiveExecutor | None) -> ImmuneBreaker:
    if live_executor is None:
        return ImmuneBreaker(
            name="dead_man",
            status="open",
            blocks_risk=True,
            severity="critical",
            reason="live executor not configured",
            evidence={"configured": False},
        )
    expired = live_executor.dead_man_expired()
    return ImmuneBreaker(
        name="dead_man",
        status="open" if expired else "closed",
        blocks_risk=expired,
        severity="critical" if expired else "info",
        reason="dead-man switch expired" if expired else "dead-man switch fresh",
        evidence={
            "configured": True,
            "last_heartbeat_at": live_executor.last_heartbeat_at,
            "timeout_s": live_executor.policy.dead_man_timeout_s,
        },
    )


def operator_pause_breaker(live_executor: LiveExecutor | None) -> ImmuneBreaker:
    paused = bool(live_executor and live_executor.paused)
    return ImmuneBreaker(
        name="operator_pause",
        status="open" if paused else "closed",
        blocks_risk=paused,
        severity="warning" if paused else "info",
        reason="operator pause active" if paused else "operator pause inactive",
        evidence={"paused": paused},
    )


def operator_inactivity_breaker(
    inactivity_age_s: float | None,
    inactive_after_s: float,
) -> ImmuneBreaker:
    if inactivity_age_s is None:
        return ImmuneBreaker(
            name="operator_inactivity",
            status="closed",
            blocks_risk=False,
            severity="info",
            reason="operator inactivity not configured",
            evidence={"age_s": None, "inactive_after_s": inactive_after_s},
        )
    inactive = inactivity_age_s > inactive_after_s
    return ImmuneBreaker(
        name="operator_inactivity",
        status="open" if inactive else "closed",
        blocks_risk=inactive,
        severity="critical" if inactive else "info",
        reason="operator inactivity limit exceeded" if inactive else "operator activity fresh",
        evidence={"age_s": round(inactivity_age_s, 3), "inactive_after_s": inactive_after_s},
    )


def kill_switch_breaker(live_executor: LiveExecutor | None) -> ImmuneBreaker:
    killed = bool(live_executor and live_executor.kill_switch_active)
    return ImmuneBreaker(
        name="kill_switch",
        status="open" if killed else "closed",
        blocks_risk=killed,
        severity="critical" if killed else "info",
        reason="kill switch active" if killed else "kill switch inactive",
        evidence={"kill_switch_active": killed},
    )


def daily_loss_breaker(live_executor: LiveExecutor | None) -> ImmuneBreaker:
    if live_executor is None:
        return ImmuneBreaker(
            name="daily_loss",
            status="closed",
            blocks_risk=False,
            severity="info",
            reason="no live daily loss observed",
            evidence={"daily_loss_usd": 0.0},
        )
    blocked = live_executor.daily_loss_usd >= live_executor.policy.max_daily_loss_usd
    return ImmuneBreaker(
        name="daily_loss",
        status="open" if blocked else "closed",
        blocks_risk=blocked,
        severity="critical" if blocked else "info",
        reason="daily loss limit reached" if blocked else "daily loss inside limit",
        evidence={
            "daily_loss_usd": live_executor.daily_loss_usd,
            "max_daily_loss_usd": live_executor.policy.max_daily_loss_usd,
        },
    )


def order_velocity_breaker(live_executor: LiveExecutor | None) -> ImmuneBreaker:
    exhausted = bool(live_executor and live_executor.order_rate_exhausted())
    return ImmuneBreaker(
        name="order_velocity",
        status="open" if exhausted else "closed",
        blocks_risk=exhausted,
        severity="warning" if exhausted else "info",
        reason="live order rate limit exhausted" if exhausted else "live order rate inside limit",
        evidence={
            "orders_last_minute": len(live_executor.order_timestamps) if live_executor else 0,
            "max_orders_per_minute": live_executor.policy.max_orders_per_minute if live_executor else None,
        },
    )


def exchange_error_breaker(
    live_executor: LiveExecutor | None,
    window_s: float,
) -> ImmuneBreaker:
    if live_executor is None:
        return ImmuneBreaker(
            name="exchange_error",
            status="closed",
            blocks_risk=False,
            severity="info",
            reason="no live exchange errors observed",
            evidence={"recent_errors": 0, "window_s": window_s},
        )
    now = live_executor.clock()
    recent = [
        record
        for record in live_executor.records
        if record.status == "exchange_error" and now - record.as_of <= window_s
    ]
    blocked = bool(recent)
    return ImmuneBreaker(
        name="exchange_error",
        status="open" if blocked else "closed",
        blocks_risk=blocked,
        severity="critical" if blocked else "info",
        reason="recent exchange submit error" if blocked else "no recent exchange submit errors",
        evidence={"recent_errors": len(recent), "window_s": window_s},
    )


def exposure_breaker(
    positions: dict[str, Position],
    max_exposure_usd: float,
) -> ImmuneBreaker:
    exposure = sum(abs(position.notional_usd) for position in positions.values())
    blocked = max_exposure_usd > 0 and exposure >= max_exposure_usd
    return ImmuneBreaker(
        name="max_exposure",
        status="open" if blocked else "closed",
        blocks_risk=blocked,
        severity="critical" if blocked else "info",
        reason="max exposure reached" if blocked else "exposure inside limit",
        evidence={
            "exposure_usd": round(exposure, 2),
            "max_exposure_usd": max_exposure_usd,
            "open_positions": len([position for position in positions.values() if position.quantity != 0]),
        },
    )
