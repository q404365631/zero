#!/usr/bin/env python3
"""Collect a public-safe live canary rehearsal bundle for ZERO."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "engine" / "src"))

from zero_engine.live_canary_policy import build_live_canary_policy, inputs_from_rehearsal


SCHEMA_VERSION = "zero.live_canary_rehearsal.v1"
DEFAULT_TIMEOUT_SECONDS = 8.0
RISK_CONFIRMATION = "I_UNDERSTAND_THIS_CAN_PLACE_A_REAL_HYPERLIQUID_ORDER"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run the operator-owned live canary rehearsal. Default refusal mode "
            "is safe for public paper deployments: it only proves that a live "
            "execute request fails closed when preflight is not ready."
        )
    )
    parser.add_argument(
        "url",
        nargs="?",
        default=os.environ.get("ZERO_API_URL", os.environ.get("ZERO_RAILWAY_URL", "")),
        help="Base URL for the ZERO API. Defaults to ZERO_API_URL or ZERO_RAILWAY_URL.",
    )
    parser.add_argument(
        "--mode",
        choices=("refusal", "collect-only", "canary"),
        default="refusal",
        help=(
            "refusal proves a not-ready live executor refuses; collect-only captures "
            "readiness/evidence without execute; canary can place a real live order."
        ),
    )
    parser.add_argument("--symbol", default="BTC", help="Canary symbol.")
    parser.add_argument("--side", choices=("buy", "sell"), default="buy", help="Canary side.")
    parser.add_argument("--size", type=float, default=0.001, help="Canary order size.")
    parser.add_argument(
        "--idempotency-key",
        default=None,
        help="Operator-supplied idempotency key. Defaults to a timestamped local key.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output directory. Defaults to artifacts/live-canary-rehearsal/<timestamp>.",
    )
    parser.add_argument(
        "--operator-id",
        default=os.environ.get("ZERO_OPERATOR_ID", "local-operator"),
        help="Operator audit id header.",
    )
    parser.add_argument(
        "--operator-handle",
        default=os.environ.get("ZERO_OPERATOR_HANDLE", "local-operator"),
        help="Operator audit handle header.",
    )
    parser.add_argument(
        "--operator-role",
        default=os.environ.get("ZERO_OPERATOR_ROLE", "owner"),
        help="Operator audit role header.",
    )
    parser.add_argument(
        "--operator-scope",
        default=os.environ.get("ZERO_OPERATOR_SCOPE", "local-private"),
        help="Operator audit scope header.",
    )
    parser.add_argument(
        "--confirm-live-risk",
        default="",
        help=f"Required for --mode canary. Must equal {RISK_CONFIRMATION!r}.",
    )
    parser.add_argument(
        "--skip-kill",
        action="store_true",
        help="In canary mode, skip the final /live/kill risk-reducing control.",
    )
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT_SECONDS)
    return parser.parse_args()


def normalize_base_url(raw: str) -> str:
    value = raw.strip()
    if not value:
        raise ValueError("API URL is required")
    if not value.startswith(("http://", "https://")):
        value = f"http://{value}"
    return value.rstrip("/")


def default_output_dir() -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path("artifacts") / "live-canary-rehearsal" / stamp


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def redaction_patterns(idempotency_key: str) -> tuple[tuple[str, str], ...]:
    escaped_key = re.escape(idempotency_key)
    return (
        (r"trace-[A-Za-z0-9._:-]+", "TRACE_REDACTED"),
        (r"Bearer\s+[A-Za-z0-9._~+/=-]+", "Bearer REDACTED"),
        (r"(?i)(authorization[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
        (r"(?i)(api[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
        (r"(?i)(private[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
        (rf"{escaped_key}", "IDEMPOTENCY_KEY_REDACTED"),
    )


def redact_json(value: Any, *, idempotency_key: str) -> Any:
    text = json.dumps(value, sort_keys=True)
    for pattern, replacement in redaction_patterns(idempotency_key):
        text = re.sub(pattern, replacement, text)
    return json.loads(text)


def write_json(path: Path, payload: Any, *, idempotency_key: str) -> None:
    path.write_text(
        json.dumps(redact_json(payload, idempotency_key=idempotency_key), indent=2, sort_keys=True)
        + "\n",
        encoding="utf-8",
    )


def headers(args: argparse.Namespace, *, live: bool = False) -> dict[str, str]:
    result = {
        "accept": "application/json",
        "user-agent": "zero-live-canary-rehearsal/1",
        "x-zero-operator-id": args.operator_id,
        "x-zero-operator-handle": args.operator_handle,
        "x-zero-operator-role": args.operator_role,
        "x-zero-operator-scope": args.operator_scope,
    }
    if live:
        result["x-zero-mode"] = "live"
        result["content-type"] = "application/json"
    return result


def request_json(
    base_url: str,
    method: str,
    path: str,
    *,
    args: argparse.Namespace,
    payload: dict[str, Any] | None = None,
    live: bool = False,
) -> dict[str, Any]:
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        f"{base_url}{path}",
        data=data,
        headers=headers(args, live=live),
        method=method,
    )
    try:
        with urllib.request.urlopen(request, timeout=args.timeout) as response:
            raw = response.read().decode("utf-8", errors="replace")
            return {
                "method": method,
                "path": path,
                "status": response.status,
                "payload": parse_json_or_raw(raw),
            }
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        return {
            "method": method,
            "path": path,
            "status": exc.code,
            "payload": parse_json_or_raw(raw),
            "error": str(exc),
        }
    except (OSError, TimeoutError, urllib.error.URLError) as exc:
        return {"method": method, "path": path, "status": 0, "payload": None, "error": str(exc)}


def parse_json_or_raw(raw: str) -> Any:
    if not raw:
        return None
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {"raw": raw}


def write_step(
    output_dir: Path,
    name: str,
    packet: dict[str, Any],
    *,
    idempotency_key: str,
) -> dict[str, Any]:
    file_name = f"{name}.json"
    write_json(output_dir / file_name, packet, idempotency_key=idempotency_key)
    return {"name": name, "file": file_name, "status": packet["status"]}


def packet_payload(packet: dict[str, Any]) -> dict[str, Any]:
    payload = packet.get("payload")
    return payload if isinstance(payload, dict) else {}


def execute_payload(args: argparse.Namespace, idempotency_key: str) -> dict[str, Any]:
    return {
        "coin": args.symbol.upper(),
        "side": args.side,
        "size": args.size,
        "idempotency_key": idempotency_key,
    }


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_sha256s(output_dir: Path) -> None:
    lines = []
    for path in sorted(output_dir.iterdir()):
        if path.is_file() and path.name != "SHA256SUMS":
            lines.append(f"{sha256(path)}  {path.name}")
    (output_dir / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def build_file_inventory(output_dir: Path) -> list[dict[str, Any]]:
    files: list[dict[str, Any]] = []
    for path in sorted(output_dir.iterdir()):
        if path.is_file() and path.name not in {"manifest.json", "SHA256SUMS"}:
            files.append({"path": path.name, "bytes": path.stat().st_size, "sha256": sha256(path)})
    return files


def main() -> int:
    args = parse_args()
    try:
        base_url = normalize_base_url(args.url)
    except ValueError as exc:
        print(f"zero live canary rehearsal: {exc}", file=sys.stderr)
        return 2
    if args.size <= 0:
        print("zero live canary rehearsal: --size must be positive", file=sys.stderr)
        return 2
    if args.mode == "canary" and args.confirm_live_risk != RISK_CONFIRMATION:
        print(
            "zero live canary rehearsal: --mode canary requires "
            f"--confirm-live-risk {RISK_CONFIRMATION!r}",
            file=sys.stderr,
        )
        return 2

    output_dir = args.output or default_output_dir()
    output_dir.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    idempotency_key = args.idempotency_key or f"canary-{stamp}-{args.symbol.upper()}-{args.side}"

    steps: list[dict[str, Any]] = []

    def collect(
        name: str,
        method: str,
        path: str,
        *,
        payload: dict[str, Any] | None = None,
        live: bool = False,
    ) -> dict[str, Any]:
        packet = request_json(base_url, method, path, args=args, payload=payload, live=live)
        steps.append(write_step(output_dir, name, packet, idempotency_key=idempotency_key))
        return packet

    preflight = collect("01_live_preflight", "GET", "/live/preflight")
    heartbeat = collect("02_live_heartbeat", "POST", "/live/heartbeat", payload={})
    cockpit = collect("03_live_cockpit", "GET", "/live/cockpit")
    certification = collect("04_live_certification", "GET", "/live/certification")
    reconcile = collect("05_hl_reconcile", "GET", "/hl/reconcile")

    preflight_payload = packet_payload(preflight)
    cockpit_payload = packet_payload(cockpit)
    cert_payload = packet_payload(certification)
    risk_ready = bool(
        preflight_payload.get("ready")
        and cockpit_payload.get("risk_increasing_allowed")
        and cert_payload.get("passed")
        and cert_payload.get("live_start_certified")
    )
    exit_code = 0
    live_order_attempted = False
    live_order_accepted = False
    live_order_reason = "not_attempted"

    if args.mode == "collect-only":
        live_order_reason = "collect_only"
    elif args.mode == "refusal":
        if risk_ready:
            live_order_reason = "skipped_ready_engine_requires_canary_mode"
        else:
            execute = collect(
                "06_live_execute_refusal",
                "POST",
                "/execute",
                payload=execute_payload(args, idempotency_key),
                live=True,
            )
            live_order_attempted = True
            execute_body = packet_payload(execute)
            live_order_accepted = bool(execute_body.get("accepted"))
            live_order_reason = str(execute_body.get("reason", "missing_reason"))
            if live_order_accepted:
                exit_code = 1
    elif args.mode == "canary":
        if not risk_ready:
            live_order_reason = "blocked_before_order: live gates are not ready"
            exit_code = 1
        else:
            execute = collect(
                "06_live_execute_canary",
                "POST",
                "/execute",
                payload=execute_payload(args, idempotency_key),
                live=True,
            )
            live_order_attempted = True
            execute_body = packet_payload(execute)
            live_order_accepted = bool(execute_body.get("accepted"))
            live_order_reason = str(execute_body.get("reason", "missing_reason"))
            if not live_order_accepted:
                exit_code = 1
            collect("07_live_pause", "POST", "/live/pause", payload={})
            collect("08_live_flatten", "POST", "/live/flatten", payload={})
            if not args.skip_kill:
                collect("09_live_kill", "POST", "/live/kill", payload={})

    receipts = collect("90_live_receipts", "GET", "/live/receipts")
    evidence = collect("91_live_evidence", "GET", "/live/evidence")
    metrics = collect("92_metrics", "GET", "/metrics")
    audit = collect("93_audit_export", "GET", "/audit/export?limit=1000")

    evidence_payload = packet_payload(evidence)
    summary = {
        "mode": args.mode,
        "risk_ready": risk_ready,
        "preflight_ready": bool(preflight_payload.get("ready")),
        "controls_ready": bool(preflight_payload.get("controls_ready")),
        "cockpit_risk_increasing_allowed": bool(cockpit_payload.get("risk_increasing_allowed")),
        "certification_passed": bool(cert_payload.get("passed")),
        "live_start_certified": bool(cert_payload.get("live_start_certified")),
        "live_order_attempted": live_order_attempted,
        "live_order_accepted": live_order_accepted,
        "live_order_reason": live_order_reason,
        "receipts_total": packet_payload(receipts).get("summary", {}).get("total"),
        "receipts_accepted": packet_payload(receipts).get("summary", {}).get("accepted"),
        "evidence_hash": evidence_payload.get("evidence_hash"),
        "heartbeat_ok": bool(packet_payload(heartbeat).get("ok")),
        "reconciliation_status": packet_payload(reconcile).get("status"),
        "metrics_status": metrics["status"],
        "audit_status": audit["status"],
    }
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "target": base_url,
        "generated_at": utc_now(),
        "collector": {
            "name": "scripts/live_canary_rehearsal.py",
            "mode": args.mode,
            "redaction_applied": True,
            "raw_private_keys_included": False,
            "raw_venue_acks_included": False,
        },
        "operator": {
            "id": args.operator_id,
            "handle": args.operator_handle,
            "role": args.operator_role,
            "scope": args.operator_scope,
        },
        "request": {
            "symbol": args.symbol.upper(),
            "side": args.side,
            "size": args.size,
            "idempotency_key": "IDEMPOTENCY_KEY_REDACTED",
        },
        "summary": summary,
        "steps": steps,
    }
    manifest["policy"] = build_live_canary_policy(inputs_from_rehearsal(manifest))
    write_json(output_dir / "manifest.json", manifest, idempotency_key=idempotency_key)
    manifest["files"] = build_file_inventory(output_dir)
    write_json(output_dir / "manifest.json", manifest, idempotency_key=idempotency_key)
    write_sha256s(output_dir)

    print(
        "zero live canary rehearsal: "
        f"wrote {output_dir} mode={args.mode} risk_ready={risk_ready} "
        f"attempted={live_order_attempted} accepted={live_order_accepted}"
    )
    if exit_code:
        print(f"zero live canary rehearsal: failed gate: {live_order_reason}", file=sys.stderr)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
