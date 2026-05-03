#!/usr/bin/env python3
"""Validate or sync GitHub labels from .github/labels.yml."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
LABELS_PATH = ROOT / ".github/labels.yml"
DEFAULT_REPO = "zero-intel/zero"


class LabelConfigError(ValueError):
    pass


def unquote(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        return value[1:-1]
    return value


def parse_labels(path: Path = LABELS_PATH) -> list[dict[str, str]]:
    labels: list[dict[str, str]] = []
    current: dict[str, str] | None = None

    for lineno, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw_line.rstrip()
        if not line:
            continue

        if line.startswith("- name: "):
            if current is not None:
                labels.append(current)
            current = {"name": unquote(line.removeprefix("- name: "))}
            continue

        match = re.match(r"^\s+([a-z_]+):\s*(.+)$", line)
        if match and current is not None:
            current[match.group(1)] = unquote(match.group(2))
            continue

        raise LabelConfigError(f"{path.relative_to(ROOT)}:{lineno}: unsupported label syntax")

    if current is not None:
        labels.append(current)

    validate_labels(labels)
    return labels


def validate_labels(labels: list[dict[str, str]]) -> None:
    seen: set[str] = set()
    errors: list[str] = []

    if not labels:
        errors.append(".github/labels.yml contains no labels")

    for index, label in enumerate(labels, 1):
        name = label.get("name", "").strip()
        color = label.get("color", "").strip().removeprefix("#")
        description = label.get("description", "").strip()

        if not name:
            errors.append(f"label #{index} is missing name")
        elif name in seen:
            errors.append(f"duplicate label name: {name}")
        else:
            seen.add(name)

        if not re.fullmatch(r"[0-9a-fA-F]{6}", color):
            errors.append(f"{name or f'label #{index}'} has invalid color: {color!r}")

        if not description:
            errors.append(f"{name or f'label #{index}'} is missing description")
        elif len(description) > 100:
            errors.append(f"{name} description exceeds GitHub's 100 character limit")

    if errors:
        raise LabelConfigError("\n".join(errors))


def run_gh(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["gh", *args],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def fetch_remote_labels(repo: str) -> dict[str, dict[str, str]]:
    result = run_gh(
        [
            "label",
            "list",
            "--repo",
            repo,
            "--limit",
            "1000",
            "--json",
            "name,color,description",
        ]
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "failed to fetch labels with gh")

    remote = json.loads(result.stdout)
    return {
        item["name"]: {
            "name": item["name"],
            "color": item.get("color", "").removeprefix("#").lower(),
            "description": item.get("description") or "",
        }
        for item in remote
    }


def desired_by_name(labels: list[dict[str, str]]) -> dict[str, dict[str, str]]:
    return {
        label["name"]: {
            "name": label["name"],
            "color": label["color"].removeprefix("#").lower(),
            "description": label["description"],
        }
        for label in labels
    }


def diff_labels(
    desired: dict[str, dict[str, str]],
    remote: dict[str, dict[str, str]],
) -> tuple[list[dict[str, str]], list[tuple[dict[str, str], dict[str, str]]]]:
    missing: list[dict[str, str]] = []
    changed: list[tuple[dict[str, str], dict[str, str]]] = []

    for name, label in desired.items():
        actual = remote.get(name)
        if actual is None:
            missing.append(label)
            continue
        if actual["color"] != label["color"] or actual["description"] != label["description"]:
            changed.append((label, actual))

    return missing, changed


def print_plan(
    missing: list[dict[str, str]],
    changed: list[tuple[dict[str, str], dict[str, str]]],
) -> None:
    if not missing and not changed:
        print("github label sync: remote labels already match .github/labels.yml")
        return

    for label in missing:
        print(f"create: {label['name']} color={label['color']} description={label['description']!r}")
    for desired, actual in changed:
        print(
            "update: "
            f"{desired['name']} "
            f"color {actual['color']} -> {desired['color']} "
            f"description {actual['description']!r} -> {desired['description']!r}"
        )


def apply_plan(
    repo: str,
    missing: list[dict[str, str]],
    changed: list[tuple[dict[str, str], dict[str, str]]],
    *,
    dry_run: bool,
) -> None:
    for label in missing:
        args = [
            "label",
            "create",
            label["name"],
            "--repo",
            repo,
            "--color",
            label["color"],
            "--description",
            label["description"],
        ]
        if dry_run:
            print("dry-run:", "gh", " ".join(args))
            continue
        result = run_gh(args)
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or f"failed creating label {label['name']}")

    for desired, _actual in changed:
        args = [
            "label",
            "edit",
            desired["name"],
            "--repo",
            repo,
            "--color",
            desired["color"],
            "--description",
            desired["description"],
        ]
        if dry_run:
            print("dry-run:", "gh", " ".join(args))
            continue
        result = run_gh(args)
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or f"failed updating label {desired['name']}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=os.environ.get("ZERO_GITHUB_REPO", DEFAULT_REPO))
    parser.add_argument("--validate-config", action="store_true", help="validate local label config only")
    parser.add_argument("--check", action="store_true", help="fail if remote labels drift from config")
    parser.add_argument("--apply", action="store_true", help="create or update remote labels")
    parser.add_argument("--dry-run", action="store_true", help="print gh mutations without applying them")
    args = parser.parse_args()

    try:
        labels = parse_labels()
        desired = desired_by_name(labels)

        if args.validate_config:
            print(f"github label config valid: {len(labels)} labels")
            return 0

        if not args.check and not args.apply:
            parser.error("choose --validate-config, --check, or --apply")

        remote = fetch_remote_labels(args.repo)
        missing, changed = diff_labels(desired, remote)
        print_plan(missing, changed)

        if args.check and (missing or changed):
            return 1

        if args.apply:
            apply_plan(args.repo, missing, changed, dry_run=args.dry_run)
            if args.dry_run:
                print("github label sync dry-run complete")
            else:
                print(
                    "github label sync applied: "
                    f"{len(missing)} created, {len(changed)} updated"
                )
        return 0
    except (LabelConfigError, RuntimeError, json.JSONDecodeError) as exc:
        print(exc, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
