from __future__ import annotations

from datetime import UTC, datetime, timedelta

from zero_engine.models import Position
from zero_engine.reconciliation import (
    local_account_positions,
    parse_hyperliquid_account,
    reconcile_positions,
)


FIXED_DT = datetime(2026, 5, 1, tzinfo=UTC)
USER = "0x0000000000000000000000000000000000000000"


def clearinghouse_state(*, symbol: str = "BTC", size: str = "0.01", entry: str = "50000") -> dict:
    return {
        "marginSummary": {"accountValue": "10000.5", "totalMarginUsed": "25"},
        "withdrawable": "9975.5",
        "assetPositions": [
            {
                "position": {
                    "coin": symbol,
                    "szi": size,
                    "entryPx": entry,
                    "positionValue": "500",
                    "unrealizedPnl": "10.5",
                    "marginUsed": "25",
                }
            }
        ],
    }


def test_parse_hyperliquid_account_normalizes_positions_and_orders() -> None:
    snapshot = parse_hyperliquid_account(
        user=USER,
        clearinghouse_state=clearinghouse_state(),
        open_orders=[{"coin": "BTC", "oid": 123}],
        as_of=FIXED_DT,
    )

    assert snapshot.account_value == 10000.5
    assert snapshot.margin_used == 25.0
    assert snapshot.withdrawable == 9975.5
    assert snapshot.positions[0].symbol == "BTC"
    assert snapshot.positions[0].quantity == 0.01
    assert snapshot.open_orders == ({"coin": "BTC", "oid": 123},)
    assert snapshot.to_dict()["user"] == "0x0000...0000"


def test_reconciliation_passes_when_local_and_exchange_positions_match() -> None:
    local = local_account_positions({"BTC": Position("BTC", quantity=0.01, avg_price=50_000)})
    exchange = parse_hyperliquid_account(
        user=USER,
        clearinghouse_state=clearinghouse_state(),
        as_of=FIXED_DT,
    )

    report = reconcile_positions(local_positions=local, exchange_snapshot=exchange, as_of=FIXED_DT)

    assert report.status == "ok"
    assert report.risk_increasing_allowed is True
    assert report.drifts == ()


def test_reconciliation_blocks_stale_exchange_state() -> None:
    exchange = parse_hyperliquid_account(
        user=USER,
        clearinghouse_state={"assetPositions": []},
        as_of=FIXED_DT - timedelta(seconds=11),
    )

    report = reconcile_positions(
        local_positions=(),
        exchange_snapshot=exchange,
        as_of=FIXED_DT,
        stale_after_s=10,
    )

    assert report.status == "stale_data"
    assert report.risk_increasing_allowed is False
    assert report.drifts[0].code == "stale_data"


def test_reconciliation_classifies_exchange_position_missing_from_local_as_lag() -> None:
    exchange = parse_hyperliquid_account(
        user=USER,
        clearinghouse_state=clearinghouse_state(),
        as_of=FIXED_DT,
    )

    report = reconcile_positions(local_positions=(), exchange_snapshot=exchange, as_of=FIXED_DT)

    assert report.status == "local_lag"
    assert report.risk_increasing_allowed is False
    assert report.drifts[0].code == "local_lag"


def test_reconciliation_classifies_local_position_missing_from_exchange_as_rejection() -> None:
    local = local_account_positions({"BTC": Position("BTC", quantity=0.01, avg_price=50_000)})
    exchange = parse_hyperliquid_account(
        user=USER,
        clearinghouse_state={"assetPositions": []},
        as_of=FIXED_DT,
    )

    report = reconcile_positions(local_positions=local, exchange_snapshot=exchange, as_of=FIXED_DT)

    assert report.status == "exchange_rejection"
    assert report.risk_increasing_allowed is False
    assert report.drifts[0].code == "exchange_rejection"


def test_reconciliation_blocks_critical_size_mismatch() -> None:
    local = local_account_positions({"BTC": Position("BTC", quantity=0.02, avg_price=50_000)})
    exchange = parse_hyperliquid_account(
        user=USER,
        clearinghouse_state=clearinghouse_state(size="0.01"),
        as_of=FIXED_DT,
    )

    report = reconcile_positions(local_positions=local, exchange_snapshot=exchange, as_of=FIXED_DT)

    assert report.status == "critical_mismatch"
    assert report.risk_increasing_allowed is False
    assert report.drifts[0].code == "critical_mismatch"
