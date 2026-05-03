#!/usr/bin/env python3
"""Attach public-safe exchange-side evidence to a ZERO live canary bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_canary_exchange_evidence.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Normalize an operator-owned Hyperliquid order/fill export, compare "
            "it with ZERO live receipts, and attach a public-safe evidence packet "
            "to the canary bundle."
        )
    )
    parser.add_argument("bundle", type=Path, help="Bundle directory from live_canary_rehearsal.py.")
    parser.add_argument(
        "source",
        type=Path,
        help="Operator-provided JSON export containing orders and/or fills.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output packet path. Defaults to <bundle>/exchange_evidence.json.",
    )
    parser.add_argument(
        "--require-match",
        action="store_true",
        help="Exit nonzero unless every accepted ZERO receipt is matched.",
    )
    return parser.parse_args()


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def sha256_json(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return sha256_bytes(encoded)


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def packet_payload(bundle: Path, file_name: str) -> dict[str, Any]:
    packet = load_json(bundle / file_name)
    payload = packet.get("payload") if isinstance(packet, dict) else None
    return payload if isinstance(payload, dict) else {}


def receipts(bundle: Path) -> list[dict[str, Any]]:
    payload = packet_payload(bundle, "90_live_receipts.json")
    raw = payload.get("receipts")
    return [receipt for receipt in raw if isinstance(receipt, dict)] if isinstance(raw, list) else []


def manifest_summary(bundle: Path) -> dict[str, Any]:
    manifest = load_json(bundle / "manifest.json")
    summary = manifest.get("summary") if isinstance(manifest, dict) else None
    return summary if isinstance(summary, dict) else {}


def collect_records(source: Any, key: str) -> list[dict[str, Any]]:
    if isinstance(source, dict):
        raw = source.get(key)
        if isinstance(raw, list):
            return [item for item in raw if isinstance(item, dict)]
        data = source.get("data")
        if isinstance(data, dict):
            raw = data.get(key)
            if isinstance(raw, list):
                return [item for item in raw if isinstance(item, dict)]
    if isinstance(source, list) and key in {"fills", "orders"}:
        return [item for item in source if isinstance(item, dict)]
    return []


def side_value(value: Any) -> str | None:
    raw = str(value or "").strip().lower()
    if raw in {"b", "buy", "bid", "long"}:
        return "buy"
    if raw in {"a", "s", "sell", "ask", "short"}:
        return "sell"
    return None


def float_value(record: dict[str, Any], *keys: str) -> float | None:
    for key in keys:
        if key not in record:
            continue
        try:
            return float(record[key])
        except (TypeError, ValueError):
            continue
    return None


def str_value(record: dict[str, Any], *keys: str) -> str | None:
    for key in keys:
        value = record.get(key)
        if value is not None and str(value).strip():
            return str(value).strip()
    return None


def normalize_fill(record: dict[str, Any]) -> dict[str, Any] | None:
    symbol = str_value(record, "coin", "symbol", "asset")
    side = side_value(str_value(record, "side", "dir", "direction"))
    quantity = float_value(record, "sz", "size", "qty", "quantity")
    price = float_value(record, "px", "price", "avgPx", "avg_price")
    if not symbol or not side or quantity is None:
        return None
    normalized: dict[str, Any] = {
        "kind": "fill",
        "symbol": symbol.upper(),
        "side": side,
        "quantity": quantity,
    }
    if price is not None:
        normalized["price"] = price
    if time_value := str_value(record, "time", "timestamp", "ts"):
        normalized["time_hash"] = sha256_json({"time": time_value})
    if oid := str_value(record, "oid", "order_id"):
        normalized["order_id_hash"] = sha256_json({"order_id": oid})
    if cloid := str_value(record, "cloid", "client_order_id"):
        normalized["client_order_id_hash"] = sha256_json({"client_order_id": cloid})
    normalized["record_hash"] = sha256_json(record)
    return normalized


def normalize_order(record: dict[str, Any]) -> dict[str, Any] | None:
    symbol = str_value(record, "coin", "symbol", "asset")
    side = side_value(str_value(record, "side", "dir", "direction"))
    quantity = float_value(record, "sz", "size", "origSz", "orig_size", "qty", "quantity")
    price = float_value(record, "limitPx", "price", "px", "avgPx", "avg_price")
    if not symbol or not side or quantity is None:
        return None
    normalized: dict[str, Any] = {
        "kind": "order",
        "symbol": symbol.upper(),
        "side": side,
        "quantity": quantity,
    }
    if price is not None:
        normalized["price"] = price
    if status := str_value(record, "status", "orderStatus"):
        normalized["status"] = safe_status(status)
    if oid := str_value(record, "oid", "order_id"):
        normalized["order_id_hash"] = sha256_json({"order_id": oid})
    if cloid := str_value(record, "cloid", "client_order_id"):
        normalized["client_order_id_hash"] = sha256_json({"client_order_id": cloid})
    normalized["record_hash"] = sha256_json(record)
    return normalized


def safe_status(value: str) -> str:
    return re.sub(r"[^a-zA-Z0-9_.:-]", "_", value.strip())[:80]


def request_from_receipt(receipt: dict[str, Any]) -> dict[str, Any]:
    request = receipt.get("request")
    return request if isinstance(request, dict) else {}


def quantities_match(expected: float, actual: float) -> bool:
    return abs(expected - actual) <= max(1e-9, expected * 0.000001)


def match_receipts(
    accepted_receipts: list[dict[str, Any]],
    fills: list[dict[str, Any]],
    orders: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    matches: list[dict[str, Any]] = []
    exchange_records = [*fills, *orders]
    for receipt in accepted_receipts:
        request = request_from_receipt(receipt)
        symbol = str(request.get("symbol", "")).upper()
        side = str(request.get("side", "")).lower()
        quantity = float(request.get("quantity") or 0)
        candidates = [
            record
            for record in exchange_records
            if record.get("symbol") == symbol
            and record.get("side") == side
            and quantities_match(quantity, float(record.get("quantity") or 0))
        ]
        matches.append(
            {
                "receipt_hash": receipt.get("receipt_hash"),
                "request_hash": receipt.get("request_hash"),
                "symbol": symbol,
                "side": side,
                "quantity": quantity,
                "matched": bool(candidates),
                "matched_exchange_record_hashes": [record["record_hash"] for record in candidates],
                "reason": "matched_by_symbol_side_quantity" if candidates else "no_matching_exchange_record",
            }
        )
    return matches


def write_sha256s(bundle: Path) -> None:
    lines = []
    for path in sorted(bundle.iterdir()):
        if path.is_file() and path.name != "SHA256SUMS":
            digest = sha256_file(path).removeprefix("sha256:")
            lines.append(f"{digest}  {path.name}")
    (bundle / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def build_packet(bundle: Path, source_path: Path) -> dict[str, Any]:
    raw_bytes = source_path.read_bytes()
    source = json.loads(raw_bytes.decode("utf-8"))
    normalized_fills = [
        fill for fill in (normalize_fill(record) for record in collect_records(source, "fills")) if fill
    ]
    normalized_orders = [
        order for order in (normalize_order(record) for record in collect_records(source, "orders")) if order
    ]
    live_receipts = receipts(bundle)
    accepted = [receipt for receipt in live_receipts if receipt.get("accepted") is True]
    matches = match_receipts(accepted, normalized_fills, normalized_orders)
    matched = len([match for match in matches if match["matched"]])
    summary = manifest_summary(bundle)
    return {
        "schema_version": SCHEMA_VERSION,
        "generated_at": now_iso(),
        "bundle": {
            "manifest_evidence_hash": summary.get("evidence_hash"),
            "receipts_total": summary.get("receipts_total"),
            "receipts_accepted": summary.get("receipts_accepted"),
        },
        "source": {
            "file_name_hash": sha256_json({"file_name": source_path.name}),
            "source_hash": sha256_bytes(raw_bytes),
            "raw_included": False,
        },
        "privacy": {
            "wallet_addresses_included": False,
            "raw_order_ids_included": False,
            "raw_client_order_ids_included": False,
            "raw_venue_payload_included": False,
            "included": [
                "normalized symbol",
                "normalized side",
                "normalized quantity",
                "optional normalized price",
                "hashes of raw order/fill records",
                "hashes of venue order identifiers when present",
            ],
        },
        "summary": {
            "accepted_receipts": len(accepted),
            "exchange_orders": len(normalized_orders),
            "exchange_fills": len(normalized_fills),
            "matched_receipts": matched,
            "unmatched_receipts": len(accepted) - matched,
            "complete": matched == len(accepted),
        },
        "orders": normalized_orders,
        "fills": normalized_fills,
        "matches": matches,
    }


def main() -> int:
    args = parse_args()
    output = args.output or args.bundle / "exchange_evidence.json"
    packet = build_packet(args.bundle, args.source)
    write_json(output, packet)
    if output.parent.resolve() == args.bundle.resolve():
        write_sha256s(args.bundle)
    complete = bool(packet["summary"]["complete"])
    print(
        "zero live canary exchange evidence: "
        f"wrote {output} matched={packet['summary']['matched_receipts']} "
        f"accepted={packet['summary']['accepted_receipts']}"
    )
    return 0 if complete or not args.require_match else 1


if __name__ == "__main__":
    raise SystemExit(main())
