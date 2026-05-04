#!/usr/bin/env python3
"""Check synthetic negative proof fixtures fail public-safety verification."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
ENGINE_SRC = ROOT / "engine" / "src"
if str(ENGINE_SRC) not in sys.path:
    sys.path.insert(0, str(ENGINE_SRC))
if str(Path(__file__).resolve().parent) not in sys.path:
    sys.path.insert(0, str(Path(__file__).resolve().parent))

from network_profile_verify import verify_profile  # noqa: E402

FIXTURE_DIR = ROOT / "docs" / "proof" / "privacy-regression"


def load_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return payload


def check_fixture(path: Path) -> list[str]:
    fixture = load_json(path)
    payload = fixture.get("payload")
    expected_failures = fixture.get("expected_failures", [])
    if fixture.get("synthetic") is not True:
        return [f"{path}: fixture must declare synthetic=true"]
    if not isinstance(payload, dict):
        return [f"{path}: missing payload object"]
    if not isinstance(expected_failures, list) or not expected_failures:
        return [f"{path}: expected_failures must be a non-empty list"]

    findings = verify_profile(payload, require_consent=False, forbid_tokens=[])
    failures = [finding for finding in findings if finding["status"] == "fail"]
    errors: list[str] = []
    if not failures:
        errors.append(f"{path}: verifier unexpectedly accepted negative fixture")

    for expected in expected_failures:
        if not isinstance(expected, dict):
            errors.append(f"{path}: expected failure entries must be objects")
            continue
        name = str(expected.get("name", ""))
        contains = str(expected.get("contains", ""))
        matched = [
            finding
            for finding in failures
            if finding.get("name") == name and contains in str(finding.get("detail", ""))
        ]
        if not matched:
            rendered = ", ".join(
                f"{finding.get('name')}={finding.get('detail')}" for finding in failures
            )
            errors.append(f"{path}: expected {name!r} containing {contains!r}; got {rendered}")
    return errors


def fixture_paths(fixture_dir: Path) -> list[Path]:
    return sorted(path for path in fixture_dir.glob("*.json") if path.is_file())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--fixture-dir",
        type=Path,
        default=FIXTURE_DIR,
        help="Directory containing synthetic negative proof privacy fixtures.",
    )
    args = parser.parse_args()

    paths = fixture_paths(args.fixture_dir)
    if not paths:
        print(f"no proof privacy regression fixtures found in {args.fixture_dir}", file=sys.stderr)
        return 1

    errors: list[str] = []
    for path in paths:
        errors.extend(check_fixture(path))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"proof privacy regression: {len(paths)} negative fixtures refused")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
