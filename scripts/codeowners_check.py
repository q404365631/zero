#!/usr/bin/env python3
"""Validate CODEOWNERS coverage for safety-critical public repo surfaces."""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
CODEOWNERS = ROOT / ".github" / "CODEOWNERS"

REQUIRED_PATTERNS = [
    "*",
    "/.github/",
    "/Formula/zero.rb",
    "/scripts/assemble_release_assets.sh",
    "/scripts/draft_release_rehearsal.sh",
    "/scripts/homebrew_formula.py",
    "/scripts/homebrew_formula_check.py",
    "/scripts/install.sh",
    "/scripts/package_dry_run.sh",
    "/scripts/release_evidence.py",
    "/scripts/release_provenance.py",
    "/scripts/release_rehearsal.sh",
    "/scripts/release_verify.py",
    "/engine/src/zero_engine/api.py",
    "/engine/src/zero_engine/hyperliquid.py",
    "/engine/src/zero_engine/immune.py",
    "/engine/src/zero_engine/live.py",
    "/engine/src/zero_engine/live_certification.py",
    "/engine/src/zero_engine/reconciliation.py",
    "/engine/src/zero_engine/safety.py",
    "/cli/crates/zero-commands/",
    "/cli/crates/zero-config/",
    "/cli/crates/zero-doctor/",
    "/cli/crates/zero-engine-client/",
    "/cli/crates/zero-headless/",
    "/cli/crates/zero-tui/",
    "/contracts/",
    "/openapi/",
    "/docs/live-certification.md",
    "/docs/live-cockpit.md",
    "/docs/live-evidence.md",
    "/docs/safety-model.md",
    "/docs/threat-model.md",
    "/docs/incident-runbooks.md",
    "/docs/release.md",
    "/docs/distribution.md",
]

OWNER_RE = re.compile(
    r"^(?:@[A-Za-z0-9-]+(?:/[A-Za-z0-9-]+)?|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})$"
)
PLACEHOLDER_RE = re.compile(r"(TODO|TBD|OWNER|REPLACE|example)", re.IGNORECASE)


def parse_codeowners() -> tuple[dict[str, list[str]], list[str]]:
    entries: dict[str, list[str]] = {}
    errors: list[str] = []

    if not CODEOWNERS.is_file():
        return entries, ["missing .github/CODEOWNERS"]

    for number, raw in enumerate(CODEOWNERS.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue

        if PLACEHOLDER_RE.search(line):
            errors.append(f".github/CODEOWNERS:{number}: placeholder text is forbidden")

        parts = line.split()
        if len(parts) < 2:
            errors.append(f".github/CODEOWNERS:{number}: entry must include pattern and owner")
            continue

        pattern, owners = parts[0], parts[1:]
        if not pattern.startswith(("/", "*")):
            errors.append(f".github/CODEOWNERS:{number}: pattern must be rooted or wildcard: {pattern}")

        for owner in owners:
            if not OWNER_RE.match(owner):
                errors.append(f".github/CODEOWNERS:{number}: invalid owner syntax: {owner}")

        entries[pattern] = owners

    return entries, errors


def main() -> int:
    entries, errors = parse_codeowners()

    for pattern in REQUIRED_PATTERNS:
        if pattern not in entries:
            errors.append(f".github/CODEOWNERS missing required pattern: {pattern}")

    for pattern in sorted(entries):
        owners = entries[pattern]
        if not owners:
            errors.append(f".github/CODEOWNERS pattern has no owner: {pattern}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print(
        "codeowners check passed: "
        f"{len(entries)} entries, {len(REQUIRED_PATTERNS)} required patterns"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
