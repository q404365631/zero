#!/usr/bin/env python3
"""Verify ZERO release assets and checksum coverage."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.release_verify.v1"


@dataclass(frozen=True)
class Finding:
    name: str
    status: str
    message: str
    evidence: dict[str, Any]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Verify a ZERO GitHub Release asset directory. Checks expected "
            "artifacts, SHA256SUMS coverage, checksum integrity, and common "
            "paper-runtime release mistakes."
        )
    )
    parser.add_argument("release_dir", type=Path, help="Directory containing release assets")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable JSON")
    parser.add_argument(
        "--source-only",
        action="store_true",
        help="Allow source/Python-only release assets without CLI binaries or paper image.",
    )
    return parser.parse_args()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_sha256s(path: Path) -> tuple[dict[str, str], list[str]]:
    entries: dict[str, str] = {}
    errors: list[str] = []
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw_line.strip()
        if not line:
            continue
        match = re.fullmatch(r"([a-fA-F0-9]{64})\s{2}(.+)", line)
        if not match:
            errors.append(f"line {line_number}: malformed checksum entry")
            continue
        digest, name = match.groups()
        if "/" in name or "\\" in name or name in {".", ".."}:
            errors.append(f"line {line_number}: unsafe artifact name {name!r}")
            continue
        if name in entries:
            errors.append(f"line {line_number}: duplicate artifact name {name!r}")
            continue
        entries[name] = digest.lower()
    return entries, errors


def ok(name: str, message: str, **evidence: Any) -> Finding:
    return Finding(name=name, status="ok", message=message, evidence=evidence)


def fail(name: str, message: str, **evidence: Any) -> Finding:
    return Finding(name=name, status="fail", message=message, evidence=evidence)


def verify(release_dir: Path, *, source_only: bool) -> dict[str, Any]:
    findings: list[Finding] = []
    release_dir = release_dir.resolve()

    if not release_dir.is_dir():
        findings.append(fail("release_dir", "release directory does not exist", path=str(release_dir)))
        return build_report(release_dir, findings, {})

    checksum_file = release_dir / "SHA256SUMS"
    files = sorted(path for path in release_dir.iterdir() if path.is_file())
    names = [path.name for path in files]
    if checksum_file.is_file():
        findings.append(ok("checksum_manifest", "SHA256SUMS exists", path="SHA256SUMS"))
    else:
        findings.append(fail("checksum_manifest", "SHA256SUMS is missing"))
        return build_report(release_dir, findings, {"files": names})

    entries, parse_errors = parse_sha256s(checksum_file)
    if parse_errors:
        findings.append(fail("checksum_parse", "SHA256SUMS has malformed entries", errors=parse_errors))
    else:
        findings.append(ok("checksum_parse", "SHA256SUMS format is valid", entries=len(entries)))

    assets = sorted(name for name in names if name != "SHA256SUMS")
    missing_coverage = sorted(set(assets) - set(entries))
    stale_coverage = sorted(set(entries) - set(assets))
    if missing_coverage or stale_coverage:
        findings.append(
            fail(
                "checksum_coverage",
                "SHA256SUMS must cover exactly the release assets",
                missing=missing_coverage,
                stale=stale_coverage,
            )
        )
    else:
        findings.append(ok("checksum_coverage", "SHA256SUMS covers every release asset", assets=len(assets)))

    mismatches: list[dict[str, str]] = []
    for name, expected in entries.items():
        path = release_dir / name
        if path.is_file():
            actual = sha256(path)
            if actual != expected:
                mismatches.append({"file": name, "expected": expected, "actual": actual})
    if mismatches:
        findings.append(fail("checksum_integrity", "one or more assets do not match SHA256SUMS", mismatches=mismatches))
    else:
        findings.append(ok("checksum_integrity", "all checksummed assets match"))

    wheel_count = len([name for name in assets if name.endswith(".whl")])
    sdist_count = len([name for name in assets if name.endswith(".tar.gz")])
    required = ["zero-linux", "zero-macos", "zero-paper-image.tar"]
    if source_only:
        required = []
    required.extend(["SBOM.spdx.json", "PROVENANCE.json"])
    missing_required = [name for name in required if name not in assets]
    if wheel_count == 0:
        missing_required.append("*.whl")
    if sdist_count == 0:
        missing_required.append("*.tar.gz")
    if missing_required:
        findings.append(
            fail(
                "expected_assets",
                "release directory is missing expected launch assets",
                missing=missing_required,
                source_only=source_only,
            )
        )
    else:
        findings.append(
            ok(
                "expected_assets",
                "expected release assets are present",
                wheel_count=wheel_count,
                sdist_count=sdist_count,
                source_only=source_only,
            )
        )

    executable_sizes = {
        name: (release_dir / name).stat().st_size
        for name in ("zero-linux", "zero-macos")
        if (release_dir / name).is_file()
    }
    empty_assets = [name for name in assets if (release_dir / name).stat().st_size == 0]
    if empty_assets:
        findings.append(fail("nonempty_assets", "release assets must not be empty", empty=empty_assets))
    else:
        findings.append(ok("nonempty_assets", "release assets are nonempty", executable_sizes=executable_sizes))

    metadata_findings = verify_metadata_files(release_dir)
    findings.extend(metadata_findings)

    metadata = {
        "files": names,
        "assets": assets,
        "asset_count": len(assets),
        "checksum_entries": len(entries),
        "source_only": source_only,
    }
    return build_report(release_dir, findings, metadata)


def load_json(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return None, f"{exc.msg} at line {exc.lineno} column {exc.colno}"
    if not isinstance(data, dict):
        return None, "top-level JSON value must be an object"
    return data, None


def verify_metadata_files(release_dir: Path) -> list[Finding]:
    findings: list[Finding] = []
    sbom_path = release_dir / "SBOM.spdx.json"
    provenance_path = release_dir / "PROVENANCE.json"

    if sbom_path.is_file():
        sbom, error = load_json(sbom_path)
        if error:
            findings.append(fail("sbom_metadata", "SBOM.spdx.json is invalid JSON", error=error))
        elif sbom.get("spdxVersion") != "SPDX-2.3" or not sbom.get("packages"):
            findings.append(
                fail(
                    "sbom_metadata",
                    "SBOM.spdx.json must be SPDX 2.3 and include packages",
                    spdx_version=sbom.get("spdxVersion"),
                    package_count=len(sbom.get("packages", [])) if isinstance(sbom.get("packages"), list) else 0,
                )
            )
        else:
            findings.append(
                ok(
                    "sbom_metadata",
                    "SBOM.spdx.json is parseable SPDX metadata",
                    packages=len(sbom.get("packages", [])),
                )
            )

    if provenance_path.is_file():
        provenance, error = load_json(provenance_path)
        if error:
            findings.append(fail("provenance_metadata", "PROVENANCE.json is invalid JSON", error=error))
        elif provenance.get("schema_version") != "zero.release_provenance.v1":
            findings.append(
                fail(
                    "provenance_metadata",
                    "PROVENANCE.json has an unexpected schema version",
                    schema_version=provenance.get("schema_version"),
                )
            )
        elif provenance.get("policy", {}).get("live_execution_claimed") is not False:
            findings.append(
                fail(
                    "provenance_metadata",
                    "PROVENANCE.json must not claim live execution evidence",
                    policy=provenance.get("policy", {}),
                )
            )
        else:
            findings.append(
                ok(
                    "provenance_metadata",
                    "PROVENANCE.json is parseable and policy-safe",
                    assets=len(provenance.get("release", {}).get("assets", [])),
                )
            )

    return findings


def build_report(release_dir: Path, findings: list[Finding], metadata: dict[str, Any]) -> dict[str, Any]:
    fail_count = sum(1 for finding in findings if finding.status == "fail")
    ok_count = sum(1 for finding in findings if finding.status == "ok")
    return {
        "schema_version": SCHEMA_VERSION,
        "release_dir": str(release_dir),
        "summary": {
            "status": "fail" if fail_count else "ok",
            "ok": ok_count,
            "fail": fail_count,
        },
        "metadata": metadata,
        "findings": [
            {
                "name": finding.name,
                "status": finding.status,
                "message": finding.message,
                "evidence": finding.evidence,
            }
            for finding in findings
        ],
    }


def emit_text(report: dict[str, Any]) -> None:
    summary = report["summary"]
    print(f"zero release verify: {summary['status']} ({summary['ok']} ok, {summary['fail']} fail)")
    print(f"release_dir: {report['release_dir']}")
    for finding in report["findings"]:
        print(f"{finding['status']:>4} {finding['name']}: {finding['message']}")


def main() -> int:
    args = parse_args()
    report = verify(args.release_dir, source_only=args.source_only)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        emit_text(report)
    return 1 if report["summary"]["fail"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
