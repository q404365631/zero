#!/usr/bin/env python3
"""Prove live cockpit drill verification rejects tampered bundles."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.live_cockpit_drill_tamper_rehearsal.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run the live cockpit drill verifier against a captured bundle, "
            "then prove checksum and semantic packet tampering are both rejected."
        )
    )
    parser.add_argument("bundle", type=Path, help="Bundle directory from live_cockpit_drill.py.")
    parser.add_argument(
        "--forbid-token",
        action="append",
        default=[],
        help="Additional raw token forwarded to the verifier.",
    )
    parser.add_argument("--json", action="store_true", help="Emit a machine-readable report.")
    return parser.parse_args()


def verifier_command(bundle: Path, forbid_tokens: list[str]) -> list[str]:
    script = Path(__file__).resolve().with_name("live_cockpit_drill_verify.py")
    command = [sys.executable, str(script), str(bundle)]
    for token in forbid_tokens:
        command.extend(["--forbid-token", token])
    return command


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=False, text=True, capture_output=True)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_sha256s(bundle: Path) -> None:
    lines = []
    for path in sorted(bundle.iterdir()):
        if path.is_file() and path.name != "SHA256SUMS":
            lines.append(f"{sha256(path)}  {path.name}")
    (bundle / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def copy_bundle(source: Path, destination: Path) -> Path:
    target = destination / source.name
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, target)
    return target


def checksum_tamper(source: Path, work_dir: Path) -> Path:
    bundle = copy_bundle(source, work_dir / "checksum")
    packet = bundle / "04_live_cockpit.json"
    packet.write_text(packet.read_text(encoding="utf-8") + "\n", encoding="utf-8")
    return bundle


def semantic_tamper(source: Path, work_dir: Path) -> Path:
    bundle = copy_bundle(source, work_dir / "semantic")
    packet_path = bundle / "04_live_cockpit.json"
    packet = json.loads(packet_path.read_text(encoding="utf-8"))
    payload = packet.get("payload")
    if not isinstance(payload, dict):
        raise ValueError("04_live_cockpit.json payload is not an object")
    payload["ready"] = True
    payload["live_mode"] = "ready"
    payload["risk_increasing_allowed"] = True
    packet_path.write_text(json.dumps(packet, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    write_sha256s(bundle)
    return bundle


def result(name: str, completed: subprocess.CompletedProcess[str], *, expected_success: bool) -> dict[str, Any]:
    ok = completed.returncode == 0 if expected_success else completed.returncode != 0
    return {
        "name": name,
        "ok": ok,
        "expected_success": expected_success,
        "status": completed.returncode,
        "stdout": completed.stdout.strip(),
        "stderr": completed.stderr.strip(),
    }


def render_text(report: dict[str, Any]) -> str:
    header = (
        f"zero live cockpit drill tamper rehearsal: ok={report['ok']} "
        f"checks={report['summary']['ok']} fail={report['summary']['fail']}"
    )
    failures = [
        f"- {check['name']}: status={check['status']} stdout={check['stdout']}"
        for check in report["checks"]
        if not check["ok"]
    ]
    return "\n".join([header, *failures]) if failures else header


def main() -> int:
    args = parse_args()
    bundle = args.bundle
    if not bundle.is_dir():
        print(f"zero live cockpit drill tamper rehearsal: missing bundle {bundle}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="zero-cockpit-tamper-") as tmp:
        work_dir = Path(tmp)
        clean = run(verifier_command(bundle, args.forbid_token))
        checksum = run(verifier_command(checksum_tamper(bundle, work_dir), args.forbid_token))
        semantic = run(verifier_command(semantic_tamper(bundle, work_dir), args.forbid_token))

    checks = [
        result("original_bundle_verifies", clean, expected_success=True),
        result("checksum_tamper_rejected", checksum, expected_success=False),
        result("semantic_tamper_rejected", semantic, expected_success=False),
    ]
    fail = len([check for check in checks if not check["ok"]])
    report = {
        "schema_version": SCHEMA_VERSION,
        "bundle": str(bundle),
        "ok": fail == 0,
        "summary": {
            "ok": len([check for check in checks if check["ok"]]),
            "fail": fail,
        },
        "checks": checks,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(render_text(report))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
