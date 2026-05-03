#!/usr/bin/env python3
"""Verify a ZERO live canary operator workflow directory."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_canary_operator_verify.v1"
OPERATOR_SCHEMA_VERSION = "zero.live_canary_operator.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify operator_report.json, workflow checksums, and nested canary evidence."
    )
    parser.add_argument("workflow", type=Path, help="Workflow directory or operator_report.json path.")
    parser.add_argument(
        "--forbid-token",
        action="append",
        default=[],
        help="Additional raw token that must not appear in workflow JSON.",
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


def workflow_paths(path: Path) -> tuple[Path, Path]:
    if path.is_dir():
        return path, path / "operator_report.json"
    return path.parent, path


def body_text(workflow_dir: Path) -> str:
    chunks: list[str] = []
    for path in sorted(workflow_dir.rglob("*.json")):
        if path.is_file():
            chunks.append(path.read_text(encoding="utf-8"))
    return "\n".join(chunks)


def add(findings: list[dict[str, str]], ok: bool, name: str, good: str, bad: str) -> None:
    findings.append({"status": "ok" if ok else "fail", "name": name, "detail": good if ok else bad})


def resolve_report_path(workflow_dir: Path, value: Any) -> Path:
    raw = Path(str(value or ""))
    return raw if raw.is_absolute() else workflow_dir / raw


def run_nested_verifier(report: dict[str, Any], workflow_dir: Path) -> dict[str, Any]:
    bundle = resolve_report_path(workflow_dir, report.get("bundle"))
    mode = str(report.get("mode") or "")
    command = [
        sys.executable,
        str(Path(__file__).resolve().with_name("live_canary_verify.py")),
        str(bundle),
    ]
    if mode:
        command.extend(["--require-mode", mode])
    summary = report.get("summary") if isinstance(report.get("summary"), dict) else {}
    if summary.get("publishable_canary_evidence") or summary.get("live_order_accepted"):
        command.append("--require-live-accepted")
    if summary.get("exchange_evidence_required") or summary.get("exchange_evidence_attached"):
        command.append("--require-exchange-evidence")
    command.append("--json")
    completed = subprocess.run(command, check=False, text=True, capture_output=True)
    payload: dict[str, Any] = {}
    if completed.stdout.strip():
        try:
            loaded = json.loads(completed.stdout)
            payload = loaded if isinstance(loaded, dict) else {}
        except json.JSONDecodeError:
            payload = {}
    return {
        "argv": command,
        "status": completed.returncode,
        "ok": completed.returncode == 0,
        "report": payload,
        "stderr": completed.stderr.strip(),
    }


def verify(args: argparse.Namespace) -> dict[str, Any]:
    workflow_dir, report_path = workflow_paths(args.workflow)
    findings: list[dict[str, str]] = []

    add(findings, workflow_dir.is_dir(), "workflow_dir", str(workflow_dir), "workflow directory missing")
    add(findings, report_path.is_file(), "operator_report", str(report_path), "operator_report.json missing")
    sha_path = workflow_dir / "SHA256SUMS"
    add(findings, sha_path.is_file(), "sha256s", "SHA256SUMS present", "SHA256SUMS missing")
    if not workflow_dir.is_dir() or not report_path.is_file():
        return build_report(workflow_dir, report_path, findings, nested=None)

    report = load_json(report_path)
    summary = report.get("summary") if isinstance(report.get("summary"), dict) else {}
    privacy = report.get("privacy") if isinstance(report.get("privacy"), dict) else {}

    add(
        findings,
        report.get("schema_version") == OPERATOR_SCHEMA_VERSION,
        "schema_version",
        OPERATOR_SCHEMA_VERSION,
        f"expected {OPERATOR_SCHEMA_VERSION}",
    )
    for flag in (
        "raw_private_keys_included",
        "raw_exchange_export_included",
        "raw_idempotency_key_included",
        "raw_confirmation_phrase_included",
    ):
        add(findings, privacy.get(flag) is False, f"privacy:{flag}", "omitted", f"{flag} must be false")
    add(
        findings,
        bool(report.get("ok")) is (not report.get("failures")),
        "ok_matches_failures",
        "ok flag matches failures",
        "ok flag does not match failures",
    )
    add(
        findings,
        not summary.get("live_order_accepted") or bool(summary.get("exchange_evidence_attached")),
        "accepted_requires_exchange_evidence",
        "accepted receipt has exchange evidence",
        "accepted live receipt without exchange evidence",
    )

    if sha_path.is_file():
        entries = read_sha256s(sha_path)
        expected = {
            path.relative_to(workflow_dir).as_posix()
            for path in workflow_dir.rglob("*")
            if path.is_file() and path.relative_to(workflow_dir).as_posix() != "SHA256SUMS"
        }
        add(
            findings,
            set(entries) == expected,
            "sha256_inventory",
            f"{len(entries)} entries",
            "SHA256SUMS does not match workflow files",
        )
        for name, expected_digest in sorted(entries.items()):
            path = workflow_dir / name
            add(
                findings,
                path.is_file() and sha256(path) == expected_digest,
                f"sha256:{name}",
                "hash matches",
                "hash mismatch or file missing",
            )

    text = body_text(workflow_dir)
    secret_patterns = {
        "raw_trace_id": r"trace-[A-Za-z0-9._:-]+",
        "bearer_token": r"Bearer\s+(?!REDACTED\b)[A-Za-z0-9._~+/=-]+",
        "openai_key": r"sk-[A-Za-z0-9]{16,}",
        "raw_live_confirmation": r"I_UNDERSTAND_THIS_CAN_PLACE_A_REAL_HYPERLIQUID_ORDER",
    }
    for name, pattern in secret_patterns.items():
        add(findings, re.search(pattern, text) is None, f"redaction:{name}", "not present", f"{name} found")
    for idx, token in enumerate(args.forbid_token, start=1):
        add(findings, token not in text, f"redaction:forbid_token_{idx}", "not present", "forbidden token found")

    nested = run_nested_verifier(report, workflow_dir)
    add(
        findings,
        nested["ok"],
        "nested_live_canary_verify",
        "nested canary bundle verifies",
        nested.get("stderr") or "nested canary bundle verification failed",
    )
    return build_report(workflow_dir, report_path, findings, nested=nested)


def build_report(
    workflow_dir: Path,
    report_path: Path,
    findings: list[dict[str, str]],
    *,
    nested: dict[str, Any] | None,
) -> dict[str, Any]:
    fail = len([finding for finding in findings if finding["status"] == "fail"])
    return {
        "schema_version": SCHEMA_VERSION,
        "workflow": str(workflow_dir),
        "operator_report": str(report_path),
        "ok": fail == 0,
        "summary": {
            "ok": len([finding for finding in findings if finding["status"] == "ok"]),
            "fail": fail,
        },
        "nested": nested,
        "findings": findings,
    }


def render_text(report: dict[str, Any]) -> str:
    summary = report["summary"]
    header = (
        f"zero live canary operator verify: ok={report['ok']} "
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
    report = verify(args)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(render_text(report))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
