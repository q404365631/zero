#!/usr/bin/env python3
"""Collect a redacted deployment evidence pack for a ZERO paper service."""

from __future__ import annotations

import argparse
import hmac
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from railway_doctor import build_report, normalize_base_url, run_checks


SCHEMA_VERSION = "zero.deployment_evidence.v1"
SIGNATURE_SCHEMA_VERSION = "zero.deployment_evidence_signature.v1"
DEFAULT_TIMEOUT_SECONDS = 8.0
DEFAULT_AUDIT_LIMIT = 100
DEFAULT_LOG_LINES = 200

PACKETS: tuple[tuple[str, str], ...] = (
    ("health", "/health"),
    ("v2_status", "/v2/status"),
    ("metrics", "/metrics"),
    ("audit_export", f"/audit/export?limit={DEFAULT_AUDIT_LIMIT}"),
    ("immune", "/immune"),
    ("live_preflight", "/live/preflight"),
    ("live_cockpit", "/live/cockpit"),
    ("live_certification", "/live/certification"),
    ("live_receipts", "/live/receipts"),
    ("deployment_claim", "/deployment/claim"),
    ("deployment_heartbeat", "/deployment/heartbeat"),
    ("network_profile", "/network/profile"),
    ("intelligence_snapshot", "/intelligence/snapshot"),
    ("hosted_intelligence_snapshots", "/v1/intelligence/snapshots"),
)

REDACTION_PATTERNS: tuple[tuple[str, str], ...] = (
    (r"Bearer\s+[A-Za-z0-9._~+/=-]+", "Bearer REDACTED"),
    (r"(?i)(authorization[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"(?i)(api[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"(?i)(private[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"(?i)(signing[_-]?key[\"']?\s*[:=]\s*[\"']?)[^\"'\s,}]+", r"\1REDACTED"),
    (r"trace-[A-Za-z0-9._:-]+", "TRACE_REDACTED"),
    (r"railway-[A-Za-z0-9._:-]+", "railway-REDACTED"),
    (r"smoke-[A-Za-z0-9._:-]+", "smoke-REDACTED"),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Collect a redacted evidence folder for a ZERO Railway-style paper "
            "deployment. The pack is safe to attach to launch reviews, incident "
            "notes, and deployment promotion records."
        )
    )
    parser.add_argument(
        "url",
        nargs="?",
        default=os.environ.get("ZERO_RAILWAY_URL", ""),
        help="Base URL for the deployment. Defaults to ZERO_RAILWAY_URL.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output directory. Defaults to artifacts/deployment-evidence/<timestamp>.",
    )
    parser.add_argument(
        "--token",
        default=os.environ.get("ZERO_INTELLIGENCE_API_TOKEN", ""),
        help="Optional ZERO Intelligence API bearer token for tokened doctor checks.",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_SECONDS,
        help="HTTP timeout per request in seconds.",
    )
    parser.add_argument(
        "--expect-paper",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Require public paper safety posture in the embedded doctor.",
    )
    parser.add_argument(
        "--fail-on-warn",
        action="store_true",
        help="Exit nonzero when the embedded doctor has warnings.",
    )
    parser.add_argument(
        "--allow-failures",
        action="store_true",
        help="Write the evidence pack even when the embedded doctor has failures.",
    )
    parser.add_argument(
        "--railway-logs",
        action="store_true",
        help="Best-effort capture of `railway logs` output when Railway CLI is authenticated.",
    )
    parser.add_argument(
        "--railway-log-lines",
        type=int,
        default=DEFAULT_LOG_LINES,
        help="Number of Railway log lines to request when --railway-logs is set.",
    )
    parser.add_argument(
        "--signing-key",
        default=os.environ.get("ZERO_DEPLOYMENT_EVIDENCE_SIGNING_KEY", ""),
        help=(
            "Optional HMAC-SHA256 signing key for the evidence pack. The key is "
            "never written; only EVIDENCE_SIGNATURE.json is emitted."
        ),
    )
    parser.add_argument(
        "--signer",
        default=os.environ.get("ZERO_DEPLOYMENT_EVIDENCE_SIGNER", "local-operator"),
        help="Signer label to include in EVIDENCE_SIGNATURE.json when signing is enabled.",
    )
    return parser.parse_args()


def default_output_dir() -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path("artifacts") / "deployment-evidence" / stamp


def redact_text(text: str, secrets: tuple[str, ...]) -> str:
    redacted = text
    for secret in secrets:
        if secret:
            redacted = redacted.replace(secret, "REDACTED")
    for pattern, replacement in REDACTION_PATTERNS:
        redacted = re.sub(pattern, replacement, redacted)
    return redacted


def redact_json(value: Any, secrets: tuple[str, ...]) -> Any:
    return json.loads(redact_text(json.dumps(value, sort_keys=True), secrets))


def write_json(path: Path, payload: Any, secrets: tuple[str, ...]) -> None:
    path.write_text(
        json.dumps(redact_json(payload, secrets), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_text(path: Path, text: str, secrets: tuple[str, ...]) -> None:
    path.write_text(redact_text(text, secrets), encoding="utf-8")


def fetch_packet(base_url: str, path: str, *, timeout: float) -> dict[str, Any]:
    request = urllib.request.Request(
        f"{base_url}{path}",
        headers={"accept": "application/json", "user-agent": "zero-deployment-evidence/1"},
        method="GET",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            raw = response.read().decode("utf-8", errors="replace")
            return {
                "path": path,
                "status": response.status,
                "headers": {k.lower(): v for k, v in response.headers.items()},
                "payload": parse_json_or_raw(raw),
            }
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        return {
            "path": path,
            "status": exc.code,
            "headers": {k.lower(): v for k, v in exc.headers.items()},
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


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def collect_git_context() -> dict[str, Any]:
    def run_git(args: list[str]) -> str | None:
        try:
            result = subprocess.run(
                ["git", *args],
                check=True,
                capture_output=True,
                text=True,
                timeout=5,
            )
        except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
            return None
        return result.stdout.strip()

    return {
        "commit": run_git(["rev-parse", "HEAD"]),
        "short_commit": run_git(["rev-parse", "--short", "HEAD"]),
        "branch": run_git(["rev-parse", "--abbrev-ref", "HEAD"]),
        "dirty": bool(run_git(["status", "--short"])),
    }


def collect_railway_logs(output_dir: Path, *, lines: int, secrets: tuple[str, ...]) -> dict[str, Any]:
    command = os.environ.get("ZERO_RAILWAY_LOG_COMMAND")
    if command:
        argv = shlex.split(command)
    else:
        argv = ["railway", "logs", "--lines", str(lines)]

    try:
        result = subprocess.run(
            argv,
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        status = {
            "requested": True,
            "captured": False,
            "command": argv,
            "error": str(exc),
        }
        write_json(output_dir / "railway_logs_status.json", status, secrets)
        return status

    output = result.stdout + ("\n" + result.stderr if result.stderr else "")
    write_text(output_dir / "railway_logs.txt", output, secrets)
    status = {
        "requested": True,
        "captured": result.returncode == 0,
        "command": argv,
        "exit_code": result.returncode,
        "file": "railway_logs.txt",
    }
    write_json(output_dir / "railway_logs_status.json", status, secrets)
    return status


def build_file_inventory(output_dir: Path) -> list[dict[str, Any]]:
    files: list[dict[str, Any]] = []
    for path in sorted(output_dir.iterdir()):
        if not path.is_file() or path.name in {"SHA256SUMS", "manifest.json", "EVIDENCE_SIGNATURE.json"}:
            continue
        files.append(
            {
                "path": path.name,
                "bytes": path.stat().st_size,
                "sha256": sha256(path),
            }
        )
    return files


def write_sha256s(output_dir: Path) -> None:
    lines = []
    for path in sorted(output_dir.iterdir()):
        if path.is_file() and path.name not in {"SHA256SUMS", "EVIDENCE_SIGNATURE.json"}:
            lines.append(f"{sha256(path)}  {path.name}")
    (output_dir / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def read_sha256s(path: Path) -> dict[str, str]:
    entries: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        if not raw.strip():
            continue
        digest, _, name = raw.partition("  ")
        entries[name] = digest
    return entries


def evidence_signature_payload(output_dir: Path, *, signing_key: str, signer: str) -> dict[str, Any]:
    sha_path = output_dir / "SHA256SUMS"
    files = read_sha256s(sha_path)
    signed_payload = {
        "schema_version": "zero.deployment_evidence_signature_payload.v1",
        "manifest_sha256": sha256(output_dir / "manifest.json"),
        "sha256s_sha256": sha256(sha_path),
        "files": files,
    }
    signed_payload_hash = "sha256:" + hashlib.sha256(
        json.dumps(signed_payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    signature = hmac.new(
        signing_key.encode("utf-8"),
        signed_payload_hash.encode("utf-8"),
        hashlib.sha256,
    ).hexdigest()
    return {
        "schema_version": SIGNATURE_SCHEMA_VERSION,
        "algorithm": "hmac-sha256",
        "signer": signer,
        "signed_payload_hash": signed_payload_hash,
        "signature": f"v1={signature}",
        "key_material_included": False,
        "covers": {
            "manifest": "manifest.json",
            "checksums": "SHA256SUMS",
            "file_count": len(files),
        },
        "signed_payload": signed_payload,
    }


def main() -> int:
    args = parse_args()
    try:
        base_url = normalize_base_url(args.url)
    except ValueError as exc:
        print(f"zero deployment evidence: {exc}", file=sys.stderr)
        return 2

    output_dir = args.output or default_output_dir()
    output_dir.mkdir(parents=True, exist_ok=True)
    secrets = tuple(
        secret
        for secret in (
            args.token,
            args.signing_key,
            os.environ.get("ZERO_INTELLIGENCE_WEBHOOK_SIGNING_KEY", ""),
        )
        if secret
    )

    generated_at = datetime.now(timezone.utc).isoformat()
    checks = run_checks(
        base_url,
        token=args.token,
        timeout=args.timeout,
        expect_paper=args.expect_paper,
    )
    doctor = build_report(base_url, checks)
    write_json(output_dir / "doctor.json", doctor, secrets)

    packet_results: list[dict[str, Any]] = []
    for name, path in PACKETS:
        packet = fetch_packet(base_url, path, timeout=args.timeout)
        packet_results.append({"name": name, "file": f"{name}.json", "status": packet["status"]})
        write_json(output_dir / f"{name}.json", packet, secrets)

    railway_logs = {"requested": False, "captured": False}
    if args.railway_logs:
        railway_logs = collect_railway_logs(
            output_dir,
            lines=args.railway_log_lines,
            secrets=secrets,
        )

    manifest = {
        "schema_version": SCHEMA_VERSION,
        "target": base_url,
        "generated_at": generated_at,
        "collector": {
            "name": "scripts/deployment_evidence.py",
            "paper_expected": args.expect_paper,
            "redaction_applied": True,
            "redacted_classes": [
                "authorization headers",
                "API keys",
                "private keys",
                "signing keys",
                "known supplied tokens",
                "trace IDs",
                "smoke idempotency keys",
            ],
        },
        "git": collect_git_context(),
        "doctor": {
            "file": "doctor.json",
            "summary": doctor["summary"],
        },
        "packets": packet_results,
        "railway_logs": railway_logs,
    }
    write_json(output_dir / "manifest.json", manifest, secrets)

    inventory = build_file_inventory(output_dir)
    manifest["files"] = inventory
    write_json(output_dir / "manifest.json", manifest, secrets)
    write_sha256s(output_dir)
    if args.signing_key:
        write_json(
            output_dir / "EVIDENCE_SIGNATURE.json",
            evidence_signature_payload(output_dir, signing_key=args.signing_key, signer=args.signer),
            secrets,
        )

    summary = doctor["summary"]
    print(
        f"zero deployment evidence: wrote {output_dir} "
        f"({summary['ok']} ok, {summary['warn']} warn, {summary['fail']} fail)"
    )
    if summary["fail"] and not args.allow_failures:
        return 1
    if summary["warn"] and args.fail_on_warn:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
