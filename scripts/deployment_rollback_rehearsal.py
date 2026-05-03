#!/usr/bin/env python3
"""Build a public-safe deployment rollback rehearsal from evidence packs."""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import os
import sys
from argparse import Namespace
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from deployment_evidence_verify import verify as verify_evidence


SCHEMA_VERSION = "zero.deployment_rollback_rehearsal.v1"
SIGNATURE_SCHEMA_VERSION = "zero.deployment_rollback_rehearsal_signature.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Verify current and rollback-target deployment evidence packs, then "
            "emit a plan-only rollback rehearsal report. The script never calls "
            "Railway or changes a deployment."
        )
    )
    parser.add_argument("current_bundle", type=Path, help="Evidence pack for the current deployment.")
    parser.add_argument(
        "--previous-bundle",
        type=Path,
        required=True,
        help="Evidence pack for the rollback target deployment.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output directory. Defaults to artifacts/deployment-rollback-rehearsal/<timestamp>.",
    )
    parser.add_argument(
        "--signing-key",
        default=os.environ.get("ZERO_DEPLOYMENT_EVIDENCE_SIGNING_KEY", ""),
        help="Optional HMAC-SHA256 key used to verify signed evidence packs and sign the rehearsal.",
    )
    parser.add_argument(
        "--require-signature",
        action="store_true",
        help="Require both evidence packs to have signatures verifiable with --signing-key.",
    )
    parser.add_argument(
        "--signer",
        default=os.environ.get("ZERO_DEPLOYMENT_EVIDENCE_SIGNER", "local-operator"),
        help="Signer label to include when --signing-key is supplied.",
    )
    parser.add_argument(
        "--forbid-token",
        action="append",
        default=[],
        help="Additional raw token that must not appear in either evidence pack.",
    )
    parser.add_argument("--json", action="store_true", help="Emit the rehearsal report as JSON.")
    return parser.parse_args()


def default_output_dir() -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path("artifacts") / "deployment-rollback-rehearsal" / stamp


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def evidence_args(args: argparse.Namespace, bundle: Path) -> Namespace:
    return Namespace(
        bundle=bundle,
        signing_key=args.signing_key,
        require_signature=args.require_signature,
        forbid_token=args.forbid_token,
        json=True,
    )


def packet_payload(bundle: Path, manifest: dict[str, Any], name: str) -> Any:
    packets = manifest.get("packets", [])
    if not isinstance(packets, list):
        return None
    for packet in packets:
        if not isinstance(packet, dict) or packet.get("name") != name:
            continue
        file_name = packet.get("file")
        if not isinstance(file_name, str):
            return None
        wrapper = load_json(bundle / file_name)
        if isinstance(wrapper, dict):
            return wrapper.get("payload")
    return None


def check_paper_safe(bundle: Path, manifest: dict[str, Any], label: str) -> list[dict[str, Any]]:
    checks: list[dict[str, Any]] = []

    def add(ok: bool, name: str, detail: str) -> None:
        checks.append({"status": "ok" if ok else "fail", "name": f"{label}:{name}", "detail": detail})

    preflight = packet_payload(bundle, manifest, "live_preflight")
    cockpit = packet_payload(bundle, manifest, "live_cockpit")
    heartbeat = packet_payload(bundle, manifest, "deployment_heartbeat")
    health = packet_payload(bundle, manifest, "health")

    add(isinstance(preflight, dict), "live_preflight_present", "live preflight packet is present")
    if isinstance(preflight, dict):
        add(preflight.get("ready") is False, "live_preflight_ready_false", "live readiness is refused")
        add(preflight.get("live_mode") == "refused", "live_preflight_refused", "live mode is refused")

    add(isinstance(cockpit, dict), "live_cockpit_present", "live cockpit packet is present")
    if isinstance(cockpit, dict):
        add(cockpit.get("ready") is False, "live_cockpit_ready_false", "cockpit is not live-ready")
        add(
            cockpit.get("risk_increasing_allowed") is False,
            "live_cockpit_risk_blocked",
            "risk-increasing actions are blocked",
        )

    add(isinstance(heartbeat, dict), "deployment_heartbeat_present", "deployment heartbeat packet is present")
    if isinstance(heartbeat, dict):
        liveness = heartbeat.get("liveness", {})
        if not isinstance(liveness, dict):
            liveness = {}
        add(liveness.get("status") == "paper_only", "heartbeat_paper_only", "heartbeat reports paper_only")
        add(
            liveness.get("live_executor_configured") is False,
            "heartbeat_live_executor_disabled",
            "live executor is disabled",
        )

    add(isinstance(health, dict) and health.get("status") == "ok", "health_ok", "health packet is ok")
    return checks


def bundle_summary(args: argparse.Namespace, bundle: Path, label: str) -> dict[str, Any]:
    report = verify_evidence(evidence_args(args, bundle))
    manifest = load_json(bundle / "manifest.json") if (bundle / "manifest.json").is_file() else {}
    paper_checks = check_paper_safe(bundle, manifest, label) if isinstance(manifest, dict) else []
    signature = report.get("signature", {})
    return {
        "label": label,
        "bundle": str(bundle),
        "target": manifest.get("target") if isinstance(manifest, dict) else None,
        "generated_at": manifest.get("generated_at") if isinstance(manifest, dict) else None,
        "git": manifest.get("git") if isinstance(manifest, dict) else None,
        "manifest_sha256": "sha256:" + sha256(bundle / "manifest.json") if (bundle / "manifest.json").is_file() else None,
        "sha256s_sha256": "sha256:" + sha256(bundle / "SHA256SUMS") if (bundle / "SHA256SUMS").is_file() else None,
        "evidence_verification": {
            "ok": report.get("ok"),
            "summary": report.get("summary"),
            "signature_present": signature.get("present"),
            "signature_signer": signature.get("signer"),
        },
        "paper_safety_checks": paper_checks,
    }


def sign_report(report: dict[str, Any], signing_key: str, signer: str) -> dict[str, Any]:
    payload = {
        "schema_version": "zero.deployment_rollback_rehearsal_signature_payload.v1",
        "report_schema_version": report["schema_version"],
        "current_manifest_sha256": report["current"]["manifest_sha256"],
        "rollback_target_manifest_sha256": report["rollback_target"]["manifest_sha256"],
        "rollback_plan_hash": report["rollback_plan"]["plan_hash"],
    }
    payload_hash = "sha256:" + hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    signature = hmac.new(signing_key.encode("utf-8"), payload_hash.encode("utf-8"), hashlib.sha256).hexdigest()
    return {
        "schema_version": SIGNATURE_SCHEMA_VERSION,
        "algorithm": "hmac-sha256",
        "signer": signer,
        "signed_payload_hash": payload_hash,
        "signature": f"v1={signature}",
        "key_material_included": False,
        "signed_payload": payload,
    }


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    current = bundle_summary(args, args.current_bundle, "current")
    previous = bundle_summary(args, args.previous_bundle, "rollback_target")
    checks = [
        {
            "status": "ok" if current["evidence_verification"]["ok"] else "fail",
            "name": "current:evidence_verified",
            "detail": "current evidence pack verifies",
        },
        {
            "status": "ok" if previous["evidence_verification"]["ok"] else "fail",
            "name": "rollback_target:evidence_verified",
            "detail": "rollback target evidence pack verifies",
        },
        *current["paper_safety_checks"],
        *previous["paper_safety_checks"],
    ]
    fail = len([check for check in checks if check["status"] == "fail"])
    plan = {
        "mode": "plan_only",
        "remote_mutation_performed": False,
        "rollback_ready": fail == 0,
        "same_bundle_rehearsal": args.current_bundle.resolve() == args.previous_bundle.resolve(),
        "guardrails": [
            "verify current evidence before rollback",
            "verify rollback-target evidence before rollback",
            "keep public Railway paper deployments live_mode=refused",
            "capture a fresh signed evidence pack immediately after rollback",
            "run scripts/railway_doctor.py after rollback",
            "do not place private exchange keys into public paper services",
        ],
        "operator_steps": [
            "select the previous known-good Railway deployment",
            "perform Railway rollback from the Railway dashboard or CLI",
            "wait for /health to return status=ok",
            "run scripts/railway_doctor.py against the rolled-back URL",
            "run scripts/deployment_evidence.sh with --railway-logs and --signing-key",
            "verify the new evidence with scripts/deployment_evidence_verify.py --require-signature",
        ],
    }
    plan_hash = "sha256:" + hashlib.sha256(
        json.dumps(plan, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    plan["plan_hash"] = plan_hash
    return {
        "schema_version": SCHEMA_VERSION,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "ok": fail == 0,
        "summary": {"ok": len(checks) - fail, "fail": fail},
        "current": current,
        "rollback_target": previous,
        "rollback_plan": plan,
        "checks": checks,
    }


def write_sha256s(output_dir: Path) -> None:
    lines = []
    for path in sorted(output_dir.iterdir()):
        if path.is_file() and path.name != "SHA256SUMS":
            lines.append(f"{sha256(path)}  {path.name}")
    (output_dir / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def render_text(report: dict[str, Any], output: Path) -> str:
    summary = report["summary"]
    lines = [
        (
            f"zero deployment rollback rehearsal: ok={report['ok']} "
            f"checks={summary['ok']} fail={summary['fail']} output={output}"
        )
    ]
    for check in report["checks"]:
        if check["status"] == "fail":
            lines.append(f"- {check['name']}: {check['detail']}")
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    output_dir = args.output or default_output_dir()
    output_dir.mkdir(parents=True, exist_ok=True)
    report = build_report(args)
    write_json(output_dir / "rollback_rehearsal.json", report)
    if args.signing_key:
        write_json(output_dir / "ROLLBACK_REHEARSAL_SIGNATURE.json", sign_report(report, args.signing_key, args.signer))
    write_sha256s(output_dir)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(render_text(report, output_dir))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
