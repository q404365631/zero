#!/usr/bin/env python3
"""Check that public issue seeds only reference labels defined in labels.yml."""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
LABELS_PATH = ROOT / ".github/labels.yml"
DOC_PATHS = [
    ROOT / "docs/backlog.md",
    ROOT / "docs/launch-issues.md",
]
TEMPLATE_DIR = ROOT / ".github/ISSUE_TEMPLATE"

REQUIRED_LABELS = [
    "agent-eligible",
    "bug",
    "ci",
    "cli",
    "containers",
    "contracts",
    "design",
    "design-review",
    "docs",
    "docs-gap",
    "engine",
    "enhancement",
    "examples",
    "good first issue",
    "good-first-strategy",
    "help wanted",
    "market-data",
    "mcp",
    "needs triage",
    "network",
    "packaging",
    "proof-pack",
    "release",
    "safety",
    "safety-critical",
    "security",
    "strategy",
]


def defined_labels() -> set[str]:
    text = LABELS_PATH.read_text(encoding="utf-8")
    return set(re.findall(r"^- name: (.+)$", text, flags=re.MULTILINE))


def referenced_doc_labels(path: Path) -> set[str]:
    labels: set[str] = set()
    text = path.read_text(encoding="utf-8")
    for raw in re.findall(r"^Labels:\s*`([^`]+)`", text, flags=re.MULTILINE):
        labels.update(label.strip() for label in raw.split(",") if label.strip())
    return labels


def referenced_template_labels(path: Path) -> set[str]:
    labels: set[str] = set()
    text = path.read_text(encoding="utf-8")
    for raw in re.findall(r'^labels:\s*\[(.+)\]$', text, flags=re.MULTILINE):
        labels.update(
            label.strip().strip('"').strip("'")
            for label in raw.split(",")
            if label.strip()
        )
    return labels


def main() -> int:
    errors: list[str] = []

    defined = defined_labels()
    for label in REQUIRED_LABELS:
        if label not in defined:
            errors.append(f".github/labels.yml missing required label: {label}")

    for path in DOC_PATHS:
        for label in sorted(referenced_doc_labels(path)):
            if label not in defined:
                errors.append(f"{path.relative_to(ROOT)} references undefined label: {label}")

    for path in sorted(TEMPLATE_DIR.glob("*.yml")):
        for label in sorted(referenced_template_labels(path)):
            if label not in defined:
                errors.append(f"{path.relative_to(ROOT)} references undefined label: {label}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print(
        "label taxonomy check passed: "
        f"{len(defined)} defined labels, {len(REQUIRED_LABELS)} required labels"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
