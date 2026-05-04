#!/usr/bin/env python3
"""Build and verify public-safe live trading evidence summaries."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_trading_evidence.v1"
VERIFY_SCHEMA_VERSION = "zero.live_trading_evidence_verify.v1"
FORBIDDEN_KEY_PARTS = (
    "wallet",
    "private",
    "secret",
    "api_key",
    "apikey",
    "signature",
    "oid",
    "order_id",
    "cloid",
    "client_order_id",
    "idempotency",
    "trace_id",
)
FORBIDDEN_TEXT_PATTERNS = {
    "wallet_or_private_key": r"0x[a-fA-F0-9]{32,64}",
    "bearer_token": r"Bearer\s+(?!REDACTED\b)[A-Za-z0-9._~+/=-]+",
    "openai_key": r"sk-[A-Za-z0-9]{16,}",
    "raw_trace": r"trace-[A-Za-z0-9._:-]+",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command")

    build = sub.add_parser("build", help="build a redacted live trading evidence packet")
    build.add_argument("--fills", type=Path, default=None, help="JSON object/list containing fills")
    build.add_argument("--orders", type=Path, default=None, help="JSON object/list containing orders")
    build.add_argument("--trades", type=Path, default=None, help="JSONL or JSON trade journal")
    build.add_argument("--decisions", type=Path, action="append", default=[], help="JSONL decision journal")
    build.add_argument("--reconciliation", type=Path, default=None, help="JSON reconciliation snapshot")
    build.add_argument("--label", default="operator-live-evidence")
    build.add_argument("--source", default="private-runtime-redacted")
    build.add_argument("--output", type=Path, required=True)
    build.add_argument(
        "--include-symbols",
        action="store_true",
        help="Include raw symbols. Defaults to hashed symbols only.",
    )

    verify = sub.add_parser("verify", help="verify a redacted live trading evidence packet")
    verify.add_argument("packet", type=Path)
    verify.add_argument("--forbid-token", action="append", default=[])
    verify.add_argument("--json", action="store_true")

    args = parser.parse_args()
    if args.command is None:
        parser.error("choose build or verify")
    return args


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def sha256_json(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), default=str).encode("utf-8")
    return sha256_bytes(encoded)


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def load_records(path: Path | None, *, list_keys: tuple[str, ...]) -> list[dict[str, Any]]:
    if path is None or not path.is_file():
        return []
    text = path.read_text(encoding="utf-8")
    if not text.strip():
        return []
    if path.suffix == ".jsonl":
        return [
            row
            for row in (json.loads(line) for line in text.splitlines() if line.strip())
            if isinstance(row, dict)
        ]
    payload = json.loads(text)
    if isinstance(payload, list):
        return [row for row in payload if isinstance(row, dict)]
    if isinstance(payload, dict):
        for key in list_keys:
            raw = payload.get(key)
            if isinstance(raw, list):
                return [row for row in raw if isinstance(row, dict)]
        data = payload.get("data")
        if isinstance(data, dict):
            for key in list_keys:
                raw = data.get(key)
                if isinstance(raw, list):
                    return [row for row in raw if isinstance(row, dict)]
    return []


def source_hashes(paths: list[Path | None]) -> list[dict[str, Any]]:
    out = []
    for path in paths:
        if path is None or not path.is_file():
            continue
        out.append(
            {
                "file_name_hash": sha256_json({"file_name": path.name}),
                "source_hash": sha256_bytes(path.read_bytes()),
                "raw_included": False,
            }
        )
    return out


def value(record: dict[str, Any], *keys: str) -> Any:
    for key in keys:
        if key in record and record[key] not in (None, ""):
            return record[key]
    return None


def text_value(record: dict[str, Any], *keys: str) -> str | None:
    raw = value(record, *keys)
    if raw is None:
        return None
    text = str(raw).strip()
    return text or None


def float_value(record: dict[str, Any], *keys: str) -> float | None:
    raw = value(record, *keys)
    try:
        return None if raw is None else float(raw)
    except (TypeError, ValueError):
        return None


def normalize_side(raw: Any) -> str | None:
    side = str(raw or "").strip().lower()
    if side in {"b", "buy", "bid", "long"}:
        return "buy"
    if side in {"a", "s", "sell", "ask", "short"}:
        return "sell"
    return None


def parse_ts(raw: Any) -> datetime | None:
    if raw is None:
        return None
    text = str(raw).strip()
    if not text:
        return None
    if text.isdigit():
        number = int(text)
        if number > 10_000_000_000:
            number = number / 1000
        return datetime.fromtimestamp(number, tz=timezone.utc)
    try:
        parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def time_bucket(record: dict[str, Any]) -> str | None:
    ts = parse_ts(value(record, "time", "timestamp", "ts", "created_at", "updated_at"))
    if ts is None:
        return None
    return ts.replace(minute=0, second=0, microsecond=0).isoformat().replace("+00:00", "Z")


def quantity_bucket(quantity: float | None) -> str:
    if quantity is None:
        return "unknown"
    absolute = abs(quantity)
    if absolute == 0:
        return "zero"
    if absolute < 0.001:
        return "<0.001"
    if absolute < 0.01:
        return "0.001-0.01"
    if absolute < 0.1:
        return "0.01-0.1"
    if absolute < 1:
        return "0.1-1"
    return ">=1"


def notional_bucket(quantity: float | None, price: float | None) -> str:
    if quantity is None or price is None:
        return "unknown"
    notional = abs(quantity * price)
    if notional < 10:
        return "<10"
    if notional < 100:
        return "10-100"
    if notional < 1_000:
        return "100-1000"
    if notional < 10_000:
        return "1000-10000"
    return ">=10000"


def symbol_payload(symbol: str | None, *, include_symbols: bool) -> dict[str, Any]:
    if not symbol:
        return {}
    normalized = symbol.upper()
    payload = {"symbol_hash": sha256_json({"symbol": normalized})}
    if include_symbols:
        payload["symbol"] = normalized
    return payload


def public_record(
    record: dict[str, Any],
    *,
    kind: str,
    include_symbols: bool,
) -> dict[str, Any]:
    symbol = text_value(record, "coin", "symbol", "asset")
    side = normalize_side(text_value(record, "side", "dir", "direction"))
    quantity = float_value(record, "sz", "size", "origSz", "qty", "quantity")
    price = float_value(record, "px", "price", "avgPx", "avg_price", "limitPx")
    payload: dict[str, Any] = {
        "kind": kind,
        "record_hash": sha256_json(record),
        "side": side or "unknown",
        "quantity_bucket": quantity_bucket(quantity),
        "notional_bucket": notional_bucket(quantity, price),
    }
    payload.update(symbol_payload(symbol, include_symbols=include_symbols))
    if bucket := time_bucket(record):
        payload["time_bucket"] = bucket
    return payload


def public_decision(record: dict[str, Any], *, include_symbols: bool) -> dict[str, Any]:
    payload = {
        "kind": "decision",
        "record_hash": sha256_json(record),
        "verdict": str(value(record, "verdict", "status") or "unknown")[:80],
        "direction": str(value(record, "direction", "side") or "unknown")[:20],
        "reason_hash": sha256_json({"reason": str(value(record, "reason") or "")}),
    }
    payload.update(symbol_payload(text_value(record, "coin", "symbol", "asset"), include_symbols=include_symbols))
    if bucket := time_bucket(record):
        payload["time_bucket"] = bucket
    return payload


def public_reconciliation(path: Path | None) -> dict[str, Any]:
    if path is None or not path.is_file():
        return {"available": False}
    payload = load_json(path)
    if not isinstance(payload, dict):
        return {"available": False}
    status = str(payload.get("status") or payload.get("state") or "unknown")[:80]
    return {
        "available": True,
        "status": status,
        "snapshot_hash": sha256_json(payload),
        "raw_included": False,
    }


def count_by(records: list[dict[str, Any]], key: str) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        value = str(record.get(key) or "unknown")
        counts[value] = counts.get(value, 0) + 1
    return dict(sorted(counts.items()))


def window_from_records(records: list[dict[str, Any]]) -> dict[str, Any]:
    buckets = sorted({record["time_bucket"] for record in records if isinstance(record.get("time_bucket"), str)})
    return {
        "first_bucket": buckets[0] if buckets else None,
        "last_bucket": buckets[-1] if buckets else None,
        "bucket_granularity": "hour",
    }


def build_packet(args: argparse.Namespace) -> dict[str, Any]:
    fills_raw = load_records(args.fills, list_keys=("fills", "user_fills"))
    orders_raw = load_records(args.orders, list_keys=("orders", "open_orders"))
    trades_raw = load_records(args.trades, list_keys=("trades", "fills"))
    decisions_raw: list[dict[str, Any]] = []
    for path in args.decisions:
        decisions_raw.extend(load_records(path, list_keys=("decisions",)))

    fills = [public_record(row, kind="fill", include_symbols=args.include_symbols) for row in fills_raw]
    orders = [public_record(row, kind="order", include_symbols=args.include_symbols) for row in orders_raw]
    trades = [public_record(row, kind="trade", include_symbols=args.include_symbols) for row in trades_raw]
    decisions = [public_decision(row, include_symbols=args.include_symbols) for row in decisions_raw]
    live_records = [*fills, *orders, *trades, *decisions]

    packet: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "generated_at": now_iso(),
        "label": args.label,
        "source": {
            "kind": args.source,
            "raw_included": False,
            "source_hashes": source_hashes(
                [args.fills, args.orders, args.trades, args.reconciliation, *args.decisions]
            ),
        },
        "privacy": {
            "raw_wallet_addresses_included": False,
            "raw_private_keys_included": False,
            "raw_order_ids_included": False,
            "raw_client_order_ids_included": False,
            "raw_idempotency_keys_included": False,
            "raw_trace_ids_included": False,
            "raw_exchange_payloads_included": False,
            "raw_symbols_included": bool(args.include_symbols),
            "quantity_policy": "bucketed",
            "time_policy": "hour_bucketed",
            "price_policy": "notional_bucket_only",
        },
        "summary": {
            "exchange": "hyperliquid",
            "live_execution_observed": bool(fills or trades),
            "fill_records": len(fills),
            "order_records": len(orders),
            "trade_records": len(trades),
            "decision_records": len(decisions),
            "decision_verdicts": count_by(decisions, "verdict"),
            "record_kinds": {
                "fills": len(fills),
                "orders": len(orders),
                "trades": len(trades),
                "decisions": len(decisions),
            },
            "window": window_from_records(live_records),
        },
        "reconciliation": public_reconciliation(args.reconciliation),
        "records": {
            "fills": fills,
            "orders": orders,
            "trades": trades,
            "decisions": decisions[-100:],
        },
    }
    packet["evidence_hash"] = sha256_json(packet)
    return packet


def walk_keys(value: Any, prefix: str = "") -> list[str]:
    keys: list[str] = []
    if isinstance(value, dict):
        for key, nested in value.items():
            key_path = f"{prefix}.{key}" if prefix else str(key)
            keys.append(key_path)
            keys.extend(walk_keys(nested, key_path))
    elif isinstance(value, list):
        for idx, nested in enumerate(value):
            keys.extend(walk_keys(nested, f"{prefix}[{idx}]"))
    return keys


def add(findings: list[dict[str, str]], ok: bool, name: str, good: str, bad: str) -> None:
    findings.append({"status": "ok" if ok else "fail", "name": name, "detail": good if ok else bad})


def verify_packet(path: Path, *, forbid_tokens: list[str]) -> dict[str, Any]:
    packet = load_json(path)
    findings: list[dict[str, str]] = []
    add(findings, isinstance(packet, dict), "json_object", "packet is object", "packet must be object")
    if not isinstance(packet, dict):
        return build_report(path, findings)

    expected_hash = packet.get("evidence_hash")
    without_hash = dict(packet)
    without_hash.pop("evidence_hash", None)
    add(
        findings,
        packet.get("schema_version") == SCHEMA_VERSION,
        "schema_version",
        SCHEMA_VERSION,
        f"expected {SCHEMA_VERSION}",
    )
    add(
        findings,
        expected_hash == sha256_json(without_hash),
        "evidence_hash",
        "hash matches packet",
        "evidence_hash mismatch",
    )
    privacy = packet.get("privacy") if isinstance(packet.get("privacy"), dict) else {}
    for flag in (
        "raw_wallet_addresses_included",
        "raw_private_keys_included",
        "raw_order_ids_included",
        "raw_client_order_ids_included",
        "raw_idempotency_keys_included",
        "raw_trace_ids_included",
        "raw_exchange_payloads_included",
    ):
        add(findings, privacy.get(flag) is False, f"privacy:{flag}", "omitted", f"{flag} must be false")
    source = packet.get("source") if isinstance(packet.get("source"), dict) else {}
    add(
        findings,
        source.get("raw_included") is False,
        "source_raw_included",
        "raw source omitted",
        "raw source must not be included",
    )

    keys = walk_keys(packet)
    for key in keys:
        lowered = key.lower()
        bad_parts = [part for part in FORBIDDEN_KEY_PARTS if part in lowered]
        allowed = (
            lowered.endswith("_hash")
            or ".source_hashes" in lowered
            or lowered.startswith("privacy.")
        )
        add(
            findings,
            not bad_parts or allowed,
            f"key_safety:{key}",
            "safe key",
            f"forbidden key material: {', '.join(bad_parts)}",
        )

    text = path.read_text(encoding="utf-8")
    for name, pattern in FORBIDDEN_TEXT_PATTERNS.items():
        add(findings, re.search(pattern, text) is None, f"redaction:{name}", "not present", f"{name} found")
    for idx, token in enumerate(forbid_tokens, start=1):
        add(findings, token not in text, f"redaction:forbid_token_{idx}", "not present", "forbidden token found")

    summary = packet.get("summary") if isinstance(packet.get("summary"), dict) else {}
    add(
        findings,
        summary.get("live_execution_observed") is bool(summary.get("fill_records") or summary.get("trade_records")),
        "summary_live_observed",
        "live observed flag matches fill/trade counts",
        "live observed flag does not match fill/trade counts",
    )
    return build_report(path, findings)


def build_report(path: Path, findings: list[dict[str, str]]) -> dict[str, Any]:
    fail = len([finding for finding in findings if finding["status"] == "fail"])
    return {
        "schema_version": VERIFY_SCHEMA_VERSION,
        "packet": str(path),
        "ok": fail == 0,
        "summary": {
            "ok": len([finding for finding in findings if finding["status"] == "ok"]),
            "fail": fail,
        },
        "findings": findings,
    }


def render_report(report: dict[str, Any]) -> str:
    summary = report["summary"]
    header = (
        f"zero live trading evidence verify: ok={report['ok']} "
        f"checks={summary['ok']} fail={summary['fail']}"
    )
    failures = [
        f"- {finding['name']}: {finding['detail']}"
        for finding in report["findings"]
        if finding["status"] == "fail"
    ]
    return "\n".join([header, *failures]) if failures else header


def main() -> int:
    args = parse_args()
    if args.command == "build":
        packet = build_packet(args)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(packet, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        report = verify_packet(args.output, forbid_tokens=[])
        print(
            "zero live trading evidence: "
            f"wrote {args.output} live_observed={packet['summary']['live_execution_observed']} "
            f"fills={packet['summary']['fill_records']} decisions={packet['summary']['decision_records']}"
        )
        return 0 if report["ok"] else 1

    report = verify_packet(args.packet, forbid_tokens=args.forbid_token)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(render_report(report))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
