from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import sys
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable
from urllib.parse import parse_qs, urlparse

from zero_engine.hyperliquid import (
    HyperliquidInfoClient,
    HyperliquidMarketStatus,
    is_hex_address,
    is_private_key,
    redact_secret,
    validate_dry_run_order,
)
from zero_engine.intelligence import (
    IntelligenceConfig,
    export_intelligence_snapshot,
    intelligence_catalog,
    intelligence_snapshot,
)
from zero_engine.journal import DecisionJournal
from zero_engine.live import HyperliquidSdkAdapter, LiveExecutionPolicy, LiveExecutor
from zero_engine.models import OrderIntent, Position, Side
from zero_engine.network import PublicProfileConfig, public_profile, publish_profile
from zero_engine.paper import DecisionRecord, PaperEngine
from zero_engine.safety import evaluate_order


DEFAULT_PRICES = {
    "BTC": 40_500.0,
    "ETH": 2_850.0,
    "SOL": 150.0,
}


def utc_now() -> datetime:
    return datetime.now(UTC)


@dataclass(frozen=True)
class PriceQuote:
    symbol: str
    price: float
    source: str
    as_of: datetime

    def to_dict(self) -> dict[str, Any]:
        return {
            "symbol": self.symbol,
            "price": self.price,
            "source": self.source,
            "as_of": self.as_of.isoformat().replace("+00:00", "Z"),
        }


@dataclass
class ApiMetrics:
    request_count: int = 0
    by_method: dict[str, int] = field(default_factory=dict)
    by_path: dict[str, int] = field(default_factory=dict)
    by_status: dict[str, int] = field(default_factory=dict)
    total_request_ms: float = 0.0
    execute_count: int = 0
    execute_accepted: int = 0
    execute_rejected: int = 0
    idempotency_hits: int = 0
    last_trace_id: str | None = None
    last_request_at: str | None = None

    def record_request(
        self,
        *,
        method: str,
        path: str,
        status: int,
        elapsed_ms: float,
        trace_id: str,
        at: str,
    ) -> None:
        self.request_count += 1
        self.by_method[method] = self.by_method.get(method, 0) + 1
        self.by_path[path] = self.by_path.get(path, 0) + 1
        status_key = str(int(status))
        self.by_status[status_key] = self.by_status.get(status_key, 0) + 1
        self.total_request_ms += elapsed_ms
        self.last_trace_id = trace_id
        self.last_request_at = at

    def record_execute(self, *, accepted: bool, idempotency_hit: bool = False) -> None:
        self.execute_count += 1
        if accepted:
            self.execute_accepted += 1
        else:
            self.execute_rejected += 1
        if idempotency_hit:
            self.idempotency_hits += 1

    def to_dict(self) -> dict[str, Any]:
        avg_ms = self.total_request_ms / self.request_count if self.request_count else 0.0
        return {
            "request_count": self.request_count,
            "by_method": dict(sorted(self.by_method.items())),
            "by_path": dict(sorted(self.by_path.items())),
            "by_status": dict(sorted(self.by_status.items())),
            "avg_request_ms": round(avg_ms, 3),
            "execute_count": self.execute_count,
            "execute_accepted": self.execute_accepted,
            "execute_rejected": self.execute_rejected,
            "idempotency_hits": self.idempotency_hits,
            "last_trace_id": self.last_trace_id,
            "last_request_at": self.last_request_at,
        }


@dataclass
class PaperApiState:
    engine: PaperEngine = field(default_factory=PaperEngine)
    prices: dict[str, float] = field(default_factory=lambda: dict(DEFAULT_PRICES))
    hyperliquid: HyperliquidInfoClient | None = None
    use_live_hyperliquid_prices: bool = False
    price_cache_ttl_s: float = 2.0
    clock: Callable[[], datetime] = field(default_factory=lambda: utc_now)
    started_at: datetime = field(default_factory=utc_now)
    auto_enabled: bool = False
    execution_cache: dict[str, dict[str, Any]] = field(default_factory=dict)
    price_cache: HyperliquidMarketStatus | None = None
    price_cache_at: datetime | None = None
    last_market_error: str | None = None
    metrics: ApiMetrics = field(default_factory=ApiMetrics)
    trace_sequence: int = 0
    live_wallet_address: str | None = None
    live_api_private_key: str | None = None
    live_kill_switch_path: str | None = None
    live_dead_man_timeout_s: float = 30.0
    live_executor: LiveExecutor | None = None
    network_handle: str = "local-operator"
    network_display_name: str | None = None
    network_publish_enabled: bool = False
    network_publish_path: str | None = None
    intelligence_public_delay_s: int = 900
    intelligence_export_path: str | None = None

    def __post_init__(self) -> None:
        if self.execution_cache:
            return
        for record in self.engine.decisions:
            if record.idempotency_key:
                self.execution_cache[record.idempotency_key] = execute_response_from_record(record)

    def now(self) -> datetime:
        return self.clock()

    def now_iso(self) -> str:
        return self.now().isoformat().replace("+00:00", "Z")

    def price_for(self, symbol: str) -> float:
        return self.quote_for(symbol).price

    def quote_for(self, symbol: str) -> PriceQuote:
        normalized = symbol.upper()
        if self.use_live_hyperliquid_prices:
            status = self.hyperliquid_status_cached()
            price = status.price_for(normalized)
            if price is None:
                raise ValueError(f"{normalized} missing from Hyperliquid allMids")
            return PriceQuote(
                symbol=normalized,
                price=price,
                source="hyperliquid:allMids",
                as_of=self.price_cache_at or self.now(),
            )

        return PriceQuote(
            symbol=normalized,
            price=self.prices.get(normalized, 100.0),
            source="paper:static",
            as_of=self.now(),
        )

    def hyperliquid_status_cached(self) -> HyperliquidMarketStatus:
        if self.hyperliquid is None:
            raise ValueError("--hyperliquid-live-prices requires the Hyperliquid read-only adapter")
        now = self.now()
        if self.price_cache is not None and self.price_cache_at is not None:
            age_s = (now - self.price_cache_at).total_seconds()
            if age_s <= self.price_cache_ttl_s:
                return self.price_cache
        try:
            status = self.hyperliquid.market_status()
        except Exception as exc:
            self.last_market_error = str(exc)
            raise RuntimeError(f"Hyperliquid market data unavailable: {exc}") from exc
        self.price_cache = status
        self.price_cache_at = now
        self.last_market_error = None
        return status

    def market_source(self) -> str:
        return "hyperliquid:allMids" if self.use_live_hyperliquid_prices else "paper:static"

    def next_trace_id(self, method: str, path: str) -> str:
        self.trace_sequence += 1
        seed = f"{self.started_at.isoformat()}:{method}:{path}:{self.trace_sequence}"
        digest = hashlib.sha256(seed.encode("utf-8")).hexdigest()[:16]
        return f"trace-{digest}"

    def record_request(
        self,
        *,
        method: str,
        path: str,
        status: int,
        elapsed_ms: float,
        trace_id: str,
    ) -> None:
        self.metrics.record_request(
            method=method,
            path=path,
            status=status,
            elapsed_ms=elapsed_ms,
            trace_id=trace_id,
            at=self.now_iso(),
        )


class PaperApi:
    def __init__(self, state: PaperApiState | None = None) -> None:
        self.state = state or PaperApiState()

    def get(
        self,
        path: str,
        query: dict[str, list[str]],
        trace_id: str | None = None,
    ) -> tuple[int, dict[str, Any]]:
        trace_id = trace_id or self.state.next_trace_id("GET", path)
        started = time.perf_counter()
        status, payload = self._get(path, query, trace_id)
        self.state.record_request(
            method="GET",
            path=path,
            status=status,
            elapsed_ms=(time.perf_counter() - started) * 1000,
            trace_id=trace_id,
        )
        return status, payload

    def _get(
        self,
        path: str,
        query: dict[str, list[str]],
        trace_id: str,
    ) -> tuple[int, dict[str, Any]]:
        routes = {
            "/": self.root,
            "/health": self.health,
            "/v2/status": self.v2_status,
            "/positions": self.positions,
            "/risk": self.risk,
            "/brief": self.brief,
            "/regime": lambda: self.regime(query),
            "/pulse": lambda: self.pulse(query),
            "/approaching": self.approaching,
            "/rejections": lambda: self.rejections(query),
            "/journal": lambda: self.journal(query),
            "/audit/export": lambda: self.audit_export(query),
            "/hl/status": lambda: self.hl_status(query),
            "/intelligence/catalog": self.intelligence_catalog,
            "/intelligence/snapshot": self.intelligence_snapshot,
            "/live/preflight": self.live_preflight,
            "/market/quote": lambda: self.market_quote(query),
            "/metrics": self.metrics,
            "/network/profile": self.network_profile,
            "/network/leaderboard": self.network_leaderboard,
            "/operator/state": self.operator_state,
        }
        if path.startswith("/evaluate/"):
            try:
                return HTTPStatus.OK, self.evaluate(path.removeprefix("/evaluate/"), trace_id=trace_id)
            except RuntimeError as exc:
                return HTTPStatus.SERVICE_UNAVAILABLE, {"error": str(exc)}
            except ValueError as exc:
                return HTTPStatus.BAD_REQUEST, {"error": str(exc)}
        handler = routes.get(path)
        if handler is None:
            return HTTPStatus.NOT_FOUND, {"error": "not found", "path": path}
        try:
            return HTTPStatus.OK, handler()
        except RuntimeError as exc:
            return HTTPStatus.SERVICE_UNAVAILABLE, {"error": str(exc)}
        except ValueError as exc:
            return HTTPStatus.BAD_REQUEST, {"error": str(exc)}

    def post(
        self,
        path: str,
        payload: dict[str, Any],
        trace_id: str | None = None,
        expose_trace: bool = False,
        mode: str | None = None,
    ) -> tuple[int, dict[str, Any]]:
        trace_id = trace_id or self.state.next_trace_id("POST", path)
        started = time.perf_counter()
        status, response = self._post(path, payload, trace_id, expose_trace=expose_trace, mode=mode)
        self.state.record_request(
            method="POST",
            path=path,
            status=status,
            elapsed_ms=(time.perf_counter() - started) * 1000,
            trace_id=trace_id,
        )
        return status, response

    def _post(
        self,
        path: str,
        payload: dict[str, Any],
        trace_id: str,
        *,
        expose_trace: bool = False,
        mode: str | None = None,
    ) -> tuple[int, dict[str, Any]]:
        if path == "/execute":
            try:
                if mode == "live":
                    return HTTPStatus.OK, self.live_execute(
                        payload,
                        trace_id=trace_id,
                        expose_trace=expose_trace,
                    )
                return HTTPStatus.OK, self.execute(
                    payload,
                    trace_id=trace_id,
                    expose_trace=expose_trace,
                )
            except RuntimeError as exc:
                return HTTPStatus.SERVICE_UNAVAILABLE, {"error": str(exc)}
            except ValueError as exc:
                return HTTPStatus.BAD_REQUEST, {"error": str(exc)}
        if path == "/auto/toggle":
            enabled = bool(payload.get("enabled"))
            self.state.auto_enabled = enabled
            return HTTPStatus.OK, {
                "state": "on" if enabled else "off",
                "simulated": True,
                "reason": None,
                **({"trace_id": trace_id} if expose_trace else {}),
            }
        if path == "/operator/events":
            return HTTPStatus.OK, {
                "accepted": 1,
                "snapshot": self.operator_state(),
                **({"trace_id": trace_id} if expose_trace else {}),
            }
        if path == "/network/publish":
            try:
                return HTTPStatus.OK, self.network_publish(payload)
            except ValueError as exc:
                return HTTPStatus.BAD_REQUEST, {"error": str(exc)}
        if path == "/intelligence/export":
            return HTTPStatus.OK, self.intelligence_export(payload)
        if path == "/live/heartbeat":
            return HTTPStatus.OK, self.live_heartbeat()
        if path == "/live/pause":
            return HTTPStatus.OK, self.live_pause()
        if path == "/live/resume":
            return HTTPStatus.OK, self.live_resume()
        if path == "/live/kill":
            return HTTPStatus.OK, self.live_kill()
        if path == "/live/flatten":
            return HTTPStatus.OK, self.live_flatten(trace_id=trace_id)
        return HTTPStatus.NOT_FOUND, {"error": "not found", "path": path}

    def root(self) -> dict[str, Any]:
        return {"name": "zero-paper-engine", "version": "0.1.0", "status": "ok", "ts": self.state.now_iso()}

    def health(self) -> dict[str, Any]:
        ts = self.state.now_iso()
        exchange = "hyperliquid" if self.state.hyperliquid is not None else "paper"
        market_data = "live" if self.state.use_live_hyperliquid_prices else "fixture"
        recovery = self.recovery()
        return {
            "status": "ok",
            "components": {
                "paper_engine": {"status": "healthy", "last_seen": ts, "age_s": 0.0},
                "risk": {"status": "healthy", "last_seen": ts, "age_s": 0.0},
                "recovery": {
                    "status": "healthy",
                    "last_seen": ts,
                    "age_s": 0.0,
                    "mode": "durable" if recovery["durable"] else "ephemeral",
                },
            },
            "dependencies": {
                "exchange": exchange,
                "market_data": market_data,
                "journal": "durable" if recovery["durable"] else "ephemeral",
                "secrets": "not_required",
                "live_preflight": "available",
            },
            "circuit_breakers": {
                "paper": "closed",
                "hl_info": "closed" if self.state.hyperliquid is not None else "n/a",
            },
            "risk": {"equity": 10_000.0, "drawdown_pct": 0.0, "kill_all": False},
            "recovery": recovery,
            "ws_connections": 0,
        }

    def live_preflight(self) -> dict[str, Any]:
        checks: list[dict[str, Any]] = []

        def add(name: str, status: str, note: str, **extra: Any) -> None:
            checks.append({"name": name, "status": status, "note": note, **extra})

        wallet = self.state.live_wallet_address
        private_key = self.state.live_api_private_key
        wallet_ok = bool(wallet and is_hex_address(wallet))
        key_ok = is_private_key(private_key)

        live_executor_ready = (
            self.state.live_executor is not None
            and self.state.live_executor.enabled
            and not self.state.live_executor.dead_man_expired()
        )
        add(
            "live_executor",
            "ok" if live_executor_ready else "fail",
            "live executor configured" if live_executor_ready else "live executor not configured",
        )
        add(
            "wallet_address",
            "ok" if wallet_ok else "fail",
            redact_secret(wallet) if wallet_ok else "set ZERO_HYPERLIQUID_WALLET_ADDRESS",
        )
        add(
            "api_private_key",
            "ok" if key_ok else "fail",
            redact_secret(private_key) if key_ok else "store key locally; never commit it",
            source="env_or_keychain",
        )

        if self.state.hyperliquid is None:
            add("account_read", "fail", "start with --hyperliquid to verify account state")
        elif not wallet_ok:
            add("account_read", "fail", "valid wallet address required before account read")
        else:
            try:
                state = self.state.hyperliquid.clearinghouse_state(wallet or "")
                positions = state.get("assetPositions", []) if isinstance(state, dict) else []
                add("account_read", "ok", f"clearinghouseState read ok · positions={len(positions)}")
            except Exception as exc:
                add("account_read", "fail", f"Hyperliquid account read failed: {exc}")

        try:
            validated = validate_dry_run_order({"coin": "BTC", "side": "buy", "size": 0.001})
            add(
                "dry_run_order",
                "ok",
                f"{validated['side']} {validated['size']} {validated['coin']} validates locally",
            )
        except ValueError as exc:
            add("dry_run_order", "fail", str(exc))

        durable = self.state.engine.journal is not None
        add(
            "journal",
            "ok" if durable else "fail",
            "append-only decision journal configured" if durable else "start with --journal",
        )

        limits = self.state.engine.limits
        risk_ok = limits.max_notional_usd > 0 and limits.max_position_notional_usd > 0
        add(
            "risk_limits",
            "ok" if risk_ok else "fail",
            f"max_notional_usd={limits.max_notional_usd:g} "
            f"max_position_notional_usd={limits.max_position_notional_usd:g}"
            if risk_ok
            else "risk limits must be positive",
        )

        kill_path = self.state.live_kill_switch_path
        kill_ok = bool(kill_path and os.path.exists(kill_path))
        dead_man_ok = self.state.live_dead_man_timeout_s > 0
        add(
            "emergency_controls",
            "ok" if kill_ok and dead_man_ok else "fail",
            f"kill switch armed at {kill_path}; dead_man_timeout_s={self.state.live_dead_man_timeout_s:g}"
            if kill_ok and dead_man_ok
            else "set ZERO_LIVE_KILL_SWITCH_PATH to an existing local file",
        )

        controls_ready = all(check["status"] == "ok" for check in checks if check["name"] != "live_executor")
        ready = controls_ready and all(check["status"] == "ok" for check in checks)
        return {
            "schema_version": "zero.live_preflight.v1",
            "generated_at": self.state.now_iso(),
            "exchange": "hyperliquid",
            "mode": "paper" if self.state.live_executor is None else "live-capable",
            "ready": ready,
            "live_mode": "ready" if ready else "refused",
            "controls_ready": controls_ready,
            "checks": checks,
        }

    def v2_status(self) -> dict[str, Any]:
        positions = list(self.state.engine.positions.values())
        return {
            "confidence": {"score": 90, "level": "paper"},
            "market": {
                "regime": "HYPERLIQUID LIVE MIDS. Paper execution only."
                if self.state.use_live_hyperliquid_prices
                else "PAPER MARKET. Local deterministic demo.",
                "health": 1.0,
                "signal": "live" if self.state.use_live_hyperliquid_prices else "stable",
                "prediction": "stable",
                "fear_greed": 50,
                "coins_tradeable": self.coins_tradeable(),
            },
            "positions": {
                "open": len([p for p in positions if p.quantity != 0]),
                "unrealized_pnl": 0.0,
                "equity": 10_000.0,
            },
            "today": {
                "trades": len(self.state.engine.fills),
                "wins": 0,
                "pnl": 0.0,
                "streak": 0,
                "sizing_mult": 1.0,
            },
            "approaching": [],
            "blind_spots": [],
            "alert": None,
            "recovery": self.recovery(),
            "ts": self.state.now_iso(),
        }

    def recovery(self) -> dict[str, Any]:
        payload = self.state.engine.recovery.to_dict()
        last_ts = payload.pop("last_decision_ts")
        payload["last_decision_at"] = epoch_to_iso(last_ts) if last_ts is not None else None
        payload["current_decisions"] = len(self.state.engine.decisions)
        payload["current_fills"] = len(self.state.engine.fills)
        payload["current_rejections"] = len(self.state.engine.rejections)
        payload["current_positions"] = len(
            [p for p in self.state.engine.positions.values() if p.quantity != 0]
        )
        return payload

    def positions(self) -> dict[str, Any]:
        items = [
            position_to_wire(position, self.state.quote_for(position.symbol))
            for position in self.state.engine.positions.values()
            if position.quantity != 0
        ]
        return {
            "positions": items,
            "count": len(items),
            "account_value": 10_000.0,
            "total_unrealized_pnl": round(sum(item["unrealized_pnl"] for item in items), 2),
        }

    def risk(self) -> dict[str, Any]:
        open_count = len([p for p in self.state.engine.positions.values() if p.quantity != 0])
        return {
            "account_value": 10_000.0,
            "updated_at": self.state.now_iso(),
            "daily_pnl_usd": 0.0,
            "daily_loss_usd": 0.0,
            "per_runner": {},
            "global_halt": False,
            "daily_loss_since": self.state.started_at.isoformat().replace("+00:00", "Z"),
            "halted": False,
            "halt_reason": None,
            "halt_until": None,
            "stop_failure_halt": False,
            "open_count": open_count,
            "drawdown_pct": 0.0,
            "peak_equity": 10_000.0,
            "last_drawdown_alert_pct": 20.0,
            "peak_equity_30d": 10_000.0,
            "capital_floor_hit": False,
        }

    def brief(self) -> dict[str, Any]:
        return {
            "timestamp": self.state.now_iso(),
            "fear_greed": 50,
            "open_positions": self.positions()["count"],
            "positions": self.positions()["positions"],
            "recent_signals": [],
            "approaching": [],
            "last_cycle": {
                "mode": "paper",
                "decisions": len(self.state.engine.decisions),
                "fills": len(self.state.engine.fills),
                "rejections": len(self.state.engine.rejections),
            },
        }

    def regime(self, query: dict[str, list[str]]) -> dict[str, Any]:
        coin = first(query, "coin")
        return {
            "coin": coin,
            "regime": "PAPER",
            "confidence": 1.0,
            "source": "zero-paper-api",
        }

    def evaluate(self, raw_symbol: str, trace_id: str | None = None) -> dict[str, Any]:
        symbol = raw_symbol.upper()
        quote = self.state.quote_for(symbol)
        intent = OrderIntent(symbol, Side.BUY, quantity=1 / quote.price, price=quote.price, confidence=0.9)
        decision = evaluate_order(intent, self.state.engine.limits, self.state.engine.positions.get(symbol))
        payload = {
            "coin": symbol,
            "price": quote.price,
            "price_source": quote.source,
            "consensus": 90 if decision.allowed else 0,
            "conviction": 0.9,
            "direction": "LONG" if decision.allowed else "NONE",
            "regime": "PAPER",
            "layers": [
                {
                    "layer": "risk",
                    "passed": decision.allowed,
                    "value": {"notional_usd": round(intent.notional_usd, 2)},
                    "detail": decision.reason,
                }
            ],
            "data_fresh": True,
            "timestamp": self.state.now_iso(),
        }
        if trace_id:
            payload["trace_id"] = trace_id
        return payload

    def pulse(self, query: dict[str, list[str]]) -> dict[str, Any]:
        limit = int(first(query, "limit") or "20")
        events = [
            {
                "kind": "decision",
                "coin": record.intent.symbol,
                "message": record.decision.reason,
                "severity": "info" if record.decision.allowed else "warn",
                "ts": epoch_to_iso(record.as_of),
                "trace_id": record.trace_id,
            }
            for record in self.state.engine.decisions[-limit:]
        ]
        return {"events": events}

    def approaching(self) -> dict[str, Any]:
        return {"approaching": []}

    def rejections(self, query: dict[str, list[str]]) -> dict[str, Any]:
        limit = int(first(query, "limit") or "50")
        coin = first(query, "coin")
        records = [
            record
            for record in self.state.engine.decisions
            if not record.decision.allowed and (coin is None or record.intent.symbol == coin.upper())
        ]
        return {"rejections": [rejection_to_wire(record) for record in records[-limit:]]}

    def journal(self, query: dict[str, list[str]]) -> dict[str, Any]:
        limit = int(first(query, "limit") or "50")
        if self.state.engine.journal is not None:
            decisions = self.state.engine.journal.tail(limit)
        else:
            decisions = [record.to_dict() for record in self.state.engine.decisions[-limit:]]
        return {"decisions": decisions, "count": len(decisions)}

    def metrics(self) -> dict[str, Any]:
        decisions = self.state.engine.decisions
        return {
            "schema_version": "zero.metrics.v1",
            "generated_at": self.state.now_iso(),
            "mode": "paper",
            "runtime": {
                "started_at": self.state.started_at.isoformat().replace("+00:00", "Z"),
                "uptime_s": max(0.0, (self.state.now() - self.state.started_at).total_seconds()),
                "market_source": self.state.market_source(),
            },
            "api": self.state.metrics.to_dict(),
            "engine": {
                "decisions": len(decisions),
                "fills": len(self.state.engine.fills),
                "rejections": len(self.state.engine.rejections),
                "open_positions": len(
                    [p for p in self.state.engine.positions.values() if p.quantity != 0]
                ),
                "acceptance_rate": round(
                    len([record for record in decisions if record.decision.allowed]) / len(decisions),
                    4,
                )
                if decisions
                else 0.0,
            },
            "recovery": self.recovery(),
        }

    def audit_export(self, query: dict[str, list[str]]) -> dict[str, Any]:
        limit = int(first(query, "limit") or "100")
        if self.state.engine.journal is not None:
            decisions = self.state.engine.journal.tail(limit)
            source = "journal"
        else:
            decisions = [record.to_dict() for record in self.state.engine.decisions[-limit:]]
            source = "memory"
        return {
            "schema_version": "zero.audit.v1",
            "exported_at": self.state.now_iso(),
            "mode": "paper",
            "source": source,
            "limit": limit,
            "summary": {
                "decisions": len(self.state.engine.decisions),
                "fills": len(self.state.engine.fills),
                "rejections": len(self.state.engine.rejections),
                "open_positions": len(
                    [p for p in self.state.engine.positions.values() if p.quantity != 0]
                ),
            },
            "retention": {
                "policy": "operator-managed",
                "redaction": "no secrets are recorded by the public paper runtime",
                "format": "append-only-jsonl",
            },
            "metrics": self.metrics(),
            "recovery": self.recovery(),
            "decisions": decisions,
        }

    def network_profile(self) -> dict[str, Any]:
        return public_profile(
            self.state.engine,
            config=self.network_config(),
            generated_at=self.state.now_iso(),
            mode=self.network_mode(),
            live_execution_count=self.live_execution_count(),
        )

    def network_leaderboard(self) -> dict[str, Any]:
        profile = self.network_profile()
        return {
            "schema_version": "zero.network.leaderboard.v1",
            "generated_at": self.state.now_iso(),
            "mode": profile["mode"],
            "rows": [profile["leaderboard_row"]],
            "privacy": profile["privacy"],
        }

    def network_publish(self, payload: dict[str, Any]) -> dict[str, Any]:
        handle = str(payload.get("handle") or self.state.network_handle)
        display_name = payload.get("display_name", self.state.network_display_name)
        if display_name is not None:
            display_name = str(display_name)
        config = PublicProfileConfig(
            handle=handle,
            display_name=display_name,
            publish_enabled=True,
        )
        profile = public_profile(
            self.state.engine,
            config=config,
            generated_at=self.state.now_iso(),
            mode=self.network_mode(),
            live_execution_count=self.live_execution_count(),
        )
        return publish_profile(
            profile,
            consent=bool(payload.get("consent")),
            publish_path=self.state.network_publish_path,
        )

    def network_config(self) -> PublicProfileConfig:
        return PublicProfileConfig(
            handle=self.state.network_handle,
            display_name=self.state.network_display_name,
            publish_enabled=self.state.network_publish_enabled,
        )

    def network_mode(self) -> str:
        return "live" if self.live_execution_count() else "paper"

    def live_execution_count(self) -> int:
        if self.state.live_executor is None:
            return 0
        return len([record for record in self.state.live_executor.records if record.accepted])

    def intelligence_snapshot(self) -> dict[str, Any]:
        return intelligence_snapshot(
            self.network_profile(),
            generated_at=self.state.now_iso(),
            config=self.intelligence_config(),
        )

    def intelligence_catalog(self) -> dict[str, Any]:
        return intelligence_catalog(
            generated_at=self.state.now_iso(),
            public_delay_s=self.state.intelligence_public_delay_s,
        )

    def intelligence_export(self, payload: dict[str, Any]) -> dict[str, Any]:
        return export_intelligence_snapshot(
            self.intelligence_snapshot(),
            consent=bool(payload.get("consent")),
            export_path=self.state.intelligence_export_path,
        )

    def intelligence_config(self) -> IntelligenceConfig:
        return IntelligenceConfig(
            public_delay_s=self.state.intelligence_public_delay_s,
            export_path=self.state.intelligence_export_path,
        )

    def hl_status(self, query: dict[str, list[str]]) -> dict[str, Any]:
        if self.state.hyperliquid is None:
            return {
                "enabled": False,
                "exchange": "hyperliquid",
                "reason": "start zero-paper-api with --hyperliquid to enable read-only market data",
            }
        symbols = [symbol.upper() for symbol in query.get("symbol", [])]
        status = self.state.hyperliquid.market_status()
        payload = status.to_dict(symbols=symbols or None)
        payload["enabled"] = True
        payload["exchange"] = "hyperliquid"
        payload["secrets_required"] = False
        return payload

    def market_quote(self, query: dict[str, list[str]]) -> dict[str, Any]:
        symbol = first(query, "symbol")
        if symbol is None:
            raise ValueError("symbol is required")
        quote = self.state.quote_for(symbol)
        payload = quote.to_dict()
        payload["mode"] = "paper"
        payload["live"] = self.state.use_live_hyperliquid_prices
        return payload

    def operator_state(self) -> dict[str, Any]:
        return {
            "label": "fresh",
            "friction": "l0",
            "vector": {
                "velocity": {
                    "last_1h": 0,
                    "last_4h": 0,
                    "last_24h": 0,
                    "baseline_1h": None,
                },
                "deviation": {
                    "overrides_last_10": 0,
                    "verdicts_last_10": 0,
                    "overrides_last_50": 0,
                    "verdicts_last_50": 0,
                },
                "session": {
                    "active_duration_ms": 0,
                    "longest_focus_ms": 0,
                    "since_last_break_ms": 0,
                },
                "loss_reaction": {
                    "median_last_10_ms": 0,
                    "fastest_session_ms": 0,
                    "baseline_ms": None,
                },
                "re_entry": {"within_15m": 0, "within_30m": 0, "within_2h": 0},
                "sleep_proxy": {"hours_since_rest_ended": None},
                "on_break": False,
            },
            "as_of": self.state.now_iso(),
            "version": len(self.state.engine.decisions),
        }

    def execute(
        self,
        payload: dict[str, Any],
        trace_id: str | None = None,
        *,
        expose_trace: bool = False,
    ) -> dict[str, Any]:
        key = str(payload.get("idempotency_key") or "")
        if key and key in self.state.execution_cache:
            cached = self.state.execution_cache[key]
            self.state.metrics.record_execute(
                accepted=bool(cached.get("accepted")),
                idempotency_hit=True,
            )
            return self.state.execution_cache[key]

        symbol = str(payload.get("coin") or "").upper()
        side = Side(str(payload.get("side") or "").lower())
        quantity = float(payload.get("size") or 0)
        quote = self.state.quote_for(symbol)
        intent = OrderIntent(symbol, side, quantity=quantity, price=quote.price, confidence=0.9)
        source = (
            "api:/execute"
            if quote.source == "paper:static"
            else f"api:/execute:{quote.source}"
        )
        self.state.engine.submit(
            intent,
            source=source,
            idempotency_key=key or None,
            trace_id=trace_id,
        )
        response = execute_response_from_record(
            self.state.engine.decisions[-1],
            include_trace=expose_trace,
        )
        self.state.metrics.record_execute(accepted=bool(response["accepted"]))
        if key:
            self.state.execution_cache[key] = response
        return response

    def live_execute(
        self,
        payload: dict[str, Any],
        trace_id: str | None = None,
        *,
        expose_trace: bool = False,
    ) -> dict[str, Any]:
        key = str(payload.get("idempotency_key") or trace_id or "")
        symbol = str(payload.get("coin") or "").upper()
        side = Side(str(payload.get("side") or "").lower())
        quantity = float(payload.get("size") or 0)
        quote = self.state.quote_for(symbol)
        intent = OrderIntent(symbol, side, quantity=quantity, price=quote.price, confidence=0.9)
        executor = self.state.live_executor
        if executor is None:
            self.state.metrics.record_execute(accepted=False)
            return live_response(
                accepted=False,
                reason="live executor not configured",
                intent=intent,
                key=key,
                trace_id=trace_id if expose_trace else None,
            )
        record = executor.submit(intent, idempotency_key=key, trace_id=trace_id)
        self.state.metrics.record_execute(accepted=record.accepted)
        return live_response_from_record(record, include_trace=expose_trace)

    def live_heartbeat(self) -> dict[str, Any]:
        if self.state.live_executor is None:
            return {"ok": False, "reason": "live executor not configured"}
        return self.state.live_executor.heartbeat()

    def live_pause(self) -> dict[str, Any]:
        if self.state.live_executor is None:
            return {"ok": False, "reason": "live executor not configured"}
        return self.state.live_executor.pause()

    def live_resume(self) -> dict[str, Any]:
        if self.state.live_executor is None:
            return {"ok": False, "reason": "live executor not configured"}
        return self.state.live_executor.resume()

    def live_kill(self) -> dict[str, Any]:
        if self.state.live_executor is None:
            return {"ok": False, "reason": "live executor not configured"}
        return self.state.live_executor.kill()

    def live_flatten(self, trace_id: str | None = None) -> dict[str, Any]:
        if self.state.live_executor is None:
            return {"ok": False, "reason": "live executor not configured", "orders": []}
        prices = {symbol: self.state.quote_for(symbol).price for symbol in self.state.engine.positions}
        records = self.state.live_executor.flatten(
            self.state.engine.positions,
            prices,
            idempotency_prefix=f"flatten-{self.state.next_trace_id('POST', '/live/flatten')}",
            trace_id=trace_id,
        )
        return {"ok": True, "orders": [live_response_from_record(record) for record in records]}

    def coins_tradeable(self) -> int:
        if not self.state.use_live_hyperliquid_prices:
            return len(self.state.prices)
        if self.state.price_cache is not None:
            return len(self.state.price_cache.mids)
        return 0


def position_to_wire(position: Position, quote: PriceQuote) -> dict[str, Any]:
    unrealized_pnl = (quote.price - position.avg_price) * position.quantity
    return {
        "symbol": position.symbol,
        "side": "long" if position.quantity > 0 else "short",
        "size": abs(position.quantity),
        "entry": position.avg_price,
        "mark": quote.price,
        "unrealized_pnl": round(unrealized_pnl, 2),
        "unrealized_r": 0.0,
        "age_s": 0.0,
    }


def rejection_to_wire(record: DecisionRecord) -> dict[str, Any]:
    payload = {
        "coin": record.intent.symbol,
        "direction": record.intent.side.value,
        "stage": "risk",
        "reason": record.decision.reason,
        "ts": epoch_to_iso(record.as_of),
    }
    if record.trace_id:
        payload["trace_id"] = record.trace_id
    return payload


def execute_response_from_record(
    record: DecisionRecord,
    *,
    include_trace: bool = False,
) -> dict[str, Any]:
    key = record.idempotency_key or ""
    payload = {
        "accepted": record.decision.allowed,
        "simulated": True,
        "fill_id": f"paper-{key[:8]}" if record.decision.allowed and key else None,
        "coin": record.intent.symbol,
        "side": record.intent.side.value,
        "size": record.intent.quantity,
        "reason": record.decision.reason,
    }
    if include_trace and record.trace_id:
        payload["trace_id"] = record.trace_id
    return payload


def live_response(
    *,
    accepted: bool,
    reason: str,
    intent: OrderIntent,
    key: str,
    trace_id: str | None = None,
) -> dict[str, Any]:
    payload = {
        "accepted": accepted,
        "simulated": False,
        "fill_id": None,
        "coin": intent.symbol,
        "side": intent.side.value,
        "size": intent.quantity,
        "reason": reason,
        "idempotency_key": key,
        "status": "submitted" if accepted else "refused",
    }
    if trace_id:
        payload["trace_id"] = trace_id
    return payload


def live_response_from_record(record: Any, *, include_trace: bool = False) -> dict[str, Any]:
    payload = {
        "accepted": record.accepted,
        "simulated": False,
        "fill_id": None,
        "coin": record.symbol,
        "side": record.side,
        "size": record.quantity,
        "reason": record.reason,
        "idempotency_key": record.idempotency_key,
        "status": record.status,
    }
    if include_trace and record.trace_id:
        payload["trace_id"] = record.trace_id
    return payload


def first(query: dict[str, list[str]], name: str) -> str | None:
    values = query.get(name)
    return values[0] if values else None


def epoch_to_iso(value: float) -> str:
    return datetime.fromtimestamp(value, UTC).isoformat().replace("+00:00", "Z")


def make_handler(api: PaperApi) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        server_version = "zero-paper-api/0.1"
        protocol_version = "HTTP/1.1"

        def log_message(self, format: str, *args: object) -> None:
            return

        def do_GET(self) -> None:
            parsed = urlparse(self.path)
            trace_id = api.state.next_trace_id("GET", parsed.path)
            if parsed.path == "/ws":
                self.accept_websocket(trace_id)
                return
            status, payload = api.get(parsed.path, parse_qs(parsed.query), trace_id=trace_id)
            self.write_json(status, payload, trace_id=trace_id)

        def do_POST(self) -> None:
            parsed = urlparse(self.path)
            trace_id = api.state.next_trace_id("POST", parsed.path)
            try:
                length = int(self.headers.get("content-length", "0"))
                body = self.rfile.read(length).decode("utf-8") if length else "{}"
                payload = json.loads(body)
                status, response = api.post(
                    parsed.path,
                    payload,
                    trace_id=trace_id,
                    expose_trace=True,
                    mode=self.headers.get("x-zero-mode"),
                )
            except (ValueError, TypeError, json.JSONDecodeError) as exc:
                status, response = HTTPStatus.BAD_REQUEST, {"error": str(exc)}
                api.state.record_request(
                    method="POST",
                    path=parsed.path,
                    status=status,
                    elapsed_ms=0.0,
                    trace_id=trace_id,
                )
            self.write_json(status, response, trace_id=trace_id)

        def write_json(
            self,
            status: int,
            payload: dict[str, Any],
            trace_id: str | None = None,
        ) -> None:
            body = json.dumps(payload, sort_keys=True).encode("utf-8")
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            if trace_id:
                self.send_header("x-zero-trace-id", trace_id)
            self.end_headers()
            self.wfile.write(body)

        def accept_websocket(self, trace_id: str) -> None:
            key = self.headers.get("Sec-WebSocket-Key")
            if not key:
                self.write_json(
                    HTTPStatus.BAD_REQUEST,
                    {"error": "missing websocket key"},
                    trace_id=trace_id,
                )
                return
            accept = websocket_accept_key(key)
            self.send_response(HTTPStatus.SWITCHING_PROTOCOLS)
            self.send_header("Upgrade", "websocket")
            self.send_header("Connection", "Upgrade")
            self.send_header("Sec-WebSocket-Accept", accept)
            self.send_header("x-zero-trace-id", trace_id)
            self.end_headers()
            payload = {
                "event": "heartbeat",
                "ts": api.state.now_iso(),
                "data": {"mode": "paper", "source": "zero-paper-api", "trace_id": trace_id},
            }
            self.wfile.write(websocket_text_frame(json.dumps(payload, sort_keys=True)))
            self.wfile.flush()
            time.sleep(0.2)

    return Handler


def websocket_accept_key(key: str) -> str:
    seed = f"{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11".encode("ascii")
    return base64.b64encode(hashlib.sha1(seed).digest()).decode("ascii")


def websocket_text_frame(text: str) -> bytes:
    payload = text.encode("utf-8")
    length = len(payload)
    if length < 126:
        return bytes([0x81, length]) + payload
    if length < 65_536:
        return bytes([0x81, 126]) + length.to_bytes(2, "big") + payload
    return bytes([0x81, 127]) + length.to_bytes(8, "big") + payload


def serve(
    host: str = "127.0.0.1",
    port: int = 8765,
    journal_path: str | None = None,
    *,
    hyperliquid: bool = False,
    hyperliquid_live_prices: bool = False,
) -> None:
    engine = (
        PaperEngine.recover_from_journal(DecisionJournal(journal_path))
        if journal_path
        else PaperEngine()
    )
    hl_client = HyperliquidInfoClient() if hyperliquid or hyperliquid_live_prices else None
    dead_man_timeout_s = parse_float_env("ZERO_LIVE_DEAD_MAN_TIMEOUT_S", 30.0)
    live_executor = build_live_executor(dead_man_timeout_s)
    server = ThreadingHTTPServer(
        (host, port),
        make_handler(
            PaperApi(
                PaperApiState(
                    engine=engine,
                    hyperliquid=hl_client,
                    use_live_hyperliquid_prices=hyperliquid_live_prices,
                    live_wallet_address=os.environ.get("ZERO_HYPERLIQUID_WALLET_ADDRESS"),
                    live_api_private_key=os.environ.get("ZERO_HYPERLIQUID_API_PRIVATE_KEY"),
                    live_kill_switch_path=os.environ.get("ZERO_LIVE_KILL_SWITCH_PATH"),
                    live_dead_man_timeout_s=dead_man_timeout_s,
                    live_executor=live_executor,
                    network_handle=os.environ.get("ZERO_NETWORK_HANDLE", "local-operator"),
                    network_display_name=os.environ.get("ZERO_NETWORK_DISPLAY_NAME"),
                    network_publish_enabled=parse_bool_env("ZERO_NETWORK_PUBLISH_ENABLED", False),
                    network_publish_path=os.environ.get("ZERO_NETWORK_PUBLISH_PATH"),
                    intelligence_public_delay_s=parse_int_env(
                        "ZERO_INTELLIGENCE_PUBLIC_DELAY_S",
                        900,
                    ),
                    intelligence_export_path=os.environ.get("ZERO_INTELLIGENCE_EXPORT_PATH"),
                )
            )
        ),
    )
    print(f"zero paper API listening on http://{host}:{port}", flush=True)
    server.serve_forever()


def parse_float_env(name: str, default: float) -> float:
    raw = os.environ.get(name)
    if raw is None:
        return default
    try:
        parsed = float(raw)
    except ValueError:
        return default
    return parsed if parsed > 0 else default


def parse_int_env(name: str, default: int) -> int:
    raw = os.environ.get(name)
    if raw is None:
        return default
    try:
        parsed = int(raw)
    except ValueError:
        return default
    return parsed if parsed >= 0 else default


def parse_bool_env(name: str, default: bool) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return default
    return raw.lower() in {"1", "true", "yes", "on"}


def build_live_executor(dead_man_timeout_s: float) -> LiveExecutor | None:
    if os.environ.get("ZERO_LIVE_EXECUTION_ENABLED", "").lower() not in {"1", "true", "yes"}:
        return None
    wallet = os.environ.get("ZERO_HYPERLIQUID_WALLET_ADDRESS")
    key = os.environ.get("ZERO_HYPERLIQUID_API_PRIVATE_KEY")
    if not wallet or not key:
        raise RuntimeError("live execution requires ZERO_HYPERLIQUID_WALLET_ADDRESS and key")
    policy = LiveExecutionPolicy(
        max_notional_usd=parse_float_env("ZERO_LIVE_MAX_NOTIONAL_USD", 1_000.0),
        max_daily_loss_usd=parse_float_env("ZERO_LIVE_MAX_DAILY_LOSS_USD", 250.0),
        max_orders_per_minute=int(parse_float_env("ZERO_LIVE_MAX_ORDERS_PER_MINUTE", 6)),
        dead_man_timeout_s=dead_man_timeout_s,
    )
    adapter = HyperliquidSdkAdapter(wallet_address=wallet, private_key=key)
    executor = LiveExecutor(adapter=adapter, policy=policy, enabled=True)
    try:
        heartbeat = executor.heartbeat()
        if not heartbeat.get("ok"):
            print(f"zero live executor heartbeat failed during startup: {heartbeat}", file=sys.stderr)
    except Exception as exc:
        print(f"zero live executor heartbeat failed during startup: {exc}", file=sys.stderr)
    return executor


def main() -> None:
    parser = argparse.ArgumentParser(description="Run the local ZERO paper engine API")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--journal", help="Append paper decisions to this JSONL journal")
    parser.add_argument(
        "--hyperliquid",
        action="store_true",
        help="Enable read-only Hyperliquid public market data endpoints",
    )
    parser.add_argument(
        "--hyperliquid-live-prices",
        action="store_true",
        help="Use read-only Hyperliquid mids for paper quotes and paper execution; implies --hyperliquid",
    )
    args = parser.parse_args()
    serve(
        args.host,
        args.port,
        args.journal,
        hyperliquid=args.hyperliquid,
        hyperliquid_live_prices=args.hyperliquid_live_prices,
    )


if __name__ == "__main__":
    main()
