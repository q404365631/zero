from __future__ import annotations

import hashlib
from collections.abc import Callable
from dataclasses import dataclass, field
from time import time
from typing import Any, Protocol

from zero_engine.models import OrderIntent, Position, Side


class LiveExchangeAdapter(Protocol):
    def place_order(self, intent: OrderIntent, *, cloid: str) -> dict[str, Any]: ...

    def cancel_all(self) -> dict[str, Any]: ...

    def schedule_cancel(self, timeout_s: float) -> dict[str, Any]: ...


def hyperliquid_cloid(idempotency_key: str) -> str:
    """Derive a deterministic Hyperliquid client order id."""

    digest = hashlib.sha256(idempotency_key.encode("utf-8")).hexdigest()[:32]
    return f"0x{digest}"


@dataclass(frozen=True)
class LiveExecutionPolicy:
    max_notional_usd: float = 1_000.0
    max_daily_loss_usd: float = 250.0
    max_orders_per_minute: int = 6
    dead_man_timeout_s: float = 30.0


@dataclass(frozen=True)
class LiveExecutionRecord:
    accepted: bool
    status: str
    reason: str
    idempotency_key: str
    symbol: str
    side: str
    quantity: float
    price: float
    notional_usd: float
    reduce_only: bool
    as_of: float
    exchange_response: dict[str, Any] = field(default_factory=dict)
    trace_id: str | None = None
    operator_context: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        payload = {
            "accepted": self.accepted,
            "status": self.status,
            "reason": self.reason,
            "idempotency_key": self.idempotency_key,
            "symbol": self.symbol,
            "side": self.side,
            "quantity": self.quantity,
            "price": self.price,
            "notional_usd": round(self.notional_usd, 2),
            "reduce_only": self.reduce_only,
            "as_of": self.as_of,
            "exchange_response": self.exchange_response,
        }
        if self.trace_id:
            payload["trace_id"] = self.trace_id
        if self.operator_context:
            payload["operator_context"] = dict(self.operator_context)
        return payload


@dataclass
class LiveExecutor:
    adapter: LiveExchangeAdapter
    policy: LiveExecutionPolicy = field(default_factory=LiveExecutionPolicy)
    clock: Callable[[], float] = time
    enabled: bool = False
    kill_switch_active: bool = False
    paused: bool = False
    daily_loss_usd: float = 0.0
    last_heartbeat_at: float | None = None
    execution_cache: dict[str, LiveExecutionRecord] = field(default_factory=dict)
    records: list[LiveExecutionRecord] = field(default_factory=list)
    order_timestamps: list[float] = field(default_factory=list)

    def heartbeat(self) -> dict[str, Any]:
        now = self.clock()
        try:
            scheduled = self.adapter.schedule_cancel(self.policy.dead_man_timeout_s)
        except Exception as exc:
            return {
                "ok": False,
                "as_of": now,
                "dead_man_timeout_s": self.policy.dead_man_timeout_s,
                "exchange_dead_man": {"ok": False, "error": str(exc)},
            }
        ok = bool(scheduled.get("ok", True))
        if ok:
            self.last_heartbeat_at = now
        return {
            "ok": ok,
            "as_of": now,
            "dead_man_timeout_s": self.policy.dead_man_timeout_s,
            "exchange_dead_man": scheduled,
        }

    def pause(self) -> dict[str, Any]:
        self.paused = True
        return {"ok": True, "state": "paused", "as_of": self.clock()}

    def resume(self) -> dict[str, Any]:
        self.paused = False
        return {"ok": True, "state": "running", "as_of": self.clock()}

    def kill(self) -> dict[str, Any]:
        self.kill_switch_active = True
        self.paused = True
        try:
            response = self.adapter.cancel_all()
        except Exception as exc:
            response = {"ok": False, "error": str(exc)}
        return {"ok": True, "state": "killed", "as_of": self.clock(), "exchange_cancel": response}

    def submit(
        self,
        intent: OrderIntent,
        *,
        idempotency_key: str,
        trace_id: str | None = None,
        operator_context: dict[str, Any] | None = None,
    ) -> LiveExecutionRecord:
        if idempotency_key in self.execution_cache:
            return self.execution_cache[idempotency_key]

        refusal = self.refusal_reason(intent)
        now = self.clock()
        if refusal is not None:
            record = LiveExecutionRecord(
                accepted=False,
                status="refused",
                reason=refusal,
                idempotency_key=idempotency_key,
                symbol=intent.symbol,
                side=intent.side.value,
                quantity=intent.quantity,
                price=intent.price,
                notional_usd=intent.notional_usd,
                reduce_only=intent.reduce_only,
                as_of=now,
                trace_id=trace_id,
                operator_context=operator_context,
            )
            self.record(idempotency_key, record)
            return record

        try:
            response = self.adapter.place_order(intent, cloid=hyperliquid_cloid(idempotency_key))
        except Exception as exc:
            record = LiveExecutionRecord(
                accepted=False,
                status="exchange_error",
                reason=f"exchange order submit failed: {exc}",
                idempotency_key=idempotency_key,
                symbol=intent.symbol,
                side=intent.side.value,
                quantity=intent.quantity,
                price=intent.price,
                notional_usd=intent.notional_usd,
                reduce_only=intent.reduce_only,
                as_of=now,
                exchange_response={"ok": False, "error": str(exc)},
                trace_id=trace_id,
                operator_context=operator_context,
            )
            self.record(idempotency_key, record)
            return record

        record = LiveExecutionRecord(
            accepted=True,
            status="submitted",
            reason="submitted",
            idempotency_key=idempotency_key,
            symbol=intent.symbol,
            side=intent.side.value,
            quantity=intent.quantity,
            price=intent.price,
            notional_usd=intent.notional_usd,
            reduce_only=intent.reduce_only,
            as_of=now,
            exchange_response=response,
            trace_id=trace_id,
            operator_context=operator_context,
        )
        self.order_timestamps.append(now)
        self.record(idempotency_key, record)
        return record

    def flatten(
        self,
        positions: dict[str, Position],
        prices: dict[str, float],
        *,
        idempotency_prefix: str,
        trace_id: str | None = None,
        operator_context: dict[str, Any] | None = None,
    ) -> list[LiveExecutionRecord]:
        records: list[LiveExecutionRecord] = []
        for symbol, position in sorted(positions.items()):
            if position.quantity == 0:
                continue
            side = Side.SELL if position.quantity > 0 else Side.BUY
            price = prices.get(symbol) or position.avg_price
            intent = OrderIntent(
                symbol=symbol,
                side=side,
                quantity=abs(position.quantity),
                price=price,
                confidence=1.0,
                reduce_only=True,
            )
            records.append(
                self.submit(
                    intent,
                    idempotency_key=f"{idempotency_prefix}-{symbol}",
                    trace_id=trace_id,
                    operator_context=operator_context,
                )
            )
        return records

    def refusal_reason(self, intent: OrderIntent) -> str | None:
        if not self.enabled:
            return "live executor disabled"
        if self.kill_switch_active:
            return "kill switch active"
        if self.paused and not intent.reduce_only:
            return "live entries paused"
        if self.dead_man_expired():
            return "dead-man switch expired"
        if self.daily_loss_usd >= self.policy.max_daily_loss_usd and not intent.reduce_only:
            return "daily loss limit reached"
        if intent.notional_usd > self.policy.max_notional_usd and not intent.reduce_only:
            return "live order notional exceeds limit"
        if self.order_rate_exhausted() and not intent.reduce_only:
            return "live order rate limit exceeded"
        return None

    def dead_man_expired(self) -> bool:
        if self.policy.dead_man_timeout_s <= 0:
            return True
        if self.last_heartbeat_at is None:
            return True
        return (self.clock() - self.last_heartbeat_at) > self.policy.dead_man_timeout_s

    def order_rate_exhausted(self) -> bool:
        now = self.clock()
        self.order_timestamps = [ts for ts in self.order_timestamps if now - ts < 60]
        return len(self.order_timestamps) >= self.policy.max_orders_per_minute

    def record(self, idempotency_key: str, record: LiveExecutionRecord) -> None:
        self.execution_cache[idempotency_key] = record
        self.records.append(record)


@dataclass
class RecordingExchangeAdapter:
    placed: list[dict[str, Any]] = field(default_factory=list)
    cancel_count: int = 0
    scheduled_cancel_s: float | None = None

    def place_order(self, intent: OrderIntent, *, cloid: str) -> dict[str, Any]:
        payload = {
            "cloid": cloid,
            "symbol": intent.symbol,
            "side": intent.side.value,
            "size": intent.quantity,
            "price": intent.price,
            "reduce_only": intent.reduce_only,
        }
        self.placed.append(payload)
        return {"ok": True, "order": payload}

    def cancel_all(self) -> dict[str, Any]:
        self.cancel_count += 1
        return {"ok": True, "cancelled": "all"}

    def schedule_cancel(self, timeout_s: float) -> dict[str, Any]:
        self.scheduled_cancel_s = timeout_s
        return {"ok": True, "timeout_s": timeout_s}


class HyperliquidSdkAdapter:
    """Thin boundary around Hyperliquid's official Python SDK.

    The public engine deliberately avoids hand-rolled signatures. Hyperliquid's
    docs recommend using the SDK because field order and signing scheme details
    are easy to get wrong.
    """

    def __init__(
        self,
        *,
        wallet_address: str,
        private_key: str,
        endpoint: str = "https://api.hyperliquid.xyz",
    ) -> None:
        try:
            from eth_account import Account  # type: ignore[import-not-found]
            from hyperliquid.exchange import Exchange  # type: ignore[import-not-found]
            from hyperliquid.info import Info  # type: ignore[import-not-found]
            from hyperliquid.utils.types import Cloid  # type: ignore[import-not-found]
        except Exception as exc:
            raise RuntimeError(
                "Hyperliquid live execution requires the official hyperliquid-python-sdk"
            ) from exc

        self.wallet_address = wallet_address.lower()
        self._cloid = Cloid
        account = Account.from_key(private_key)
        self.exchange = Exchange(account, endpoint, account_address=self.wallet_address)
        self.info = Info(endpoint, skip_ws=True)

    def place_order(self, intent: OrderIntent, *, cloid: str) -> dict[str, Any]:
        result = self.exchange.order(
            intent.symbol,
            intent.side is Side.BUY,
            intent.quantity,
            intent.price,
            {"limit": {"tif": "Ioc"}},
            reduce_only=intent.reduce_only,
            cloid=self._cloid.from_str(cloid),
        )
        return {"ok": True, "raw": result}

    def cancel_all(self) -> dict[str, Any]:
        open_orders = self.info.open_orders(self.wallet_address)
        cancelled: list[dict[str, Any]] = []
        skipped: list[dict[str, Any]] = []
        for order in open_orders:
            coin = order.get("coin")
            oid = order.get("oid")
            if coin is None or oid is None:
                skipped.append(order)
                continue
            result = self.exchange.cancel(str(coin), int(oid))
            cancelled.append({"coin": coin, "oid": oid, "raw": result})
        return {
            "ok": True,
            "open_orders": len(open_orders),
            "cancelled": len(cancelled),
            "skipped": len(skipped),
            "responses": cancelled,
        }

    def schedule_cancel(self, timeout_s: float) -> dict[str, Any]:
        cancel_at_ms = int((time() + timeout_s) * 1000)
        result = self.exchange.schedule_cancel(cancel_at_ms)
        return {"ok": True, "cancel_at_ms": cancel_at_ms, "raw": result}
