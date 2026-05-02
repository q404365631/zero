from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import sys
import time
from collections.abc import Mapping
from dataclasses import dataclass, field
from datetime import UTC, datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable
from urllib.parse import parse_qs, urlparse

from zero_engine.deployment import DeploymentIdentityConfig, deployment_claim, deployment_heartbeat
from zero_engine.hyperliquid import (
    HyperliquidInfoClient,
    HyperliquidMarketStatus,
    is_hex_address,
    is_private_key,
    redact_secret,
    validate_dry_run_order,
)
from zero_engine.immune import build_immune_report
from zero_engine.intelligence import (
    IntelligenceConfig,
    export_intelligence_snapshot,
    intelligence_catalog,
    intelligence_snapshot,
)
from zero_engine.journal import DecisionJournal
from zero_engine.live import HyperliquidSdkAdapter, LiveExecutionPolicy, LiveExecutor
from zero_engine.live_certification import run_live_certification
from zero_engine.model_gateway import ModelGateway, ModelGatewayConfig
from zero_engine.models import OrderIntent, Position, Side
from zero_engine.network import (
    PublicProfileConfig,
    public_leaderboard,
    public_profile,
    publish_profile,
)
from zero_engine.paper import DecisionRecord, PaperEngine
from zero_engine.reconciliation import (
    ReconciliationReport,
    local_account_positions,
    reconcile_positions,
)
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


def safe_context_value(value: Any, *, default: str, max_len: int = 80) -> str:
    raw = str(value or "").strip()
    if not raw:
        return default
    safe = "".join(ch for ch in raw if ch.isalnum() or ch in {"-", "_", ".", ":", "@"})
    return (safe or default)[:max_len]


@dataclass(frozen=True)
class OperatorContext:
    operator_id: str = "local-operator"
    handle: str = "local-operator"
    role: str = "owner"
    scope: str = "local-private"
    source: str = "runtime-default"

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": "zero.operator_context.v1",
            "operator_id": self.operator_id,
            "handle": self.handle,
            "role": self.role,
            "scope": self.scope,
            "source": self.source,
        }


@dataclass(frozen=True)
class OperatorActionRecord:
    action: str
    risk_direction: str
    ok: bool
    operator_context: OperatorContext
    as_of: str
    trace_id: str | None = None
    state: str | None = None
    reason: str | None = None

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "action": self.action,
            "risk_direction": self.risk_direction,
            "ok": self.ok,
            "as_of": self.as_of,
            "operator_context": self.operator_context.to_dict(),
        }
        if self.trace_id:
            payload["trace_id"] = self.trace_id
        if self.state:
            payload["state"] = self.state
        if self.reason:
            payload["reason"] = self.reason
        return payload


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
    reconciliation_stale_after_s: float = 10.0
    live_executor: LiveExecutor | None = None
    network_handle: str = "local-operator"
    network_display_name: str | None = None
    network_publish_enabled: bool = False
    network_publish_path: str | None = None
    deployment_id: str = "local-paper"
    deployment_kind: str = "local"
    deployment_environment: str = "paper"
    deployment_owner: str = "local-operator"
    deployment_version: str = "0.1.1"
    deployment_public_key: str | None = None
    deployment_signature: str | None = None
    deployment_signer: str | None = None
    deployment_heartbeat_public_key: str | None = None
    deployment_heartbeat_signature: str | None = None
    deployment_heartbeat_signer: str | None = None
    intelligence_public_delay_s: int = 900
    intelligence_export_path: str | None = None
    model_gateway_provider: str = "none"
    model_gateway_model: str | None = None
    model_gateway_mock_enabled: bool = False
    model_gateway_allow_network: bool = False
    model_gateway_configured_providers: frozenset[str] = frozenset()
    model_gateway_provider_credentials: Mapping[str, str] = field(default_factory=dict, repr=False)
    model_gateway_provider_endpoints: Mapping[str, str] = field(default_factory=dict)
    model_gateway_instance: ModelGateway | None = None
    default_operator_id: str = "local-operator"
    default_operator_handle: str = "local-operator"
    default_operator_role: str = "owner"
    default_operator_scope: str = "local-private"
    default_operator_source: str = "runtime-default"
    operator_action_log: list[OperatorActionRecord] = field(default_factory=list)

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

    def hyperliquid_account_snapshot(self) -> Any:
        if self.hyperliquid is None:
            raise ValueError("start with --hyperliquid to verify account state")
        if not self.live_wallet_address or not is_hex_address(self.live_wallet_address):
            raise ValueError("valid ZERO_HYPERLIQUID_WALLET_ADDRESS required before account read")
        return self.hyperliquid.account_snapshot(self.live_wallet_address)

    def reconcile_hyperliquid_account(self) -> ReconciliationReport:
        local_positions = local_account_positions(self.engine.positions)
        if self.hyperliquid is None or not self.live_wallet_address or not is_hex_address(self.live_wallet_address):
            return reconcile_positions(
                local_positions=local_positions,
                exchange_snapshot=None,
                as_of=self.now(),
                stale_after_s=self.reconciliation_stale_after_s,
            )
        snapshot = self.hyperliquid_account_snapshot()
        return reconcile_positions(
            local_positions=local_positions,
            exchange_snapshot=snapshot,
            as_of=self.now(),
            stale_after_s=self.reconciliation_stale_after_s,
        )

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

    def operator_context(
        self,
        headers: Mapping[str, str] | None = None,
    ) -> OperatorContext:
        normalized = {key.lower(): value for key, value in (headers or {}).items()}
        has_header_context = any(
            key in normalized
            for key in {
                "x-zero-operator-id",
                "x-zero-operator-handle",
                "x-zero-operator-role",
                "x-zero-operator-scope",
            }
        )
        handle = safe_context_value(
            normalized.get("x-zero-operator-handle", self.default_operator_handle),
            default="local-operator",
        )
        return OperatorContext(
            operator_id=safe_context_value(
                normalized.get("x-zero-operator-id", self.default_operator_id),
                default=handle,
            ),
            handle=handle,
            role=safe_context_value(
                normalized.get("x-zero-operator-role", self.default_operator_role),
                default="operator",
                max_len=32,
            ),
            scope=safe_context_value(
                normalized.get("x-zero-operator-scope", self.default_operator_scope),
                default="local-private",
                max_len=40,
            ),
            source="request-header" if has_header_context else self.default_operator_source,
        )

    def record_operator_action(
        self,
        *,
        action: str,
        risk_direction: str,
        result: dict[str, Any],
        trace_id: str | None,
        operator_context: OperatorContext,
    ) -> None:
        self.operator_action_log.append(
            OperatorActionRecord(
                action=action,
                risk_direction=risk_direction,
                ok=bool(result.get("ok")),
                state=str(result["state"]) if result.get("state") is not None else None,
                reason=str(result["reason"]) if result.get("reason") is not None else None,
                trace_id=trace_id,
                as_of=self.now_iso(),
                operator_context=operator_context,
            )
        )


class PaperApi:
    def __init__(self, state: PaperApiState | None = None) -> None:
        self.state = state or PaperApiState()

    def get(
        self,
        path: str,
        query: dict[str, list[str]],
        trace_id: str | None = None,
        operator_context: OperatorContext | None = None,
    ) -> tuple[int, dict[str, Any]]:
        trace_id = trace_id or self.state.next_trace_id("GET", path)
        operator_context = operator_context or self.state.operator_context()
        started = time.perf_counter()
        status, payload = self._get(path, query, trace_id, operator_context)
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
        operator_context: OperatorContext,
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
            "/audit/export": lambda: self.audit_export(query, operator_context),
            "/hl/account": self.hl_account,
            "/hl/reconcile": self.hl_reconcile,
            "/hl/status": lambda: self.hl_status(query),
            "/immune": self.immune,
            "/live/cockpit": lambda: self.live_cockpit(operator_context),
            "/intelligence/catalog": self.intelligence_catalog,
            "/intelligence/model-gateway": self.intelligence_model_gateway,
            "/intelligence/snapshot": self.intelligence_snapshot,
            "/live/certification": self.live_certification,
            "/live/preflight": self.live_preflight,
            "/market/quote": lambda: self.market_quote(query),
            "/metrics": self.metrics,
            "/deployment/claim": lambda: self.deployment_claim(operator_context),
            "/deployment/heartbeat": lambda: self.deployment_heartbeat(operator_context),
            "/network/profile": self.network_profile,
            "/network/leaderboard": self.network_leaderboard,
            "/operator/state": self.operator_state,
            "/operator/context": lambda: self.operator_context_payload(operator_context),
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
        operator_context: OperatorContext | None = None,
    ) -> tuple[int, dict[str, Any]]:
        trace_id = trace_id or self.state.next_trace_id("POST", path)
        operator_context = operator_context or self.state.operator_context()
        started = time.perf_counter()
        status, response = self._post(
            path,
            payload,
            trace_id,
            expose_trace=expose_trace,
            mode=mode,
            operator_context=operator_context,
        )
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
        operator_context: OperatorContext,
    ) -> tuple[int, dict[str, Any]]:
        if path == "/execute":
            try:
                if mode == "live":
                    return HTTPStatus.OK, self.live_execute(
                        payload,
                        trace_id=trace_id,
                        expose_trace=expose_trace,
                        operator_context=operator_context,
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
                "operator_context": operator_context.to_dict(),
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
            return HTTPStatus.OK, self.live_heartbeat(trace_id, operator_context)
        if path == "/live/pause":
            return HTTPStatus.OK, self.live_pause(trace_id, operator_context)
        if path == "/live/resume":
            return HTTPStatus.OK, self.live_resume(trace_id, operator_context)
        if path == "/live/kill":
            return HTTPStatus.OK, self.live_kill(trace_id, operator_context)
        if path == "/live/flatten":
            return HTTPStatus.OK, self.live_flatten(trace_id=trace_id, operator_context=operator_context)
        return HTTPStatus.NOT_FOUND, {"error": "not found", "path": path}

    def root(self) -> dict[str, Any]:
        return {
            "name": "zero-paper-engine",
            "version": "0.1.1",
            "status": "ok",
            "ts": self.state.now_iso(),
        }

    def health(self) -> dict[str, Any]:
        ts = self.state.now_iso()
        exchange = "hyperliquid" if self.state.hyperliquid is not None else "paper"
        market_data = "live" if self.state.use_live_hyperliquid_prices else "fixture"
        recovery = self.recovery()
        immune = self.immune()
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
                breaker["name"]: breaker["status"] for breaker in immune["breakers"]
            },
            "risk": {"equity": 10_000.0, "drawdown_pct": 0.0, "kill_all": False},
            "recovery": recovery,
            "immune": immune,
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
                account = self.state.hyperliquid_account_snapshot()
                add(
                    "account_read",
                    "ok",
                    "account snapshot read ok · "
                    f"positions={len(account.positions)} open_orders={len(account.open_orders)}",
                )
            except Exception as exc:
                add("account_read", "fail", f"Hyperliquid account read failed: {exc}")

        try:
            reconciliation = self.state.reconcile_hyperliquid_account()
            add(
                "reconciliation",
                "ok" if reconciliation.risk_increasing_allowed else "fail",
                reconciliation.reason,
                status_code=reconciliation.status,
            )
        except Exception as exc:
            reconciliation = None
            add("reconciliation", "fail", f"Hyperliquid reconciliation failed: {exc}")

        immune = self.immune(reconciliation=reconciliation)
        open_breakers = [
            breaker["name"]
            for breaker in immune["breakers"]
            if breaker["blocks_risk"]
        ]
        add(
            "immune_breakers",
            "ok" if immune["risk_increasing_allowed"] else "fail",
            "all risk-blocking breakers closed"
            if immune["risk_increasing_allowed"]
            else "risk-blocking breakers open: " + ", ".join(open_breakers),
            risk_increasing_allowed=immune["risk_increasing_allowed"],
        )

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

        controls_ready = all(
            check["status"] == "ok"
            for check in checks
            if check["name"] not in {"live_executor", "immune_breakers"}
        )
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
            "immune": immune,
        }

    def live_certification(self) -> dict[str, Any]:
        return run_live_certification().to_dict()

    def live_cockpit(self, operator_context: OperatorContext | None = None) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        preflight = self.live_preflight()
        try:
            reconciliation = self.state.reconcile_hyperliquid_account()
            reconciliation_payload = {
                "schema_version": reconciliation.to_dict()["schema_version"],
                "status": reconciliation.status,
                "risk_increasing_allowed": reconciliation.risk_increasing_allowed,
                "reason": reconciliation.reason,
                "drifts": len(reconciliation.drifts),
            }
        except Exception as exc:
            reconciliation_payload = {
                "schema_version": "zero.reconciliation.v1",
                "status": "error",
                "risk_increasing_allowed": False,
                "reason": f"Hyperliquid reconciliation failed: {exc}",
                "drifts": 0,
            }
        immune = preflight["immune"]
        certification = self.live_certification()
        failed_checks = [check for check in preflight["checks"] if check["status"] != "ok"]
        open_breakers = [breaker for breaker in immune["breakers"] if breaker["blocks_risk"]]
        failed_drills = [drill for drill in certification["drills"] if drill["status"] != "pass"]
        executor = self.state.live_executor
        records = list(executor.records if executor is not None else [])
        record_statuses = [record.status for record in records]

        if failed_checks:
            first = failed_checks[0]
            next_action = f"fix preflight check {first['name']}: {first['note']}"
        elif open_breakers:
            first = open_breakers[0]
            next_action = f"close immune breaker {first['name']}: {first['reason']}"
        elif not certification["passed"] or not certification["live_start_certified"]:
            next_action = "rerun live certification and resolve failed drills"
        else:
            next_action = "ready for operator-owned tiny-capital canary; capture the evidence bundle"

        return {
            "schema_version": "zero.live_cockpit.v1",
            "generated_at": self.state.now_iso(),
            "mode": preflight["mode"],
            "live_mode": preflight["live_mode"],
            "ready": preflight["ready"],
            "controls_ready": preflight["controls_ready"],
            "risk_increasing_allowed": bool(
                preflight["ready"]
                and immune["risk_increasing_allowed"]
                and certification["passed"]
                and certification["live_start_certified"]
            ),
            "next_action": next_action,
            "operator_context": operator_context.to_dict(),
            "access_policy": {
                "identity_required_for_live_controls": True,
                "default_scope": "local-private",
                "header_overrides": [
                    "X-Zero-Operator-Id",
                    "X-Zero-Operator-Handle",
                    "X-Zero-Operator-Role",
                    "X-Zero-Operator-Scope",
                ],
            },
            "preflight": {
                "schema_version": preflight["schema_version"],
                "ready": preflight["ready"],
                "live_mode": preflight["live_mode"],
                "controls_ready": preflight["controls_ready"],
                "summary": {
                    "total": len(preflight["checks"]),
                    "passed": len(preflight["checks"]) - len(failed_checks),
                    "failed": len(failed_checks),
                },
                "failed_checks": failed_checks,
            },
            "immune": {
                "schema_version": immune["schema_version"],
                "risk_increasing_allowed": immune["risk_increasing_allowed"],
                "summary": immune["summary"],
                "open_breakers": open_breakers,
            },
            "reconciliation": reconciliation_payload,
            "certification": {
                "schema_version": certification["schema_version"],
                "mode": certification["mode"],
                "passed": certification["passed"],
                "live_start_certified": certification["live_start_certified"],
                "summary": certification["summary"],
                "failed_drills": failed_drills,
            },
            "heartbeat": {
                "configured": executor is not None,
                "expired": True if executor is None else executor.dead_man_expired(),
                "last_heartbeat_at": None if executor is None else executor.last_heartbeat_at,
                "timeout_s": None if executor is None else executor.policy.dead_man_timeout_s,
            },
            "live_records": {
                "total": len(records),
                "accepted": len([record for record in records if record.accepted]),
                "refused": record_statuses.count("refused"),
                "exchange_error": record_statuses.count("exchange_error"),
                "recent": [record.to_dict() for record in records[-5:]],
            },
            "operator_actions": {
                "risk_reducing": ["/pause-entries", "/kill", "/flatten-all"],
                "risk_increasing": ["/resume-entries"],
                "read_only": ["/live-cockpit", "/live-certify", "/immune", "/hl-reconcile"],
                "recent": [
                    record.to_dict()
                    for record in self.state.operator_action_log[-10:]
                ],
            },
        }

    def immune(self, reconciliation: ReconciliationReport | None = None) -> dict[str, Any]:
        if reconciliation is None:
            try:
                reconciliation = self.state.reconcile_hyperliquid_account()
            except Exception:
                reconciliation = None
        return build_immune_report(
            generated_at=self.state.now_iso(),
            mode="live-capable" if self.state.live_executor is not None else "paper",
            live_executor=self.state.live_executor,
            market_data_age_s=self.market_data_age_s(),
            market_data_stale_after_s=self.state.price_cache_ttl_s,
            reconciliation=reconciliation,
            positions=self.state.engine.positions,
            max_exposure_usd=self.state.engine.limits.max_position_notional_usd,
        ).to_dict()

    def market_data_age_s(self) -> float | None:
        if not self.state.use_live_hyperliquid_prices:
            return None
        if self.state.price_cache_at is None:
            return self.state.price_cache_ttl_s + 1
        return max(0.0, (self.state.now() - self.state.price_cache_at).total_seconds())

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
            "immune": self.immune(),
            "recovery": self.recovery(),
        }

    def audit_export(
        self,
        query: dict[str, list[str]],
        operator_context: OperatorContext | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        limit = int(first(query, "limit") or "100")
        generated_at = self.state.now_iso()
        claim = self.deployment_claim(operator_context, generated_at=generated_at)
        heartbeat = self.deployment_heartbeat(
            operator_context,
            generated_at=generated_at,
            deployment_claim_packet=claim,
        )
        if self.state.engine.journal is not None:
            decisions = self.state.engine.journal.tail(limit)
            source = "journal"
        else:
            decisions = [record.to_dict() for record in self.state.engine.decisions[-limit:]]
            source = "memory"
        return {
            "schema_version": "zero.audit.v1",
            "exported_at": generated_at,
            "mode": "paper",
            "source": source,
            "limit": limit,
            "operator_context": operator_context.to_dict(),
            "summary": {
                "decisions": len(self.state.engine.decisions),
                "fills": len(self.state.engine.fills),
                "rejections": len(self.state.engine.rejections),
                "open_positions": len(
                    [p for p in self.state.engine.positions.values() if p.quantity != 0]
                ),
            },
            "immune": self.immune(),
            "retention": {
                "policy": "operator-managed",
                "redaction": "no secrets are recorded by the public paper runtime",
                "format": "append-only-jsonl",
            },
            "metrics": self.metrics(),
            "recovery": self.recovery(),
            "deployment_claim": claim,
            "deployment_heartbeat": heartbeat,
            "operator_actions": [
                record.to_dict()
                for record in self.state.operator_action_log[-limit:]
            ],
            "decisions": decisions,
        }

    def network_profile(self) -> dict[str, Any]:
        generated_at = self.state.now_iso()
        claim = self.deployment_claim(generated_at=generated_at)
        heartbeat = self.deployment_heartbeat(generated_at=generated_at, deployment_claim_packet=claim)
        return public_profile(
            self.state.engine,
            config=self.network_config(),
            generated_at=generated_at,
            mode=self.network_mode(),
            live_execution_count=self.live_execution_count(),
            deployment_claim=claim,
            deployment_heartbeat=heartbeat,
        )

    def network_leaderboard(self) -> dict[str, Any]:
        profile = self.network_profile()
        return {
            **public_leaderboard([profile], generated_at=self.state.now_iso()),
            "mode": profile["mode"],
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
        generated_at = self.state.now_iso()
        claim = self.deployment_claim(generated_at=generated_at)
        heartbeat = self.deployment_heartbeat(generated_at=generated_at, deployment_claim_packet=claim)
        profile = public_profile(
            self.state.engine,
            config=config,
            generated_at=generated_at,
            mode=self.network_mode(),
            live_execution_count=self.live_execution_count(),
            deployment_claim=claim,
            deployment_heartbeat=heartbeat,
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

    def deployment_config(self) -> DeploymentIdentityConfig:
        return DeploymentIdentityConfig(
            deployment_id=self.state.deployment_id,
            deployment_kind=self.state.deployment_kind,
            environment=self.state.deployment_environment,
            owner=self.state.deployment_owner,
            version=self.state.deployment_version,
            public_key=self.state.deployment_public_key,
            signature=self.state.deployment_signature,
            signer=self.state.deployment_signer,
            heartbeat_public_key=self.state.deployment_heartbeat_public_key,
            heartbeat_signature=self.state.deployment_heartbeat_signature,
            heartbeat_signer=self.state.deployment_heartbeat_signer,
        )

    def deployment_claim(
        self,
        operator_context: OperatorContext | None = None,
        *,
        generated_at: str | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        engine = self.state.engine
        return deployment_claim(
            config=self.deployment_config(),
            generated_at=generated_at or self.state.now_iso(),
            operator_context=operator_context.to_dict(),
            runtime={
                "mode": self.network_mode(),
                "market_source": self.state.market_source(),
                "journal_durable": engine.recovery.durable or engine.journal is not None,
                "live_executor_configured": self.state.live_executor is not None,
            },
            evidence={
                "decisions": len(engine.decisions),
                "fills": len(engine.fills),
                "rejections": len(engine.rejections),
                "live_execution_count": self.live_execution_count(),
            },
        )

    def deployment_heartbeat(
        self,
        operator_context: OperatorContext | None = None,
        *,
        generated_at: str | None = None,
        deployment_claim_packet: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        generated_at = generated_at or self.state.now_iso()
        operator_context = operator_context or self.state.operator_context()
        claim = deployment_claim_packet or self.deployment_claim(
            operator_context,
            generated_at=generated_at,
        )
        executor = self.state.live_executor
        now_ts = self.state.now().timestamp()
        if executor is None:
            liveness = {
                "status": "paper_only",
                "live_executor_configured": False,
                "dead_man_expired": True,
                "last_live_heartbeat_at": None,
                "dead_man_timeout_s": None,
                "next_required_within_s": None,
            }
        else:
            last = executor.last_heartbeat_at
            timeout = executor.policy.dead_man_timeout_s
            next_required_within_s = None if last is None else max(0.0, round(last + timeout - now_ts, 3))
            expired = executor.dead_man_expired()
            liveness = {
                "status": "expired" if expired else "fresh",
                "live_executor_configured": True,
                "dead_man_expired": expired,
                "last_live_heartbeat_at": last,
                "dead_man_timeout_s": timeout,
                "next_required_within_s": next_required_within_s,
            }
        return deployment_heartbeat(
            config=self.deployment_config(),
            generated_at=generated_at,
            deployment_claim_hash=str(claim["claim_hash"]),
            operator_context=operator_context.to_dict(),
            runtime={
                "mode": self.network_mode(),
                "market_source": self.state.market_source(),
                "journal_durable": self.state.engine.recovery.durable
                or self.state.engine.journal is not None,
            },
            liveness=liveness,
        )

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

    def intelligence_model_gateway(self) -> dict[str, Any]:
        return self.model_gateway().status(generated_at=self.state.now_iso())

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

    def model_gateway(self) -> ModelGateway:
        if self.state.model_gateway_instance is None:
            self.state.model_gateway_instance = ModelGateway(
                ModelGatewayConfig(
                    provider=self.state.model_gateway_provider,
                    model=self.state.model_gateway_model,
                    mock_enabled=self.state.model_gateway_mock_enabled,
                    allow_network=self.state.model_gateway_allow_network,
                    configured_providers=self.state.model_gateway_configured_providers,
                    provider_credentials=self.state.model_gateway_provider_credentials,
                    provider_endpoints=self.state.model_gateway_provider_endpoints,
                )
            )
        return self.state.model_gateway_instance

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

    def hl_account(self) -> dict[str, Any]:
        return self.state.hyperliquid_account_snapshot().to_dict()

    def hl_reconcile(self) -> dict[str, Any]:
        return self.state.reconcile_hyperliquid_account().to_dict()

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

    def operator_context_payload(self, operator_context: OperatorContext) -> dict[str, Any]:
        return {
            **operator_context.to_dict(),
            "generated_at": self.state.now_iso(),
            "identity_policy": {
                "live_controls": "required",
                "secrets": "never_recorded",
                "audit_scope": "operator_context plus trace_id",
            },
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
        operator_context: OperatorContext | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
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
                operator_context=operator_context,
            )
        if not intent.reduce_only:
            reconciliation = self.state.reconcile_hyperliquid_account()
            if not reconciliation.risk_increasing_allowed:
                self.state.metrics.record_execute(accepted=False)
                return live_response(
                    accepted=False,
                    reason=f"reconciliation {reconciliation.status}: {reconciliation.reason}",
                    intent=intent,
                    key=key,
                    trace_id=trace_id if expose_trace else None,
                    operator_context=operator_context,
                )
            immune = self.immune(reconciliation=reconciliation)
            if not immune["risk_increasing_allowed"]:
                open_breakers = [
                    breaker["name"]
                    for breaker in immune["breakers"]
                    if breaker["blocks_risk"]
                ]
                reason = "immune breaker open: " + ", ".join(open_breakers)
                if "kill_switch" in open_breakers:
                    reason = "kill switch active"
                elif "operator_pause" in open_breakers:
                    reason = "live entries paused"
                elif "dead_man" in open_breakers:
                    reason = "dead-man switch expired"
                self.state.metrics.record_execute(accepted=False)
                return live_response(
                    accepted=False,
                    reason=reason,
                    intent=intent,
                    key=key,
                    trace_id=trace_id if expose_trace else None,
                    operator_context=operator_context,
                )
        record = executor.submit(
            intent,
            idempotency_key=key,
            trace_id=trace_id,
            operator_context=operator_context.to_dict(),
        )
        self.state.metrics.record_execute(accepted=record.accepted)
        return live_response_from_record(record, include_trace=expose_trace)

    def live_heartbeat(
        self,
        trace_id: str | None = None,
        operator_context: OperatorContext | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        if self.state.live_executor is None:
            result = {"ok": False, "reason": "live executor not configured"}
        else:
            result = self.state.live_executor.heartbeat()
        return self.live_control_result("heartbeat", "neutral", result, trace_id, operator_context)

    def live_pause(
        self,
        trace_id: str | None = None,
        operator_context: OperatorContext | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        if self.state.live_executor is None:
            result = {"ok": False, "reason": "live executor not configured"}
        else:
            result = self.state.live_executor.pause()
        return self.live_control_result("pause_entries", "reduces", result, trace_id, operator_context)

    def live_resume(
        self,
        trace_id: str | None = None,
        operator_context: OperatorContext | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        if self.state.live_executor is None:
            result = {"ok": False, "reason": "live executor not configured"}
        else:
            result = self.state.live_executor.resume()
        return self.live_control_result("resume_entries", "increases", result, trace_id, operator_context)

    def live_kill(
        self,
        trace_id: str | None = None,
        operator_context: OperatorContext | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        if self.state.live_executor is None:
            result = {"ok": False, "reason": "live executor not configured"}
        else:
            result = self.state.live_executor.kill()
        return self.live_control_result("kill", "reduces", result, trace_id, operator_context)

    def live_flatten(
        self,
        trace_id: str | None = None,
        operator_context: OperatorContext | None = None,
    ) -> dict[str, Any]:
        operator_context = operator_context or self.state.operator_context()
        if self.state.live_executor is None:
            result = {"ok": False, "reason": "live executor not configured", "orders": []}
            return self.live_control_result("flatten_all", "reduces", result, trace_id, operator_context)
        prices = {symbol: self.state.quote_for(symbol).price for symbol in self.state.engine.positions}
        records = self.state.live_executor.flatten(
            self.state.engine.positions,
            prices,
            idempotency_prefix=f"flatten-{self.state.next_trace_id('POST', '/live/flatten')}",
            trace_id=trace_id,
            operator_context=operator_context.to_dict(),
        )
        result = {"ok": True, "orders": [live_response_from_record(record) for record in records]}
        return self.live_control_result("flatten_all", "reduces", result, trace_id, operator_context)

    def live_control_result(
        self,
        action: str,
        risk_direction: str,
        result: dict[str, Any],
        trace_id: str | None,
        operator_context: OperatorContext,
    ) -> dict[str, Any]:
        payload = {
            **result,
            "operator_context": operator_context.to_dict(),
            "action": action,
            "risk_direction": risk_direction,
        }
        if trace_id:
            payload["trace_id"] = trace_id
        self.state.record_operator_action(
            action=action,
            risk_direction=risk_direction,
            result=payload,
            trace_id=trace_id,
            operator_context=operator_context,
        )
        return payload

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
    operator_context: OperatorContext | None = None,
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
    if operator_context is not None:
        payload["operator_context"] = operator_context.to_dict()
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
    if getattr(record, "operator_context", None):
        payload["operator_context"] = record.operator_context
    return payload


def first(query: dict[str, list[str]], name: str) -> str | None:
    values = query.get(name)
    return values[0] if values else None


def request_headers(headers: Any) -> dict[str, str]:
    return {str(key).lower(): str(value) for key, value in headers.items()}


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
            operator_context = api.state.operator_context(request_headers(self.headers))
            if parsed.path == "/ws":
                self.accept_websocket(trace_id)
                return
            status, payload = api.get(
                parsed.path,
                parse_qs(parsed.query),
                trace_id=trace_id,
                operator_context=operator_context,
            )
            self.write_json(status, payload, trace_id=trace_id)

        def do_POST(self) -> None:
            parsed = urlparse(self.path)
            trace_id = api.state.next_trace_id("POST", parsed.path)
            operator_context = api.state.operator_context(request_headers(self.headers))
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
                    operator_context=operator_context,
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
    operator_source = (
        "env"
        if any(
            os.environ.get(name)
            for name in {
                "ZERO_OPERATOR_ID",
                "ZERO_OPERATOR_HANDLE",
                "ZERO_OPERATOR_ROLE",
                "ZERO_OPERATOR_SCOPE",
            }
        )
        else "runtime-default"
    )
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
                    deployment_id=os.environ.get("ZERO_DEPLOYMENT_ID", "local-paper"),
                    deployment_kind=os.environ.get("ZERO_DEPLOYMENT_KIND", "local"),
                    deployment_environment=os.environ.get("ZERO_DEPLOYMENT_ENVIRONMENT", "paper"),
                    deployment_owner=os.environ.get("ZERO_DEPLOYMENT_OWNER", "local-operator"),
                    deployment_version=os.environ.get("ZERO_DEPLOYMENT_VERSION", "0.1.1"),
                    deployment_public_key=os.environ.get("ZERO_DEPLOYMENT_PUBLIC_KEY"),
                    deployment_signature=os.environ.get("ZERO_DEPLOYMENT_SIGNATURE"),
                    deployment_signer=os.environ.get("ZERO_DEPLOYMENT_SIGNER"),
                    deployment_heartbeat_public_key=os.environ.get(
                        "ZERO_DEPLOYMENT_HEARTBEAT_PUBLIC_KEY"
                    ),
                    deployment_heartbeat_signature=os.environ.get(
                        "ZERO_DEPLOYMENT_HEARTBEAT_SIGNATURE"
                    ),
                    deployment_heartbeat_signer=os.environ.get("ZERO_DEPLOYMENT_HEARTBEAT_SIGNER"),
                    intelligence_public_delay_s=parse_int_env(
                        "ZERO_INTELLIGENCE_PUBLIC_DELAY_S",
                        900,
                    ),
                    intelligence_export_path=os.environ.get("ZERO_INTELLIGENCE_EXPORT_PATH"),
                    model_gateway_provider=os.environ.get("ZERO_MODEL_PROVIDER", "none"),
                    model_gateway_model=os.environ.get("ZERO_MODEL_NAME"),
                    model_gateway_mock_enabled=parse_bool_env("ZERO_MODEL_MOCK_ENABLED", False),
                    model_gateway_allow_network=parse_bool_env("ZERO_MODEL_ALLOW_NETWORK", False),
                    model_gateway_configured_providers=configured_model_providers(),
                    model_gateway_provider_credentials=model_provider_credentials(),
                    model_gateway_provider_endpoints=model_provider_endpoints(),
                    default_operator_id=os.environ.get("ZERO_OPERATOR_ID", "local-operator"),
                    default_operator_handle=os.environ.get("ZERO_OPERATOR_HANDLE", "local-operator"),
                    default_operator_role=os.environ.get("ZERO_OPERATOR_ROLE", "owner"),
                    default_operator_scope=os.environ.get("ZERO_OPERATOR_SCOPE", "local-private"),
                    default_operator_source=operator_source,
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


def configured_model_providers() -> frozenset[str]:
    providers: set[str] = set()
    if os.environ.get("OPENAI_API_KEY"):
        providers.add("openai")
    if os.environ.get("ANTHROPIC_API_KEY"):
        providers.add("anthropic")
    if os.environ.get("OLLAMA_BASE_URL"):
        providers.add("ollama")
    if os.environ.get("OPENROUTER_API_KEY"):
        providers.add("openrouter")
    return frozenset(providers)


def model_provider_credentials() -> dict[str, str]:
    credentials: dict[str, str] = {}
    if api_key := os.environ.get("OPENAI_API_KEY"):
        credentials["openai"] = api_key
    if api_key := os.environ.get("ANTHROPIC_API_KEY"):
        credentials["anthropic"] = api_key
    if api_key := os.environ.get("OPENROUTER_API_KEY"):
        credentials["openrouter"] = api_key
    return credentials


def model_provider_endpoints() -> dict[str, str]:
    endpoints: dict[str, str] = {}
    if base_url := os.environ.get("OLLAMA_BASE_URL"):
        endpoints["ollama"] = f"{base_url.rstrip('/')}/api/generate"
    if url := os.environ.get("ZERO_OPENAI_BASE_URL"):
        endpoints["openai"] = url
    if url := os.environ.get("ZERO_ANTHROPIC_BASE_URL"):
        endpoints["anthropic"] = url
    if url := os.environ.get("ZERO_OPENROUTER_BASE_URL"):
        endpoints["openrouter"] = url
    return endpoints


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
