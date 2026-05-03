#!/usr/bin/env python3
"""Verify a ZERO live cockpit drill bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_cockpit_drill_verify.v1"
DRILL_SCHEMA_VERSION = "zero.live_cockpit_drill.v1"

PACKETS: tuple[tuple[str, str, str | None], ...] = (
    ("01_health", "01_health.json", None),
    ("02_v2_status", "02_v2_status.json", None),
    ("03_live_preflight", "03_live_preflight.json", "zero.live_preflight.v1"),
    ("04_live_cockpit", "04_live_cockpit.json", "zero.live_cockpit.v1"),
    ("05_immune", "05_immune.json", "zero.immune.v1"),
    ("06_hl_reconcile", "06_hl_reconcile.json", "zero.reconciliation.v1"),
    ("07_live_certification", "07_live_certification.json", "zero.live_certification.v1"),
    ("08_live_receipts", "08_live_receipts.json", "zero.live_execution_receipts.v1"),
    ("09_live_evidence", "09_live_evidence.json", "zero.live_evidence.v1"),
    ("10_live_canary_policy", "10_live_canary_policy.json", "zero.live_canary_policy.v1"),
    ("11_metrics", "11_metrics.json", "zero.metrics.v1"),
    ("12_audit_export", "12_audit_export.json", "zero.audit.v1"),
)


@dataclass(frozen=True)
class Finding:
    status: str
    name: str
    detail: str

    def to_dict(self) -> dict[str, str]:
        return {"status": self.status, "name": self.name, "detail": self.detail}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Verify a local live cockpit drill bundle before sharing it or "
            "using it as operator-readiness evidence."
        )
    )
    parser.add_argument("bundle", type=Path, help="Bundle directory from live_cockpit_drill.py.")
    parser.add_argument(
        "--expect-refusal",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Require the public paper safety posture: live_mode=refused and ready=false.",
    )
    parser.add_argument(
        "--require-ok",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Require manifest summary ok=true and fail=0.",
    )
    parser.add_argument(
        "--forbid-token",
        action="append",
        default=[],
        help="Additional raw token that must not appear in bundle JSON.",
    )
    parser.add_argument("--json", action="store_true", help="Emit the verification report as JSON.")
    return parser.parse_args()


def add(findings: list[Finding], ok: bool, name: str, good: str, bad: str) -> None:
    findings.append(Finding("ok" if ok else "fail", name, good if ok else bad))


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_sha256s(path: Path) -> dict[str, str]:
    entries: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        if not raw.strip():
            continue
        digest, separator, name = raw.partition("  ")
        if separator:
            entries[name] = digest
    return entries


def packet_payload(bundle: Path, file_name: str) -> dict[str, Any]:
    packet = load_json(bundle / file_name)
    payload = packet.get("payload") if isinstance(packet, dict) else None
    return payload if isinstance(payload, dict) else {}


def packet_status(bundle: Path, file_name: str) -> int | None:
    packet = load_json(bundle / file_name)
    status = packet.get("status") if isinstance(packet, dict) else None
    return status if isinstance(status, int) else None


def body_text(bundle: Path) -> str:
    chunks: list[str] = []
    for path in sorted(bundle.iterdir()):
        if path.is_file() and path.suffix == ".json":
            chunks.append(path.read_text(encoding="utf-8"))
    return "\n".join(chunks)


def replay_summary(bundle: Path) -> dict[str, Any]:
    preflight = packet_payload(bundle, "03_live_preflight.json")
    cockpit = packet_payload(bundle, "04_live_cockpit.json")
    immune = packet_payload(bundle, "05_immune.json")
    reconciliation = packet_payload(bundle, "06_hl_reconcile.json")
    certification = packet_payload(bundle, "07_live_certification.json")
    receipts = packet_payload(bundle, "08_live_receipts.json")
    evidence = packet_payload(bundle, "09_live_evidence.json")
    canary_policy = packet_payload(bundle, "10_live_canary_policy.json")
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


def verify_bundle(args: argparse.Namespace) -> dict[str, Any]:
    bundle = args.bundle
    findings: list[Finding] = []

    add(findings, bundle.is_dir(), "bundle_dir", str(bundle), "bundle directory missing")
    manifest_path = bundle / "manifest.json"
    sha_path = bundle / "SHA256SUMS"
    add(findings, manifest_path.is_file(), "manifest", "manifest.json present", "manifest.json missing")
    add(findings, sha_path.is_file(), "sha256s", "SHA256SUMS present", "SHA256SUMS missing")
    if not bundle.is_dir() or not manifest_path.is_file() or not sha_path.is_file():
        return build_report(args, findings, manifest=None, replay=None)

    manifest = load_json(manifest_path)
    summary = manifest.get("summary", {}) if isinstance(manifest, dict) else {}
    packets = manifest.get("packets", []) if isinstance(manifest, dict) else []
    files = manifest.get("files", []) if isinstance(manifest, dict) else []

    add(
        findings,
        manifest.get("schema_version") == DRILL_SCHEMA_VERSION,
        "schema_version",
        DRILL_SCHEMA_VERSION,
        f"expected {DRILL_SCHEMA_VERSION}",
    )
    if args.require_ok:
        add(
            findings,
            summary.get("ok") is True and summary.get("fail") == 0,
            "manifest_ok",
            "manifest ok=true fail=0",
            "manifest reports failed drill checks",
        )

    sha_entries = read_sha256s(sha_path)
    expected_files = {path.name for path in bundle.iterdir() if path.is_file() and path.name != "SHA256SUMS"}
    add(
        findings,
        set(sha_entries) == expected_files,
        "sha256_inventory",
        f"{len(sha_entries)} entries",
        "SHA256SUMS does not match bundle files",
    )
    for name, expected in sorted(sha_entries.items()):
        path = bundle / name
        add(
            findings,
            path.is_file() and sha256(path) == expected,
            f"sha256:{name}",
            "hash matches",
            "hash mismatch or file missing",
        )

    expected_packet_files = {file_name for _, file_name, _ in PACKETS}
    packet_files = {str(packet.get("file")) for packet in packets if isinstance(packet, dict)}
    add(
        findings,
        expected_packet_files == packet_files,
        "packet_inventory",
        "expected packet files present",
        "manifest packet inventory is incomplete",
    )
    inventory_files = {str(item.get("path")) for item in files if isinstance(item, dict)}
    add(
        findings,
        expected_packet_files.issubset(inventory_files),
        "file_inventory",
        "packet file inventory present",
        "manifest files inventory is incomplete",
    )

    for name, file_name, schema in PACKETS:
        path = bundle / file_name
        add(findings, path.is_file(), f"packet_file:{name}", file_name, f"{file_name} missing")
        if not path.is_file():
            continue
        add(
            findings,
            packet_status(bundle, file_name) == 200,
            f"packet_status:{name}",
            "http 200",
            f"expected http 200, got {packet_status(bundle, file_name)}",
        )
        if schema is not None:
            payload = packet_payload(bundle, file_name)
            add(
                findings,
                payload.get("schema_version") == schema,
                f"packet_schema:{name}",
                schema,
                f"expected schema_version={schema}",
            )

    replay = replay_summary(bundle)
    for key, value in replay.items():
        add(
            findings,
            summary.get(key) == value,
            f"replay:{key}",
            "matches packet payloads",
            f"manifest has {summary.get(key)!r}, replay has {value!r}",
        )

    cockpit = packet_payload(bundle, "04_live_cockpit.json")
    preflight = packet_payload(bundle, "03_live_preflight.json")
    immune = packet_payload(bundle, "05_immune.json")
    certification = packet_payload(bundle, "07_live_certification.json")
    canary_policy = packet_payload(bundle, "10_live_canary_policy.json")
    if args.expect_refusal:
        add(
            findings,
            preflight.get("ready") is False and preflight.get("live_mode") == "refused",
            "refusal:preflight",
            "preflight fail-closed",
            "preflight did not report refused ready=false",
        )
        add(
            findings,
            cockpit.get("ready") is False
            and cockpit.get("live_mode") == "refused"
            and cockpit.get("risk_increasing_allowed") is False,
            "refusal:cockpit",
            "cockpit fail-closed",
            "cockpit did not report refused ready=false risk=false",
        )
        add(
            findings,
            immune.get("risk_increasing_allowed") is False,
            "refusal:immune",
            "immune blocks risk",
            "immune unexpectedly allows risk",
        )
    add(
        findings,
        certification.get("mode") == "dry_run" and certification.get("passed") is True,
        "certification",
        "dry-run certification passed",
        "live certification did not pass dry-run",
    )
    add(
        findings,
        canary_policy.get("summary", {}).get("policy_armed") is False,
        "canary_policy",
        "canary policy disarmed",
        "canary policy unexpectedly armed",
    )

    text = body_text(bundle)
    secret_patterns = {
        "raw_trace_id": r"trace-[A-Za-z0-9._:-]+",
        "bearer_token": r"Bearer\s+(?!REDACTED\b)[A-Za-z0-9._~+/=-]+",
        "openai_key": r"sk-[A-Za-z0-9]{16,}",
        "private_key": r"(?i)private[_-]?key[\"']?\s*[:=]\s*[\"']?(?!REDACTED\b)[^\"'\s,}]+",
    }
    for name, pattern in secret_patterns.items():
        add(findings, re.search(pattern, text) is None, f"redaction:{name}", "not present", f"{name} found")
    for idx, token in enumerate(args.forbid_token, start=1):
        add(
            findings,
            token not in text,
            f"redaction:forbid_token_{idx}",
            "not present",
            "forbidden token found",
        )

    return build_report(args, findings, manifest=manifest, replay=replay)


def build_report(
    args: argparse.Namespace,
    findings: list[Finding],
    *,
    manifest: dict[str, Any] | None,
    replay: dict[str, Any] | None,
) -> dict[str, Any]:
    fail = len([finding for finding in findings if finding.status == "fail"])
    return {
        "schema_version": SCHEMA_VERSION,
        "bundle": str(args.bundle),
        "ok": fail == 0,
        "summary": {
            "ok": len([finding for finding in findings if finding.status == "ok"]),
            "fail": fail,
        },
        "manifest": {
            "schema_version": None if manifest is None else manifest.get("schema_version"),
            "created_at": None if manifest is None else manifest.get("created_at"),
            "ready": None if manifest is None else manifest.get("summary", {}).get("ready"),
            "risk_increasing_allowed": None
            if manifest is None
            else manifest.get("summary", {}).get("risk_increasing_allowed"),
        },
        "replay": replay or {},
        "findings": [finding.to_dict() for finding in findings],
    }


def render_text(report: dict[str, Any]) -> str:
    summary = report["summary"]
    header = (
        f"zero live cockpit drill verify: ok={report['ok']} "
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
    report = verify_bundle(args)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(render_text(report))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
