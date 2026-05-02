from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any

from zero_engine.models import Position


SCHEMA_ACCOUNT = "zero.hl_account.v1"
SCHEMA_RECONCILIATION = "zero.reconciliation.v1"


@dataclass(frozen=True)
class AccountPosition:
    symbol: str
    quantity: float
    entry_price: float = 0.0
    position_value: float = 0.0
    unrealized_pnl: float = 0.0
    margin_used: float = 0.0

    @property
    def side(self) -> str:
        if self.quantity > 0:
            return "long"
        if self.quantity < 0:
            return "short"
        return "flat"

    def to_dict(self) -> dict[str, Any]:
        return {
            "symbol": self.symbol,
            "side": self.side,
            "quantity": self.quantity,
            "entry_price": self.entry_price,
            "position_value": self.position_value,
            "unrealized_pnl": self.unrealized_pnl,
            "margin_used": self.margin_used,
        }


@dataclass(frozen=True)
class HyperliquidAccountSnapshot:
    user: str
    as_of: datetime
    account_value: float = 0.0
    margin_used: float = 0.0
    withdrawable: float = 0.0
    positions: tuple[AccountPosition, ...] = ()
    open_orders: tuple[dict[str, Any], ...] = ()

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": SCHEMA_ACCOUNT,
            "exchange": "hyperliquid",
            "user": redact_public_id(self.user),
            "as_of": isoformat_utc(self.as_of),
            "account_value": self.account_value,
            "margin_used": self.margin_used,
            "withdrawable": self.withdrawable,
            "positions": [position.to_dict() for position in self.positions],
            "open_orders": [dict(order) for order in self.open_orders],
            "counts": {
                "positions": len(self.positions),
                "open_orders": len(self.open_orders),
            },
        }


@dataclass(frozen=True)
class ReconciliationDrift:
    code: str
    severity: str
    symbol: str | None
    reason: str
    local_quantity: float | None = None
    exchange_quantity: float | None = None

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "code": self.code,
            "severity": self.severity,
            "symbol": self.symbol,
            "reason": self.reason,
        }
        if self.local_quantity is not None:
            payload["local_quantity"] = self.local_quantity
        if self.exchange_quantity is not None:
            payload["exchange_quantity"] = self.exchange_quantity
        return payload


@dataclass(frozen=True)
class ReconciliationReport:
    status: str
    risk_increasing_allowed: bool
    reason: str
    as_of: datetime
    exchange: str = "hyperliquid"
    stale_after_s: float = 10.0
    local_positions: tuple[AccountPosition, ...] = ()
    exchange_positions: tuple[AccountPosition, ...] = ()
    drifts: tuple[ReconciliationDrift, ...] = ()
    account: HyperliquidAccountSnapshot | None = None

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "schema_version": SCHEMA_RECONCILIATION,
            "exchange": self.exchange,
            "status": self.status,
            "risk_increasing_allowed": self.risk_increasing_allowed,
            "reason": self.reason,
            "as_of": isoformat_utc(self.as_of),
            "stale_after_s": self.stale_after_s,
            "local": {
                "positions": [position.to_dict() for position in self.local_positions],
                "open_positions": len(self.local_positions),
            },
            "exchange_state": {
                "positions": [position.to_dict() for position in self.exchange_positions],
                "open_positions": len(self.exchange_positions),
            },
            "drifts": [drift.to_dict() for drift in self.drifts],
        }
        if self.account is not None:
            payload["account"] = self.account.to_dict()
        return payload


def parse_hyperliquid_account(
    *,
    user: str,
    clearinghouse_state: dict[str, Any],
    open_orders: list[dict[str, Any]] | None = None,
    as_of: datetime | None = None,
) -> HyperliquidAccountSnapshot:
    asset_positions = clearinghouse_state.get("assetPositions", [])
    if not isinstance(asset_positions, list):
        raise ValueError("Hyperliquid assetPositions must be a list")

    positions: list[AccountPosition] = []
    for entry in asset_positions:
        if not isinstance(entry, dict):
            continue
        raw_position = entry.get("position", entry)
        if not isinstance(raw_position, dict):
            continue
        symbol = str(raw_position.get("coin") or "").upper()
        if not symbol:
            continue
        quantity = parse_float(raw_position.get("szi", 0), f"{symbol} szi")
        if quantity == 0:
            continue
        positions.append(
            AccountPosition(
                symbol=symbol,
                quantity=quantity,
                entry_price=parse_optional_float(raw_position.get("entryPx")),
                position_value=parse_optional_float(raw_position.get("positionValue")),
                unrealized_pnl=parse_optional_float(raw_position.get("unrealizedPnl")),
                margin_used=parse_optional_float(raw_position.get("marginUsed")),
            )
        )

    orders = tuple(dict(order) for order in (open_orders or []) if isinstance(order, dict))
    margin_summary = object_mapping(clearinghouse_state.get("marginSummary"))
    cross_margin_summary = object_mapping(clearinghouse_state.get("crossMarginSummary"))
    return HyperliquidAccountSnapshot(
        user=user,
        as_of=as_of or datetime.now(UTC),
        account_value=parse_optional_float(
            margin_summary.get("accountValue", cross_margin_summary.get("accountValue"))
        ),
        margin_used=parse_optional_float(
            margin_summary.get("totalMarginUsed", cross_margin_summary.get("totalMarginUsed"))
        ),
        withdrawable=parse_optional_float(clearinghouse_state.get("withdrawable")),
        positions=tuple(sorted(positions, key=lambda position: position.symbol)),
        open_orders=orders,
    )


def local_account_positions(positions: dict[str, Position]) -> tuple[AccountPosition, ...]:
    local_positions = [
        AccountPosition(
            symbol=position.symbol.upper(),
            quantity=position.quantity,
            entry_price=position.avg_price,
            position_value=abs(position.quantity * position.avg_price),
        )
        for position in positions.values()
        if position.quantity != 0
    ]
    return tuple(sorted(local_positions, key=lambda position: position.symbol))


def reconcile_positions(
    *,
    local_positions: tuple[AccountPosition, ...],
    exchange_snapshot: HyperliquidAccountSnapshot | None,
    as_of: datetime | None = None,
    stale_after_s: float = 10.0,
    size_tolerance: float = 1e-9,
    entry_price_tolerance: float = 5.0,
) -> ReconciliationReport:
    now = as_of or datetime.now(UTC)
    if exchange_snapshot is None:
        return ReconciliationReport(
            status="not_configured",
            risk_increasing_allowed=False,
            reason="Hyperliquid account reconciliation is not configured",
            as_of=now,
            stale_after_s=stale_after_s,
            local_positions=local_positions,
        )

    age_s = (now - exchange_snapshot.as_of).total_seconds()
    if age_s > stale_after_s:
        return ReconciliationReport(
            status="stale_data",
            risk_increasing_allowed=False,
            reason=f"Hyperliquid account snapshot is stale ({age_s:.1f}s old)",
            as_of=now,
            stale_after_s=stale_after_s,
            local_positions=local_positions,
            exchange_positions=exchange_snapshot.positions,
            account=exchange_snapshot,
            drifts=(
                ReconciliationDrift(
                    code="stale_data",
                    severity="blocking",
                    symbol=None,
                    reason=f"snapshot age {age_s:.1f}s exceeds {stale_after_s:g}s",
                ),
            ),
        )

    local_by_symbol = {position.symbol: position for position in local_positions}
    exchange_by_symbol = {position.symbol: position for position in exchange_snapshot.positions}
    drifts: list[ReconciliationDrift] = []

    for symbol in sorted(set(local_by_symbol) | set(exchange_by_symbol)):
        local = local_by_symbol.get(symbol)
        exchange = exchange_by_symbol.get(symbol)
        if local is None and exchange is not None:
            drifts.append(
                ReconciliationDrift(
                    code="local_lag",
                    severity="blocking",
                    symbol=symbol,
                    reason="exchange has an open position missing from local runtime state",
                    exchange_quantity=exchange.quantity,
                )
            )
            continue
        if exchange is None and local is not None:
            drifts.append(
                ReconciliationDrift(
                    code="exchange_rejection",
                    severity="blocking",
                    symbol=symbol,
                    reason="local runtime has an open position missing from exchange state",
                    local_quantity=local.quantity,
                )
            )
            continue
        if local is None or exchange is None:
            continue
        if abs(local.quantity - exchange.quantity) > size_tolerance:
            drifts.append(
                ReconciliationDrift(
                    code="critical_mismatch",
                    severity="blocking",
                    symbol=symbol,
                    reason="local and exchange position sizes differ",
                    local_quantity=local.quantity,
                    exchange_quantity=exchange.quantity,
                )
            )
            continue
        if (
            local.entry_price > 0
            and exchange.entry_price > 0
            and abs(local.entry_price - exchange.entry_price) > entry_price_tolerance
        ):
            drifts.append(
                ReconciliationDrift(
                    code="local_lag",
                    severity="blocking",
                    symbol=symbol,
                    reason="local entry price differs from exchange entry price",
                    local_quantity=local.quantity,
                    exchange_quantity=exchange.quantity,
                )
            )

    if not drifts:
        return ReconciliationReport(
            status="ok",
            risk_increasing_allowed=True,
            reason="local runtime and Hyperliquid account state are reconciled",
            as_of=now,
            stale_after_s=stale_after_s,
            local_positions=local_positions,
            exchange_positions=exchange_snapshot.positions,
            account=exchange_snapshot,
        )

    codes = {drift.code for drift in drifts}
    if "critical_mismatch" in codes or ("local_lag" in codes and "exchange_rejection" in codes):
        status = "critical_mismatch"
    elif "local_lag" in codes:
        status = "local_lag"
    elif "exchange_rejection" in codes:
        status = "exchange_rejection"
    else:
        status = "critical_mismatch"

    return ReconciliationReport(
        status=status,
        risk_increasing_allowed=False,
        reason=f"Hyperliquid reconciliation failed: {status}",
        as_of=now,
        stale_after_s=stale_after_s,
        local_positions=local_positions,
        exchange_positions=exchange_snapshot.positions,
        account=exchange_snapshot,
        drifts=tuple(drifts),
    )


def parse_float(value: Any, label: str) -> float:
    try:
        return float(value)
    except (TypeError, ValueError) as exc:
        raise ValueError(f"{label} must be numeric") from exc


def parse_optional_float(value: Any) -> float:
    if value is None or value == "":
        return 0.0
    return parse_float(value, "numeric field")


def object_mapping(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def redact_public_id(value: str) -> str:
    if len(value) <= 12:
        return "<redacted>"
    return f"{value[:6]}...{value[-4:]}"


def isoformat_utc(value: datetime) -> str:
    if value.tzinfo is None:
        value = value.replace(tzinfo=UTC)
    return value.astimezone(UTC).isoformat().replace("+00:00", "Z")
