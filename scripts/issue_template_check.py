#!/usr/bin/env python3
"""Check GitHub issue templates and labels for public contribution lanes."""

from __future__ import annotations

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]

REQUIRED_FILES = [
    ".github/ISSUE_TEMPLATE/agent_task.yml",
    ".github/ISSUE_TEMPLATE/bug_report.yml",
    ".github/ISSUE_TEMPLATE/design_review.yml",
    ".github/ISSUE_TEMPLATE/docs_gap.yml",
    ".github/ISSUE_TEMPLATE/feature_request.yml",
    ".github/ISSUE_TEMPLATE/safety_review.yml",
    ".github/ISSUE_TEMPLATE/strategy_example.yml",
    ".github/ISSUE_TEMPLATE/config.yml",
    ".github/labels.yml",
]

REQUIRED_TEMPLATE_MARKERS = {
    ".github/ISSUE_TEMPLATE/agent_task.yml": [
        'labels: ["agent-eligible", "needs triage"]',
        "Owner Boundary",
        "Out Of Scope",
        "Safety Invariant",
        "Acceptance Criteria",
        "Required Checks",
        "paper-only",
    ],
    ".github/ISSUE_TEMPLATE/safety_review.yml": [
        'labels: ["safety-critical", "safety", "needs triage"]',
        "Failure Modes",
        "Fail-Closed Behavior",
        "Paper-Mode Validation",
        "just hardening-gate",
    ],
    ".github/ISSUE_TEMPLATE/strategy_example.yml": [
        'labels: ["good-first-strategy", "strategy", "examples", "needs triage"]',
        "paper-only",
        "deterministic",
        "just strategy-runner-example",
    ],
    ".github/ISSUE_TEMPLATE/design_review.yml": [
        'labels: ["design-review", "design", "needs triage"]',
        "Safety Copy Invariant",
        "guaranteed returns",
        "just network-pages-smoke",
    ],
    ".github/ISSUE_TEMPLATE/docs_gap.yml": [
        'labels: ["docs-gap", "docs", "needs triage"]',
        "coding agent",
        "Source Of Truth",
        "docs/llms.txt",
    ],
    ".github/ISSUE_TEMPLATE/bug_report.yml": [
        "Safety Impact",
        "safety-critical bug",
        "security-sensitive bug",
    ],
    ".github/ISSUE_TEMPLATE/feature_request.yml": [
        "Open-Core Boundary",
        "Safety Invariant",
        "contracts",
        "design",
    ],
    ".github/ISSUE_TEMPLATE/config.yml": [
        "blank_issues_enabled: false",
        "Agentic contribution guide",
        "docs/agentic-contribution.md",
    ],
}

REQUIRED_LABELS = [
    "agent-eligible",
    "safety-critical",
    "good-first-strategy",
    "design-review",
    "docs-gap",
    "strategy",
    "proof-pack",
    "mcp",
]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def main() -> int:
    errors: list[str] = []

    for path in REQUIRED_FILES:
        if not (ROOT / path).is_file():
            errors.append(f"missing required issue/label file: {path}")

    for path, markers in REQUIRED_TEMPLATE_MARKERS.items():
        if not (ROOT / path).is_file():
            continue
        text = read(path)
        for marker in markers:
            if marker not in text:
                errors.append(f"{path} missing marker: {marker}")

    labels_text = read(".github/labels.yml") if (ROOT / ".github/labels.yml").is_file() else ""
    for label in REQUIRED_LABELS:
        if f"name: {label}" not in labels_text:
            errors.append(f".github/labels.yml missing label: {label}")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print(
        "issue template check passed: "
        f"{len(REQUIRED_FILES) - 1} templates, {len(REQUIRED_LABELS)} required labels"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
