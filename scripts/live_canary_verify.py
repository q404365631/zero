#!/usr/bin/env python3
"""Verify a ZERO live canary rehearsal bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_canary_verify.v1"
REHEARSAL_SCHEMA_VERSION = "zero.live_canary_rehearsal.v1"
EXCHANGE_EVIDENCE_SCHEMA_VERSION = "zero.live_canary_exchange_evidence.v1"


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
            "Verify a local live canary rehearsal bundle before sharing it or "
            "using it as launch evidence."
        )
    )
    parser.add_argument("bundle", type=Path, help="Bundle directory from live_canary_rehearsal.py.")
    parser.add_argument(
        "--require-mode",
        choices=("refusal", "collect-only", "canary"),
        default=None,
        help="Require the manifest collector mode to match.",
    )
    parser.add_argument(
        "--require-live-accepted",
        action="store_true",
        help="Require the bundle to contain an accepted live canary order.",
    )
    parser.add_argument(
        "--require-exchange-evidence",
        action="store_true",
        help="Require exchange_evidence.json and verify it against ZERO receipts.",
    )
    parser.add_argument(
        "--forbid-token",
        action="append",
        default=[],
        help="Additional raw token that must not appear in bundle JSON.",
    )
    parser.add_argument("--json", action="store_true", help="Emit the verification report as JSON.")
    return parser.parse_args()


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
        digest, _, name = raw.partition("  ")
        entries[name] = digest
    return entries


def body_text(bundle: Path) -> str:
    chunks: list[str] = []
    for path in sorted(bundle.iterdir()):
        if path.is_file() and path.suffix == ".json":
            chunks.append(path.read_text(encoding="utf-8"))
    return "\n".join(chunks)


def packet_payload(bundle: Path, file_name: str) -> dict[str, Any]:
    packet = load_json(bundle / file_name)
    payload = packet.get("payload") if isinstance(packet, dict) else None
    return payload if isinstance(payload, dict) else {}


def add(findings: list[Finding], ok: bool, name: str, good: str, bad: str) -> None:
    findings.append(Finding("ok" if ok else "fail", name, good if ok else bad))


def verify_bundle(args: argparse.Namespace) -> dict[str, Any]:
    bundle = args.bundle
    findings: list[Finding] = []

    add(findings, bundle.is_dir(), "bundle_dir", str(bundle), "bundle directory missing")
    manifest_path = bundle / "manifest.json"
    sha_path = bundle / "SHA256SUMS"
    add(findings, manifest_path.is_file(), "manifest", "manifest.json present", "manifest.json missing")
    add(findings, sha_path.is_file(), "sha256s", "SHA256SUMS present", "SHA256SUMS missing")
    if not bundle.is_dir() or not manifest_path.is_file() or not sha_path.is_file():
        return build_report(args, findings, manifest=None)

    manifest = load_json(manifest_path)
    summary = manifest.get("summary", {}) if isinstance(manifest, dict) else {}
    policy = manifest.get("policy", {}) if isinstance(manifest, dict) else {}
    steps = manifest.get("steps", []) if isinstance(manifest, dict) else []
    mode = manifest.get("collector", {}).get("mode") if isinstance(manifest, dict) else None

    add(
        findings,
        manifest.get("schema_version") == REHEARSAL_SCHEMA_VERSION,
        "schema_version",
        REHEARSAL_SCHEMA_VERSION,
        f"expected {REHEARSAL_SCHEMA_VERSION}",
    )
    if args.require_mode:
        add(
            findings,
            mode == args.require_mode,
            "required_mode",
            args.require_mode,
            f"expected mode={args.require_mode}, got {mode}",
        )
    add(
        findings,
        policy.get("schema_version") == "zero.live_canary_policy.v1",
        "policy_schema",
        "zero.live_canary_policy.v1",
        "manifest missing live canary policy",
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

    required_step_names = {
        "01_live_preflight",
        "02_live_heartbeat",
        "03_live_cockpit",
        "04_live_certification",
        "05_hl_reconcile",
        "90_live_receipts",
        "91_live_evidence",
        "92_metrics",
        "93_audit_export",
    }
    step_names = {str(step.get("name")) for step in steps if isinstance(step, dict)}
    add(
        findings,
        required_step_names.issubset(step_names),
        "required_steps",
        "required packet sequence present",
        "required packet sequence incomplete",
    )
    for step in steps:
        if not isinstance(step, dict):
            add(findings, False, "step_shape", "step ok", "step is not an object")
            continue
        file_name = str(step.get("file", ""))
        status = step.get("status")
        add(
            findings,
            bool(file_name) and (bundle / file_name).is_file(),
            f"step_file:{step.get('name')}",
            file_name,
            f"missing step file {file_name}",
        )
        add(
            findings,
            status == 200,
            f"step_status:{step.get('name')}",
            "http 200",
            f"expected http 200, got {status}",
        )

    text = body_text(bundle)
    secret_patterns = {
        "raw_trace_id": r"trace-[A-Za-z0-9._:-]+",
        "bearer_token": r"Bearer\s+(?!REDACTED\b)[A-Za-z0-9._~+/=-]+",
        "openai_key": r"sk-[A-Za-z0-9]{16,}",
        "raw_idempotency": r"smoke-live-canary-refusal|canary-\d{8}T\d{6}Z-[A-Z]+-(buy|sell)",
    }
    for name, pattern in secret_patterns.items():
        add(
            findings,
            re.search(pattern, text) is None,
            f"redaction:{name}",
            "not present",
            f"unredacted {name} found",
        )
    for idx, token in enumerate(args.forbid_token, start=1):
        add(
            findings,
            token not in text,
            f"redaction:forbid_token_{idx}",
            "not present",
            "forbidden token found",
        )

    receipts = packet_payload(bundle, "90_live_receipts.json")
    evidence = packet_payload(bundle, "91_live_evidence.json")
    receipt_summary = receipts.get("summary", {}) if isinstance(receipts, dict) else {}
    add(
        findings,
        summary.get("receipts_total") == receipt_summary.get("total"),
        "receipts_total",
        "matches manifest",
        "manifest receipts_total does not match receipts packet",
    )
    add(
        findings,
        summary.get("receipts_accepted") == receipt_summary.get("accepted"),
        "receipts_accepted",
        "matches manifest",
        "manifest receipts_accepted does not match receipts packet",
    )
    add(
        findings,
        bool(summary.get("evidence_hash"))
        and summary.get("evidence_hash") == evidence.get("evidence_hash"),
        "evidence_hash",
        "matches manifest",
        "manifest evidence_hash does not match live evidence packet",
    )

    if mode == "refusal":
        add(
            findings,
            bool(summary.get("live_order_attempted")),
            "refusal_attempted",
            "live request attempted",
            "refusal mode did not attempt a live request",
        )
        add(
            findings,
            summary.get("live_order_accepted") is False,
            "refusal_closed",
            "live request refused",
            "refusal mode accepted live risk",
        )
    if args.require_live_accepted:
        add(
            findings,
            summary.get("live_order_accepted") is True,
            "live_order_accepted",
            "accepted live canary present",
            "accepted live canary missing",
        )
    if mode == "canary":
        add(
            findings,
            bool(summary.get("risk_ready")),
            "canary_risk_ready",
            "risk gates were ready",
            "canary mode ran without ready risk gates",
        )

    exchange_evidence_path = bundle / "exchange_evidence.json"
    if exchange_evidence_path.is_file():
        verify_exchange_evidence(bundle, exchange_evidence_path, findings)
    elif args.require_exchange_evidence:
        add(
            findings,
            False,
            "exchange_evidence",
            "exchange_evidence.json present",
            "exchange_evidence.json missing",
        )

    return build_report(args, findings, manifest=manifest)


def verify_exchange_evidence(bundle: Path, path: Path, findings: list[Finding]) -> None:
    payload = load_json(path)
    receipts_payload = packet_payload(bundle, "90_live_receipts.json")
    receipt_summary = receipts_payload.get("summary", {}) if isinstance(receipts_payload, dict) else {}
    summary = payload.get("summary", {}) if isinstance(payload, dict) else {}
    privacy = payload.get("privacy", {}) if isinstance(payload, dict) else {}
    source = payload.get("source", {}) if isinstance(payload, dict) else {}

    add(
        findings,
        payload.get("schema_version") == EXCHANGE_EVIDENCE_SCHEMA_VERSION,
        "exchange_evidence_schema",
        EXCHANGE_EVIDENCE_SCHEMA_VERSION,
        f"expected {EXCHANGE_EVIDENCE_SCHEMA_VERSION}",
    )
    add(
        findings,
        source.get("raw_included") is False,
        "exchange_evidence_raw_source",
        "raw source omitted",
        "raw source must not be included",
    )
    add(
        findings,
        "file_name" not in source and bool(source.get("file_name_hash")),
        "exchange_evidence_source_name",
        "source filename hashed",
        "source filename must be hashed, not embedded",
    )
    privacy_flags = (
        "wallet_addresses_included",
        "raw_order_ids_included",
        "raw_client_order_ids_included",
        "raw_venue_payload_included",
    )
    for flag in privacy_flags:
        add(
            findings,
            privacy.get(flag) is False,
            f"exchange_evidence_privacy:{flag}",
            "omitted",
            f"{flag} must be false",
        )
    add(
        findings,
        summary.get("accepted_receipts") == receipt_summary.get("accepted"),
        "exchange_evidence_receipt_count",
        "matches live receipts",
        "accepted receipt count does not match live receipts",
    )
    add(
        findings,
        summary.get("complete") is True,
        "exchange_evidence_complete",
        "all accepted receipts matched",
        "one or more accepted receipts lack exchange-side evidence",
    )


def build_report(
    args: argparse.Namespace,
    findings: list[Finding],
    *,
    manifest: dict[str, Any] | None,
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
            "mode": None if manifest is None else manifest.get("collector", {}).get("mode"),
            "generated_at": None if manifest is None else manifest.get("generated_at"),
        },
        "findings": [finding.to_dict() for finding in findings],
    }


def render_text(report: dict[str, Any]) -> str:
    summary = report["summary"]
    header = (
        f"zero live canary verify: ok={report['ok']} "
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
