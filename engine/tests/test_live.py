from __future__ import annotations

from zero_engine.live import LiveExecutionPolicy, LiveExecutor, RecordingExchangeAdapter
from zero_engine.live_certification import run_live_certification
from zero_engine.models import OrderIntent, Position, Side


class FailingDeadManAdapter(RecordingExchangeAdapter):
    def schedule_cancel(self, timeout_s: float) -> dict[str, object]:
        super().schedule_cancel(timeout_s)
        return {"ok": False, "error": "exchange rejected scheduleCancel"}


class FailingPlaceOrderAdapter(RecordingExchangeAdapter):
    attempts = 0

    def place_order(self, intent: OrderIntent, *, cloid: str) -> dict[str, object]:
        self.attempts += 1
        raise RuntimeError("exchange unavailable")


def intent(*, size: float = 0.01, reduce_only: bool = False) -> OrderIntent:
    return OrderIntent(
        symbol="BTC",
        side=Side.BUY,
        quantity=size,
        price=50_000,
        confidence=0.9,
        reduce_only=reduce_only,
    )


def test_live_executor_requires_heartbeat_before_risk_increase() -> None:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)

    record = executor.submit(intent(), idempotency_key="live-1")

    assert record.accepted is False
    assert record.reason == "dead-man switch expired"
    assert adapter.placed == []


def test_live_executor_only_arms_heartbeat_after_exchange_dead_man_ok() -> None:
    adapter = FailingDeadManAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)

    heartbeat = executor.heartbeat()
    record = executor.submit(intent(), idempotency_key="after-failed-heartbeat")

    assert heartbeat["ok"] is False
    assert record.accepted is False
    assert record.reason == "dead-man switch expired"
    assert adapter.placed == []


def test_live_executor_submits_once_per_idempotency_key() -> None:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()

    first = executor.submit(intent(), idempotency_key="live-same")
    second = executor.submit(intent(size=0.02), idempotency_key="live-same")

    assert first.accepted is True
    assert first is second
    assert len(adapter.placed) == 1
    assert adapter.placed[0]["size"] == 0.01
    assert adapter.placed[0]["cloid"].startswith("0x")
    assert len(adapter.placed[0]["cloid"]) == 34


def test_live_executor_exchange_submit_failure_is_auditable_and_not_retried() -> None:
    adapter = FailingPlaceOrderAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()

    record = executor.submit(intent(), idempotency_key="exchange-down")
    duplicate = executor.submit(intent(size=0.02), idempotency_key="exchange-down")

    assert record.accepted is False
    assert record.status == "exchange_error"
    assert record.reason == "exchange order submit failed: exchange unavailable"
    assert record.exchange_response == {"ok": False, "error": "exchange unavailable"}
    assert duplicate is record
    assert adapter.attempts == 1


def test_live_executor_kill_switch_blocks_new_risk_and_cancels_orders() -> None:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()

    killed = executor.kill()
    record = executor.submit(intent(), idempotency_key="after-kill")

    assert killed["state"] == "killed"
    assert adapter.cancel_count == 1
    assert record.accepted is False
    assert record.reason == "kill switch active"


def test_live_executor_enforces_notional_and_rate_limits() -> None:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(
        adapter=adapter,
        enabled=True,
        policy=LiveExecutionPolicy(max_notional_usd=1_000, max_orders_per_minute=1),
    )
    executor.heartbeat()

    too_large = executor.submit(intent(size=1), idempotency_key="too-large")
    accepted = executor.submit(intent(size=0.01), idempotency_key="accepted")
    rate_limited = executor.submit(intent(size=0.01), idempotency_key="rate-limited")

    assert too_large.accepted is False
    assert too_large.reason == "live order notional exceeds limit"
    assert accepted.accepted is True
    assert rate_limited.accepted is False
    assert rate_limited.reason == "live order rate limit exceeded"


def test_flatten_uses_reduce_only_orders_even_when_paused() -> None:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()
    executor.pause()

    records = executor.flatten(
        {"BTC": Position("BTC", quantity=0.02, avg_price=50_000)},
        {"BTC": 50_100},
        idempotency_prefix="flat",
    )

    assert len(records) == 1
    assert records[0].accepted is True
    assert adapter.placed[0]["reduce_only"] is True
    assert adapter.placed[0]["side"] == "sell"


def test_live_certification_harness_proves_all_required_drills() -> None:
    report = run_live_certification()
    payload = report.to_dict()

    assert payload["schema_version"] == "zero.live_certification.v1"
    assert payload["mode"] == "dry_run"
    assert payload["passed"] is True
    assert payload["live_start_certified"] is True
    assert payload["summary"]["orders_placed_live"] == 0
    drills = {drill["name"]: drill for drill in payload["drills"]}
    assert drills["exchange_submit_outage_fails_closed_without_retry"]["status"] == "pass"
    assert drills["kill_cancels_and_blocks_new_risk"]["status"] == "pass"
    assert drills["reduce_only_flatten_works_while_paused"]["status"] == "pass"
