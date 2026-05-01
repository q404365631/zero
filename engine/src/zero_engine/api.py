from __future__ import annotations

import argparse
import base64
import hashlib
import json
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import parse_qs, urlparse

from zero_engine.models import OrderIntent, Position, Side
from zero_engine.paper import DecisionRecord, PaperEngine
from zero_engine.safety import evaluate_order


DEFAULT_PRICES = {
    "BTC": 40_500.0,
    "ETH": 2_850.0,
    "SOL": 150.0,
}


@dataclass
class PaperApiState:
    engine: PaperEngine = field(default_factory=PaperEngine)
    prices: dict[str, float] = field(default_factory=lambda: dict(DEFAULT_PRICES))
    started_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    auto_enabled: bool = False
    execution_cache: dict[str, dict[str, Any]] = field(default_factory=dict)

    def now(self) -> datetime:
        return datetime.now(UTC)

    def now_iso(self) -> str:
        return self.now().isoformat().replace("+00:00", "Z")

    def price_for(self, symbol: str) -> float:
        return self.prices.get(symbol.upper(), 100.0)


class PaperApi:
    def __init__(self, state: PaperApiState | None = None) -> None:
        self.state = state or PaperApiState()

    def get(self, path: str, query: dict[str, list[str]]) -> tuple[int, dict[str, Any]]:
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
            "/operator/state": self.operator_state,
        }
        if path.startswith("/evaluate/"):
            return HTTPStatus.OK, self.evaluate(path.removeprefix("/evaluate/"))
        handler = routes.get(path)
        if handler is None:
            return HTTPStatus.NOT_FOUND, {"error": "not found", "path": path}
        return HTTPStatus.OK, handler()

    def post(self, path: str, payload: dict[str, Any]) -> tuple[int, dict[str, Any]]:
        if path == "/execute":
            return HTTPStatus.OK, self.execute(payload)
        if path == "/auto/toggle":
            enabled = bool(payload.get("enabled"))
            self.state.auto_enabled = enabled
            return HTTPStatus.OK, {
                "state": "on" if enabled else "off",
                "simulated": True,
                "reason": None,
            }
        if path == "/operator/events":
            return HTTPStatus.OK, {"accepted": 1, "snapshot": self.operator_state()}
        return HTTPStatus.NOT_FOUND, {"error": "not found", "path": path}

    def root(self) -> dict[str, Any]:
        return {"name": "zero-paper-engine", "version": "0.1.0", "status": "ok", "ts": self.state.now_iso()}

    def health(self) -> dict[str, Any]:
        ts = self.state.now_iso()
        return {
            "status": "ok",
            "components": {
                "paper_engine": {"status": "healthy", "last_seen": ts, "age_s": 0.0},
                "risk": {"status": "healthy", "last_seen": ts, "age_s": 0.0},
            },
            "dependencies": {"exchange": "paper", "secrets": "not_required"},
            "circuit_breakers": {"paper": "closed"},
            "risk": {"equity": 10_000.0, "drawdown_pct": 0.0, "kill_all": False},
            "ws_connections": 0,
        }

    def v2_status(self) -> dict[str, Any]:
        positions = list(self.state.engine.positions.values())
        return {
            "confidence": {"score": 90, "level": "paper"},
            "market": {
                "regime": "PAPER MARKET. Local deterministic demo.",
                "health": 1.0,
                "signal": "stable",
                "prediction": "stable",
                "fear_greed": 50,
                "coins_tradeable": len(self.state.prices),
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
            "ts": self.state.now_iso(),
        }

    def positions(self) -> dict[str, Any]:
        items = [
            position_to_wire(position, self.state.price_for(position.symbol))
            for position in self.state.engine.positions.values()
            if position.quantity != 0
        ]
        return {
            "positions": items,
            "count": len(items),
            "account_value": 10_000.0,
            "total_unrealized_pnl": 0.0,
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

    def evaluate(self, raw_symbol: str) -> dict[str, Any]:
        symbol = raw_symbol.upper()
        price = self.state.price_for(symbol)
        intent = OrderIntent(symbol, Side.BUY, quantity=1 / price, price=price, confidence=0.9)
        decision = evaluate_order(intent, self.state.engine.limits, self.state.engine.positions.get(symbol))
        return {
            "coin": symbol,
            "price": price,
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

    def pulse(self, query: dict[str, list[str]]) -> dict[str, Any]:
        limit = int(first(query, "limit") or "20")
        events = [
            {
                "kind": "decision",
                "coin": record.intent.symbol,
                "message": record.decision.reason,
                "severity": "info" if record.decision.allowed else "warn",
                "ts": epoch_to_iso(record.as_of),
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

    def execute(self, payload: dict[str, Any]) -> dict[str, Any]:
        key = str(payload.get("idempotency_key") or "")
        if key and key in self.state.execution_cache:
            return self.state.execution_cache[key]

        symbol = str(payload.get("coin") or "").upper()
        side = Side(str(payload.get("side") or "").lower())
        quantity = float(payload.get("size") or 0)
        price = self.state.price_for(symbol)
        intent = OrderIntent(symbol, side, quantity=quantity, price=price, confidence=0.9)
        decision = self.state.engine.submit(intent, source="api:/execute")
        response = {
            "accepted": decision.allowed,
            "simulated": True,
            "fill_id": f"paper-{key[:8]}" if decision.allowed and key else None,
            "coin": symbol,
            "side": side.value,
            "size": quantity,
            "reason": decision.reason,
        }
        if key:
            self.state.execution_cache[key] = response
        return response


def position_to_wire(position: Position, mark: float) -> dict[str, Any]:
    return {
        "symbol": position.symbol,
        "side": "long" if position.quantity > 0 else "short",
        "size": abs(position.quantity),
        "entry": position.avg_price,
        "mark": mark,
        "unrealized_pnl": 0.0,
        "unrealized_r": 0.0,
        "age_s": 0.0,
    }


def rejection_to_wire(record: DecisionRecord) -> dict[str, Any]:
    return {
        "coin": record.intent.symbol,
        "direction": record.intent.side.value,
        "stage": "risk",
        "reason": record.decision.reason,
        "ts": epoch_to_iso(record.as_of),
    }


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
            if parsed.path == "/ws":
                self.accept_websocket()
                return
            status, payload = api.get(parsed.path, parse_qs(parsed.query))
            self.write_json(status, payload)

        def do_POST(self) -> None:
            parsed = urlparse(self.path)
            try:
                length = int(self.headers.get("content-length", "0"))
                body = self.rfile.read(length).decode("utf-8") if length else "{}"
                payload = json.loads(body)
                status, response = api.post(parsed.path, payload)
            except (ValueError, TypeError, json.JSONDecodeError) as exc:
                status, response = HTTPStatus.BAD_REQUEST, {"error": str(exc)}
            self.write_json(status, response)

        def write_json(self, status: int, payload: dict[str, Any]) -> None:
            body = json.dumps(payload, sort_keys=True).encode("utf-8")
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def accept_websocket(self) -> None:
            key = self.headers.get("Sec-WebSocket-Key")
            if not key:
                self.write_json(HTTPStatus.BAD_REQUEST, {"error": "missing websocket key"})
                return
            accept = websocket_accept_key(key)
            self.send_response(HTTPStatus.SWITCHING_PROTOCOLS)
            self.send_header("Upgrade", "websocket")
            self.send_header("Connection", "Upgrade")
            self.send_header("Sec-WebSocket-Accept", accept)
            self.end_headers()
            payload = {
                "event": "heartbeat",
                "ts": api.state.now_iso(),
                "data": {"mode": "paper", "source": "zero-paper-api"},
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


def serve(host: str = "127.0.0.1", port: int = 8765) -> None:
    server = ThreadingHTTPServer((host, port), make_handler(PaperApi()))
    print(f"zero paper API listening on http://{host}:{port}", flush=True)
    server.serve_forever()


def main() -> None:
    parser = argparse.ArgumentParser(description="Run the local ZERO paper engine API")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    serve(args.host, args.port)


if __name__ == "__main__":
    main()
