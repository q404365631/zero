#!/usr/bin/env python3
"""Collect and verify a public-safe ZERO live cockpit drill bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_cockpit_drill.v1"
DEFAULT_TIMEOUT_SECONDS = 8.0

PACKETS: tuple[tuple[str, str, str | None], ...] = (
    ("01_health", "/health", None),
    ("02_v2_status", "/v2/status", None),
    ("03_live_preflight", "/live/preflight", "zero.live_preflight.v1"),
    ("04_live_cockpit", "/live/cockpit", "zero.live_cockpit.v1"),
    ("05_immune", "/immune", "zero.immune.v1"),
    ("06_hl_reconcile", "/hl/reconcile", "zero.reconciliation.v1"),
    ("07_live_certification", "/live/certification", "zero.live_certification.v1"),
    ("08_live_receipts", "/live/receipts", "zero.live_execution_receipts.v1"),
    ("09_live_evidence", "/live/evidence", "zero.live_evidence.v1"),
    ("10_live_canary_policy", "/live/canary-policy", "zero.live_canary_policy.v1"),
    ("11_metrics", "/metrics", "zero.metrics.v1"),
    ("12_audit_export", "/audit/export?limit=100", "zero.audit.v1"),
)

REDACTION_PATTERNS: tuple[tuple[str, str], ...] = (
    (r"trace-[A-Za-z0-9._:-]+", "TRACE_REDACTED"),
    (r"Bearer\s+[A-Za-z0-9._~+/=-]+", "Bearer REDACTED"),
    (r"(?i)(authorization[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"(?i)(api[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"(?i)(private[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"(?i)(idempotency[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"smoke-[A-Za-z0-9._:-]+", "smoke-REDACTED"),
)


@dataclass(frozen=True)
class Check:
    status: str
    name: str
    detail: str

    def to_dict(self) -> dict[str, str]:
        return {"status": self.status, "name": self.name, "detail": self.detail}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Collect the read-only live cockpit stack as a public-safe drill "
            "bundle. Public paper deployments should stay fail-closed: "
            "ready=false and risk_increasing_allowed=false."
        )
    )
    parser.add_argument(
        "url",
        nargs="?",
        default=os.environ.get("ZERO_API_URL", os.environ.get("ZERO_RAILWAY_URL", "")),
        help="Base URL for the ZERO API. Defaults to ZERO_API_URL or ZERO_RAILWAY_URL.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output directory. Defaults to artifacts/live-cockpit-drill/<timestamp>.",
    )
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT_SECONDS)
    parser.add_argument(
        "--expect-refusal",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Require the public paper safety posture: live_mode=refused and ready=false.",
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
        "--forbid-token",
        action="append",
        default=[],
        help="Additional raw token that must not appear in bundle JSON.",
    )
    parser.add_argument("--json", action="store_true", help="Print manifest JSON instead of summary.")
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
    return Path("artifacts") / "live-cockpit-drill" / stamp


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def redact_text(text: str, extra_tokens: list[str]) -> str:
    redacted = text
    for token in extra_tokens:
        if token:
            redacted = redacted.replace(token, "REDACTED")
    for pattern, replacement in REDACTION_PATTERNS:
        redacted = re.sub(pattern, replacement, redacted)
    return redacted


def redact_json(value: Any, extra_tokens: list[str]) -> Any:
    return json.loads(redact_text(json.dumps(value, sort_keys=True), extra_tokens))


def write_json(path: Path, payload: Any, extra_tokens: list[str]) -> None:
    path.write_text(
        json.dumps(redact_json(payload, extra_tokens), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def headers(args: argparse.Namespace) -> dict[str, str]:
    return {
        "accept": "application/json",
        "user-agent": "zero-live-cockpit-drill/1",
        "x-zero-operator-id": args.operator_id,
        "x-zero-operator-handle": args.operator_handle,
        "x-zero-operator-role": args.operator_role,
        "x-zero-operator-scope": args.operator_scope,
    }


def fetch_packet(base_url: str, path: str, *, args: argparse.Namespace) -> dict[str, Any]:
    request = urllib.request.Request(
        f"{base_url}{path}",
        headers=headers(args),
        method="GET",
    )
    try:
        with urllib.request.urlopen(request, timeout=args.timeout) as response:
            raw = response.read().decode("utf-8", errors="replace")
            return {
                "path": path,
                "status": response.status,
                "headers": {key.lower(): value for key, value in response.headers.items()},
                "payload": parse_json_or_raw(raw),
            }
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        return {
            "path": path,
            "status": exc.code,
            "headers": {key.lower(): value for key, value in exc.headers.items()},
            "payload": parse_json_or_raw(raw),
            "error": str(exc),
        }
    except (OSError, TimeoutError, urllib.error.URLError) as exc:
        return {"path": path, "status": 0, "headers": {}, "payload": None, "error": str(exc)}


def parse_json_or_raw(raw: str) -> Any:
    if not raw:
        return None
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {"raw": raw}


def packet_payload(packet: dict[str, Any]) -> dict[str, Any]:
    payload = packet.get("payload")
    return payload if isinstance(payload, dict) else {}


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


def add(checks: list[Check], ok: bool, name: str, good: str, bad: str) -> None:
    checks.append(Check("ok" if ok else "fail", name, good if ok else bad))


def body_text(output_dir: Path) -> str:
    return "\n".join(
        path.read_text(encoding="utf-8")
        for path in sorted(output_dir.iterdir())
        if path.is_file() and path.suffix == ".json"
    )


def validate_packets(
    packets: dict[str, dict[str, Any]],
    output_dir: Path,
    *,
    expect_refusal: bool,
    forbid_tokens: list[str],
) -> list[Check]:
    checks: list[Check] = []
    for name, _, schema in PACKETS:
        packet = packets[name]
        payload = packet_payload(packet)
        add(checks, packet.get("status") == 200, f"status:{name}", "http 200", "non-200 packet")
        if schema is not None:
            add(
                checks,
                payload.get("schema_version") == schema,
                f"schema:{name}",
                schema,
                f"expected schema_version={schema}",
            )

    preflight = packet_payload(packets["03_live_preflight"])
    cockpit = packet_payload(packets["04_live_cockpit"])
    immune = packet_payload(packets["05_immune"])
    reconciliation = packet_payload(packets["06_hl_reconcile"])
    certification = packet_payload(packets["07_live_certification"])
    canary_policy = packet_payload(packets["10_live_canary_policy"])

    if expect_refusal:
        add(
            checks,
            preflight.get("ready") is False and preflight.get("live_mode") == "refused",
            "refusal:preflight",
            "preflight fail-closed",
            "preflight did not report refused ready=false",
        )
        add(
            checks,
            cockpit.get("ready") is False
            and cockpit.get("live_mode") == "refused"
            and cockpit.get("risk_increasing_allowed") is False,
            "refusal:cockpit",
            "cockpit fail-closed",
            "cockpit did not report refused ready=false risk=false",
        )
        add(
            checks,
            immune.get("risk_increasing_allowed") is False,
            "refusal:immune",
            "immune blocks risk",
            "immune unexpectedly allows risk",
        )

    add(
        checks,
        isinstance(cockpit.get("next_action"), str) and bool(cockpit.get("next_action")),
        "operator:next_action",
        "next action present",
        "cockpit next_action missing",
    )
    actions = cockpit.get("operator_actions", {})
    risk_reducing = actions.get("risk_reducing", []) if isinstance(actions, dict) else []
    add(
        checks,
        "/kill" in risk_reducing and "/flatten-all" in risk_reducing,
        "operator:risk_reducing",
        "kill and flatten actions visible",
        "risk-reducing action list incomplete",
    )
    add(
        checks,
        reconciliation.get("schema_version") == "zero.reconciliation.v1",
        "operator:reconciliation",
        str(reconciliation.get("status")),
        "reconciliation packet missing",
    )
    add(
        checks,
        certification.get("mode") == "dry_run" and certification.get("passed") is True,
        "operator:certification",
        "dry-run certification passed",
        "live certification did not pass dry-run",
    )
    add(
        checks,
        canary_policy.get("summary", {}).get("policy_armed") is False,
        "operator:canary_policy",
        "canary policy disarmed",
        "canary policy unexpectedly armed",
    )

    text = body_text(output_dir)
    leak_patterns = {
        "raw_trace_id": r"trace-[A-Za-z0-9._:-]+",
        "bearer_token": r"Bearer\s+(?!REDACTED\b)[A-Za-z0-9._~+/=-]+",
        "openai_key": r"sk-[A-Za-z0-9]{16,}",
        "private_key": r"(?i)private[_-]?key[\"']?\s*[:=]\s*[\"']?(?!REDACTED\b)[^\"'\s,}]+",
    }
    for name, pattern in leak_patterns.items():
        add(checks, re.search(pattern, text) is None, f"redaction:{name}", "not present", f"{name} found")
    for idx, token in enumerate(forbid_tokens, start=1):
        add(
            checks,
            token not in text,
            f"redaction:forbid_token_{idx}",
            "not present",
            "forbidden token found",
        )
    return checks


def build_summary(packets: dict[str, dict[str, Any]]) -> dict[str, Any]:
    preflight = packet_payload(packets["03_live_preflight"])
    cockpit = packet_payload(packets["04_live_cockpit"])
    immune = packet_payload(packets["05_immune"])
    reconciliation = packet_payload(packets["06_hl_reconcile"])
    certification = packet_payload(packets["07_live_certification"])
    receipts = packet_payload(packets["08_live_receipts"])
    evidence = packet_payload(packets["09_live_evidence"])
    canary_policy = packet_payload(packets["10_live_canary_policy"])
    return {
        "live_mode": cockpit.get("live_mode", preflight.get("live_mode")),
        "ready": cockpit.get("ready"),
        "controls_ready": cockpit.get("controls_ready", preflight.get("controls_ready")),
        "risk_increasing_allowed": cockpit.get("risk_increasing_allowed"),
        "next_action": cockpit.get("next_action"),
        "preflight_failed": cockpit.get("preflight", {}).get("summary", {}).get("failed"),
        "immune_risk_blocking": immune.get("summary", {}).get("risk_blocking"),
        "reconciliation_status": reconciliation.get("status"),
        "certification_passed": certification.get("passed"),
        "live_records_total": receipts.get("summary", {}).get("total"),
        "live_records_accepted": receipts.get("summary", {}).get("accepted"),
        "evidence_hash": evidence.get("evidence_hash"),
        "canary_policy_qualified": canary_policy.get("summary", {}).get("qualified"),
        "canary_policy_next_step": canary_policy.get("summary", {}).get("next_step"),
    }


def main() -> int:
    args = parse_args()
    try:
        base_url = normalize_base_url(args.url)
    except ValueError as exc:
        print(f"zero live cockpit drill: {exc}", file=sys.stderr)
        return 2

    output_dir = args.output or default_output_dir()
    output_dir.mkdir(parents=True, exist_ok=True)

    packets: dict[str, dict[str, Any]] = {}
    packet_manifest: list[dict[str, Any]] = []
    for name, path, schema in PACKETS:
        packet = fetch_packet(base_url, path, args=args)
        file_name = f"{name}.json"
        write_json(output_dir / file_name, packet, args.forbid_token)
        packets[name] = packet
        packet_manifest.append(
            {
                "name": name,
                "path": path,
                "file": file_name,
                "status": packet.get("status"),
                "expected_schema_version": schema,
            }
        )

    checks = validate_packets(
        packets,
        output_dir,
        expect_refusal=args.expect_refusal,
        forbid_tokens=args.forbid_token,
    )
    summary = build_summary(packets)
    fail_count = sum(1 for check in checks if check.status == "fail")
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "created_at": utc_now(),
        "collector": {
            "expect_refusal": args.expect_refusal,
            "timeout_s": args.timeout,
            "packets": len(PACKETS),
        },
        "operator_context": {
            "operator_id": args.operator_id,
            "handle": args.operator_handle,
            "role": args.operator_role,
            "scope": args.operator_scope,
        },
        "summary": {
            **summary,
            "ok": fail_count == 0,
            "checks": len(checks),
            "fail": fail_count,
        },
        "packets": packet_manifest,
        "checks": [check.to_dict() for check in checks],
        "files": [],
    }
    write_json(output_dir / "manifest.json", manifest, args.forbid_token)
    manifest["files"] = build_file_inventory(output_dir)
    write_json(output_dir / "manifest.json", manifest, args.forbid_token)
    write_sha256s(output_dir)

    if args.json:
        print(json.dumps(manifest, indent=2, sort_keys=True))
    else:
        print(
            "zero live cockpit drill: "
            f"ok={fail_count == 0} ready={summary['ready']} "
            f"risk_allowed={summary['risk_increasing_allowed']} "
            f"fail={fail_count} output={output_dir}"
        )
    return 0 if fail_count == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
