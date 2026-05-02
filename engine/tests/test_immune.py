from __future__ import annotations

from datetime import UTC, datetime

from zero_engine.immune import build_immune_report
from zero_engine.live import LiveExecutionPolicy, LiveExecutor, RecordingExchangeAdapter
from zero_engine.models import OrderIntent, Position, Side
from zero_engine.reconciliation import ReconciliationReport


FIXED = datetime(2026, 5, 1, tzinfo=UTC)


def reconciled() -> ReconciliationReport:
    return ReconciliationReport(
        status="ok",
        risk_increasing_allowed=True,
        reason="local runtime and Hyperliquid account state are reconciled",
        as_of=FIXED,
    )


def intent() -> OrderIntent:
    return OrderIntent("BTC", Side.BUY, quantity=0.01, price=50_000, confidence=0.9)


def test_immune_report_blocks_live_risk_without_executor_or_reconciliation() -> None:
    report = build_immune_report(generated_at="2026-05-01T00:00:00Z")
    payload = report.to_dict()
    breakers = {breaker["name"]: breaker for breaker in payload["breakers"]}

    assert payload["schema_version"] == "zero.immune.v1"
    assert payload["risk_increasing_allowed"] is False
    assert breakers["dead_man"]["blocks_risk"] is True
    assert breakers["reconciliation"]["blocks_risk"] is True
    assert breakers["stale_market_data"]["status"] == "closed"


def test_immune_report_opens_on_pause_kill_loss_and_velocity() -> None:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(
        adapter=adapter,
        enabled=True,
        policy=LiveExecutionPolicy(max_orders_per_minute=1, max_daily_loss_usd=10),
    )
    executor.heartbeat()
    executor.submit(intent(), idempotency_key="accepted")
    executor.pause()
    executor.kill()
    executor.daily_loss_usd = 10

    report = build_immune_report(
        generated_at="2026-05-01T00:00:00Z",
        live_executor=executor,
        reconciliation=reconciled(),
    ).to_dict()
    breakers = {breaker["name"]: breaker for breaker in report["breakers"]}

    assert report["risk_increasing_allowed"] is False
    assert breakers["operator_pause"]["status"] == "open"
    assert breakers["kill_switch"]["status"] == "open"
    assert breakers["daily_loss"]["status"] == "open"
    assert breakers["order_velocity"]["status"] == "open"


def test_immune_report_allows_risk_when_required_breakers_are_closed() -> None:
    executor = LiveExecutor(adapter=RecordingExchangeAdapter(), enabled=True)
    executor.heartbeat()

    report = build_immune_report(
        generated_at="2026-05-01T00:00:00Z",
        live_executor=executor,
        market_data_age_s=0.1,
        market_data_stale_after_s=2.0,
        reconciliation=reconciled(),
        positions={"BTC": Position("BTC", quantity=0.01, avg_price=50_000)},
        max_exposure_usd=2_500,
    ).to_dict()

    assert report["risk_increasing_allowed"] is True
    assert report["summary"]["open"] == 0
