#!/usr/bin/env python3
"""Run the public-safe ZERO live canary operator workflow."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_canary_operator.v1"
RISK_CONFIRMATION = "I_UNDERSTAND_THIS_CAN_PLACE_A_REAL_HYPERLIQUID_ORDER"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run or finalize a ZERO live canary evidence workflow. The command "
            "never submits live risk unless --mode canary and the rehearsal "
            "confirmation phrase are provided."
        )
    )
    parser.add_argument(
        "url",
        nargs="?",
        default=os.environ.get("ZERO_API_URL", os.environ.get("ZERO_RAILWAY_URL", "")),
        help="Base URL for the ZERO API. Required unless --bundle is provided.",
    )
    parser.add_argument(
        "--bundle",
        type=Path,
        default=None,
        help="Existing canary bundle to finalize instead of running a new rehearsal.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Operator workflow directory. Defaults to artifacts/live-canary-operator/<timestamp>.",
    )
    parser.add_argument(
        "--mode",
        choices=("refusal", "collect-only", "canary"),
        default="refusal",
        help="Canary collector mode when running a new rehearsal.",
    )
    parser.add_argument("--symbol", default="BTC", help="Canary symbol.")
    parser.add_argument("--side", choices=("buy", "sell"), default="buy", help="Canary side.")
    parser.add_argument("--size", type=float, default=0.001, help="Canary order size.")
    parser.add_argument("--idempotency-key", default=None, help="Operator-supplied idempotency key.")
    parser.add_argument(
        "--exchange-export",
        type=Path,
        default=None,
        help="Operator-owned Hyperliquid JSON order/fill export.",
    )
    parser.add_argument(
        "--require-exchange-evidence",
        action="store_true",
        help="Fail unless exchange evidence is attached and every accepted receipt is matched.",
    )
    parser.add_argument(
        "--require-live-accepted",
        action="store_true",
        help="Fail unless the bundle contains an accepted live canary order.",
    )
    parser.add_argument(
        "--confirm-live-risk",
        default="",
        help=f"Required by live_canary_rehearsal.py for --mode canary. Must equal {RISK_CONFIRMATION!r}.",
    )
    parser.add_argument("--operator-id", default=os.environ.get("ZERO_OPERATOR_ID", "local-operator"))
    parser.add_argument(
        "--operator-handle",
        default=os.environ.get("ZERO_OPERATOR_HANDLE", "local-operator"),
    )
    parser.add_argument("--operator-role", default=os.environ.get("ZERO_OPERATOR_ROLE", "owner"))
    parser.add_argument(
        "--operator-scope",
        default=os.environ.get("ZERO_OPERATOR_SCOPE", "local-private"),
    )
    parser.add_argument("--timeout", type=float, default=8.0)
    parser.add_argument("--json", action="store_true", help="Print the operator report as JSON.")
    return parser.parse_args()


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def default_output_dir() -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path("artifacts") / "live-canary-operator" / stamp


def script_path(name: str) -> Path:
    return Path(__file__).resolve().with_name(name)


def run_command(argv: list[str]) -> dict[str, Any]:
    completed = subprocess.run(argv, check=False, text=True, capture_output=True)
    return {
        "argv": redact_argv(argv),
        "status": completed.returncode,
        "stdout": completed.stdout.strip(),
        "stderr": completed.stderr.strip(),
    }


def redact_argv(argv: list[str]) -> list[str]:
    redacted: list[str] = []
    skip_next = False
    for idx, part in enumerate(argv):
        if skip_next:
            redacted.append("REDACTED")
            skip_next = False
            continue
        redacted.append(part)
        if part in {"--idempotency-key", "--confirm-live-risk", "--forbid-token"}:
            skip_next = idx + 1 < len(argv)
    return redacted


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_sha256s(workflow_dir: Path) -> None:
    lines = []
    for path in sorted(workflow_dir.rglob("*")):
        if path.is_file() and path.relative_to(workflow_dir).as_posix() != "SHA256SUMS":
            lines.append(f"{sha256(path)}  {path.relative_to(workflow_dir).as_posix()}")
    (workflow_dir / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def manifest_summary(bundle: Path) -> dict[str, Any]:
    manifest = load_json(bundle / "manifest.json")
    summary = manifest.get("summary") if isinstance(manifest, dict) else None
    return summary if isinstance(summary, dict) else {}


def report_path(workflow_dir: Path) -> Path:
    return workflow_dir / "operator_report.json"


def write_empty_exchange_export(workflow_dir: Path) -> Path:
    path = workflow_dir / "empty_exchange_export.json"
    write_json(path, {"orders": [], "fills": []})
    return path


def build_rehearsal_command(args: argparse.Namespace, bundle: Path) -> list[str]:
    command = [
        sys.executable,
        str(script_path("live_canary_rehearsal.py")),
        args.url,
        "--mode",
        args.mode,
        "--symbol",
        args.symbol,
        "--side",
        args.side,
        "--size",
        str(args.size),
        "--output",
        str(bundle),
        "--operator-id",
        args.operator_id,
        "--operator-handle",
        args.operator_handle,
        "--operator-role",
        args.operator_role,
        "--operator-scope",
        args.operator_scope,
        "--timeout",
        str(args.timeout),
    ]
    if args.idempotency_key:
        command.extend(["--idempotency-key", args.idempotency_key])
    if args.confirm_live_risk:
        command.extend(["--confirm-live-risk", args.confirm_live_risk])
    return command


def build_exchange_command(bundle: Path, source: Path, *, require_match: bool) -> list[str]:
    command = [
        sys.executable,
        str(script_path("live_canary_exchange_evidence.py")),
        str(bundle),
        str(source),
    ]
    if require_match:
        command.append("--require-match")
    return command


def build_verify_command(
    args: argparse.Namespace,
    bundle: Path,
    *,
    require_exchange_evidence: bool,
) -> list[str]:
    command = [
        sys.executable,
        str(script_path("live_canary_verify.py")),
        str(bundle),
        "--require-mode",
        args.mode,
    ]
    if args.require_live_accepted:
        command.append("--require-live-accepted")
    if require_exchange_evidence:
        command.append("--require-exchange-evidence")
    if args.idempotency_key:
        command.extend(["--forbid-token", args.idempotency_key])
    return command


def main() -> int:
    args = parse_args()
    workflow_dir = args.output or default_output_dir()
    workflow_dir.mkdir(parents=True, exist_ok=True)
    bundle = args.bundle or workflow_dir / "bundle"
    commands: dict[str, dict[str, Any] | None] = {
        "rehearsal": None,
        "exchange_evidence": None,
        "verify": None,
    }
    failures: list[str] = []

    if args.bundle is None:
        if not args.url.strip():
            failures.append("API URL is required unless --bundle is provided")
        elif args.mode == "canary" and args.confirm_live_risk != RISK_CONFIRMATION:
            failures.append("canary mode requires the exact live-risk confirmation phrase")
        else:
            commands["rehearsal"] = run_command(build_rehearsal_command(args, bundle))
            if commands["rehearsal"]["status"] != 0:
                failures.append("rehearsal failed")
    elif not bundle.is_dir():
        failures.append("bundle directory missing")

    summary: dict[str, Any] = {}
    accepted_receipts = 0
    if bundle.is_dir() and (bundle / "manifest.json").is_file():
        summary = manifest_summary(bundle)
        accepted_receipts = int(summary.get("receipts_accepted") or 0)
    elif not failures:
        failures.append("bundle manifest missing")

    exchange_source = args.exchange_export
    auto_empty_exchange = False
    if exchange_source is None and accepted_receipts == 0 and bundle.is_dir():
        exchange_source = write_empty_exchange_export(workflow_dir)
        auto_empty_exchange = True

    exchange_required = args.require_exchange_evidence or accepted_receipts > 0
    exchange_attached = False
    if exchange_source is not None and bundle.is_dir():
        commands["exchange_evidence"] = run_command(
            build_exchange_command(bundle, exchange_source, require_match=exchange_required)
        )
        exchange_attached = commands["exchange_evidence"]["status"] == 0
        if not exchange_attached:
            failures.append("exchange evidence failed")
    elif exchange_required:
        failures.append("exchange evidence is required for accepted live receipts")

    require_exchange_for_verify = exchange_required or exchange_attached
    if bundle.is_dir() and (bundle / "manifest.json").is_file():
        commands["verify"] = run_command(
            build_verify_command(
                args,
                bundle,
                require_exchange_evidence=require_exchange_for_verify,
            )
        )
        if commands["verify"]["status"] != 0:
            failures.append("verification failed")

    report = {
        "schema_version": SCHEMA_VERSION,
        "generated_at": utc_now(),
        "ok": not failures,
        "bundle": str(bundle),
        "report": str(report_path(workflow_dir)),
        "mode": args.mode,
        "summary": {
            "live_order_attempted": bool(summary.get("live_order_attempted")),
            "live_order_accepted": bool(summary.get("live_order_accepted")),
            "live_order_reason": summary.get("live_order_reason"),
            "receipts_total": summary.get("receipts_total"),
            "receipts_accepted": accepted_receipts,
            "evidence_hash": summary.get("evidence_hash"),
            "exchange_evidence_attached": exchange_attached,
            "exchange_evidence_required": exchange_required,
            "auto_empty_exchange_export": auto_empty_exchange,
            "publishable_canary_evidence": bool(
                summary.get("live_order_accepted") and exchange_attached and not failures
            ),
        },
        "privacy": {
            "raw_private_keys_included": False,
            "raw_exchange_export_included": False,
            "raw_idempotency_key_included": False,
            "raw_confirmation_phrase_included": False,
        },
        "commands": commands,
        "failures": failures,
        "next_actions": next_actions(args, accepted_receipts, exchange_attached, failures),
    }
    write_json(report_path(workflow_dir), report)
    write_sha256s(workflow_dir)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(
            "zero live canary operator: "
            f"ok={report['ok']} bundle={bundle} "
            f"exchange={exchange_attached} report={report_path(workflow_dir)}"
        )
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
    return 0 if report["ok"] else 1


def next_actions(
    args: argparse.Namespace,
    accepted_receipts: int,
    exchange_attached: bool,
    failures: list[str],
) -> list[str]:
    if failures:
        if accepted_receipts > 0 and not exchange_attached:
            return [
                "Export the matching Hyperliquid order/fill JSON from the operator account.",
                "Re-run this command with --bundle and --exchange-export plus --require-exchange-evidence.",
            ]
        return ["Inspect operator_report.json and the canary bundle before sharing evidence."]
    if args.mode == "refusal":
        return ["Refusal evidence is ready for public paper-mode launch proof."]
    if args.mode == "collect-only":
        return ["Collect-only evidence is ready; no live order was submitted."]
    if accepted_receipts > 0 and exchange_attached:
        return ["Publishable canary evidence is ready for review."]
    return ["Run a real canary only after live gates, tiny limits, and operator approvals are ready."]


if __name__ == "__main__":
    raise SystemExit(main())
