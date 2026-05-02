from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from zero_engine.live import LiveExecutionPolicy, LiveExecutor, RecordingExchangeAdapter
from zero_engine.models import OrderIntent, Position, Side


SCHEMA_VERSION = "zero.live_certification.v1"


@dataclass(frozen=True)
class CertificationDrill:
    name: str
    passed: bool
    note: str
    evidence: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "status": "pass" if self.passed else "fail",
            "note": self.note,
            "evidence": self.evidence,
        }


@dataclass(frozen=True)
class LiveCertificationReport:
    schema_version: str
    mode: str
    passed: bool
    live_start_certified: bool
    summary: dict[str, Any]
    drills: tuple[CertificationDrill, ...]
    evidence_requirements: tuple[str, ...]

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "mode": self.mode,
            "passed": self.passed,
            "live_start_certified": self.live_start_certified,
            "summary": self.summary,
            "drills": [drill.to_dict() for drill in self.drills],
            "evidence_requirements": list(self.evidence_requirements),
        }


@dataclass
class ManualClock:
    now: float = 1_777_777_000.0

    def __call__(self) -> float:
        return self.now

    def advance(self, seconds: float) -> None:
        self.now += seconds


class FailingScheduleCancelAdapter(RecordingExchangeAdapter):
    def schedule_cancel(self, timeout_s: float) -> dict[str, Any]:
        super().schedule_cancel(timeout_s)
        return {"ok": False, "error": "exchange rejected scheduleCancel"}


class FailingPlaceOrderAdapter(RecordingExchangeAdapter):
    attempts: int = 0

    def place_order(self, intent: OrderIntent, *, cloid: str) -> dict[str, Any]:
        self.attempts += 1
        raise RuntimeError("exchange unavailable")


def run_live_certification() -> LiveCertificationReport:
    drills = (
        heartbeat_arms_dead_man(),
        risk_increase_requires_heartbeat(),
        idempotent_submit_has_single_exchange_attempt(),
        exchange_submit_outage_fails_closed_without_retry(),
        pause_blocks_entries(),
        reduce_only_flatten_works_while_paused(),
        kill_cancels_and_blocks_new_risk(),
        rejected_exchange_dead_man_blocks_entries(),
        rate_limit_blocks_second_order(),
        daily_loss_limit_blocks_entries(),
    )
    passed = all(drill.passed for drill in drills)
    return LiveCertificationReport(
        schema_version=SCHEMA_VERSION,
        mode="dry_run",
        passed=passed,
        live_start_certified=passed,
        summary={
            "total": len(drills),
            "passed": len([drill for drill in drills if drill.passed]),
            "failed": len([drill for drill in drills if not drill.passed]),
            "exchange": "fake",
            "secrets_required": False,
            "orders_placed_live": 0,
        },
        drills=drills,
        evidence_requirements=(
            "live_preflight packet",
            "hl_reconcile packet",
            "decision journal path and checksum",
            "live certification report",
            "kill-switch path",
            "tiny-capital canary trace IDs when live rehearsal is approved",
        ),
    )


def heartbeat_arms_dead_man() -> CertificationDrill:
    clock = ManualClock()
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True, clock=clock)

    heartbeat = executor.heartbeat()
    record = executor.submit(intent(), idempotency_key="cert-heartbeat")

    passed = bool(heartbeat["ok"]) and record.accepted and adapter.scheduled_cancel_s == 30.0
    return CertificationDrill(
        "heartbeat_arms_dead_man",
        passed,
        "exchange dead-man heartbeat must be accepted before risk can increase",
        {
            "heartbeat_ok": heartbeat["ok"],
            "order_accepted": record.accepted,
            "scheduled_cancel_s": adapter.scheduled_cancel_s,
        },
    )


def risk_increase_requires_heartbeat() -> CertificationDrill:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)

    record = executor.submit(intent(), idempotency_key="cert-no-heartbeat")

    return CertificationDrill(
        "risk_increase_requires_heartbeat",
        not record.accepted and record.reason == "dead-man switch expired" and adapter.placed == [],
        "risk-increasing orders must fail closed without a fresh heartbeat",
        {"reason": record.reason, "exchange_attempts": len(adapter.placed)},
    )


def idempotent_submit_has_single_exchange_attempt() -> CertificationDrill:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()

    first = executor.submit(intent(), idempotency_key="cert-idem")
    second = executor.submit(intent(size=0.02), idempotency_key="cert-idem")

    return CertificationDrill(
        "idempotent_submit_has_single_exchange_attempt",
        first is second and first.accepted and len(adapter.placed) == 1,
        "duplicate idempotency keys must return the cached record without another exchange call",
        {"orders": len(adapter.placed), "cached": first is second},
    )


def exchange_submit_outage_fails_closed_without_retry() -> CertificationDrill:
    adapter = FailingPlaceOrderAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()

    record = executor.submit(intent(), idempotency_key="cert-outage")

    return CertificationDrill(
        "exchange_submit_outage_fails_closed_without_retry",
        not record.accepted and record.status == "exchange_error" and adapter.attempts == 1,
        "exchange submit failures must become auditable refused records and must not retry",
        {"status": record.status, "reason": record.reason, "exchange_attempts": adapter.attempts},
    )


def pause_blocks_entries() -> CertificationDrill:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()
    executor.pause()

    record = executor.submit(intent(), idempotency_key="cert-paused")

    return CertificationDrill(
        "pause_blocks_entries",
        not record.accepted and record.reason == "live entries paused" and adapter.placed == [],
        "operator pause must block new entries",
        {"reason": record.reason, "exchange_attempts": len(adapter.placed)},
    )


def reduce_only_flatten_works_while_paused() -> CertificationDrill:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()
    executor.pause()

    records = executor.flatten(
        {"BTC": Position("BTC", quantity=0.02, avg_price=50_000)},
        {"BTC": 50_100},
        idempotency_prefix="cert-flatten",
    )

    order = adapter.placed[0] if adapter.placed else {}
    passed = (
        len(records) == 1
        and records[0].accepted
        and bool(order.get("reduce_only"))
        and order.get("side") == "sell"
    )
    return CertificationDrill(
        "reduce_only_flatten_works_while_paused",
        passed,
        "risk reduction must remain available while entries are paused",
        {"orders": len(adapter.placed), "reduce_only": order.get("reduce_only"), "side": order.get("side")},
    )


def kill_cancels_and_blocks_new_risk() -> CertificationDrill:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()

    killed = executor.kill()
    record = executor.submit(intent(), idempotency_key="cert-after-kill")

    return CertificationDrill(
        "kill_cancels_and_blocks_new_risk",
        killed["state"] == "killed"
        and adapter.cancel_count == 1
        and not record.accepted
        and record.reason == "kill switch active",
        "kill must cancel open orders and block later risk increases",
        {"cancel_count": adapter.cancel_count, "reason": record.reason},
    )


def rejected_exchange_dead_man_blocks_entries() -> CertificationDrill:
    adapter = FailingScheduleCancelAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)

    heartbeat = executor.heartbeat()
    record = executor.submit(intent(), idempotency_key="cert-deadman-reject")

    return CertificationDrill(
        "rejected_exchange_dead_man_blocks_entries",
        not heartbeat["ok"] and not record.accepted and record.reason == "dead-man switch expired",
        "failed exchange dead-man scheduling must not arm the executor",
        {"heartbeat_ok": heartbeat["ok"], "reason": record.reason},
    )


def rate_limit_blocks_second_order() -> CertificationDrill:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(
        adapter=adapter,
        enabled=True,
        policy=LiveExecutionPolicy(max_orders_per_minute=1),
    )
    executor.heartbeat()

    first = executor.submit(intent(), idempotency_key="cert-rate-1")
    second = executor.submit(intent(), idempotency_key="cert-rate-2")

    return CertificationDrill(
        "rate_limit_blocks_second_order",
        first.accepted and not second.accepted and second.reason == "live order rate limit exceeded",
        "per-minute order rate limits must block bursts",
        {"first": first.reason, "second": second.reason, "orders": len(adapter.placed)},
    )


def daily_loss_limit_blocks_entries() -> CertificationDrill:
    adapter = RecordingExchangeAdapter()
    executor = LiveExecutor(adapter=adapter, enabled=True)
    executor.heartbeat()
    executor.daily_loss_usd = executor.policy.max_daily_loss_usd

    record = executor.submit(intent(), idempotency_key="cert-loss")

    return CertificationDrill(
        "daily_loss_limit_blocks_entries",
        not record.accepted and record.reason == "daily loss limit reached",
        "daily loss limit must block new entries",
        {"reason": record.reason, "exchange_attempts": len(adapter.placed)},
    )


def intent(*, size: float = 0.01, reduce_only: bool = False) -> OrderIntent:
    return OrderIntent(
        symbol="BTC",
        side=Side.BUY,
        quantity=size,
        price=50_000,
        confidence=0.9,
        reduce_only=reduce_only,
    )
