#!/usr/bin/env python3
"""Validate or create launch-seed GitHub issues from docs/launch-issues.md."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
ISSUES_PATH = ROOT / "docs/launch-issues.md"
LABELS_PATH = ROOT / ".github/labels.yml"
DEFAULT_REPO = "zero-intel/zero"
SEED_TITLE_PREFIXES = (
    "Good First Issue:",
    "Help Wanted:",
    "Maintainer Task:",
)


class LaunchIssueError(ValueError):
    pass


@dataclass(frozen=True)
class LaunchIssue:
    title: str
    labels: tuple[str, ...]
    body: str


def defined_labels() -> set[str]:
    text = LABELS_PATH.read_text(encoding="utf-8")
    return set(re.findall(r"^- name: (.+)$", text, flags=re.MULTILINE))


def parse_launch_issues(path: Path = ISSUES_PATH) -> list[LaunchIssue]:
    text = path.read_text(encoding="utf-8")
    matches = list(re.finditer(r"^## (?P<title>.+)$", text, flags=re.MULTILINE))
    issues: list[LaunchIssue] = []
    errors: list[str] = []

    for index, match in enumerate(matches):
        title = match.group("title").strip()
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        section = text[start:end].strip()
        is_seed = title.startswith(SEED_TITLE_PREFIXES)
        if not is_seed:
            continue

        if not section:
            errors.append(f"{title}: empty issue section")
            continue

        labels_match = re.search(r"^Labels:\s*(?P<labels>.+)$", section, flags=re.MULTILINE)
        if labels_match is None:
            errors.append(f"{title}: missing Labels line")
            continue

        raw_labels = labels_match.group("labels")
        labels = tuple(
            label.strip()
            for label in re.sub(r"`", "", raw_labels).split(",")
            if label.strip()
        )
        if not labels:
            errors.append(f"{title}: Labels line is empty")

        if "Acceptance:" not in section:
            errors.append(f"{title}: missing Acceptance section")

        body_without_labels = (
            section[: labels_match.start()] + section[labels_match.end() :]
        ).strip()
        body = "\n\n".join(
            [
                body_without_labels,
                "---",
                f"Seeded from `{path.relative_to(ROOT)}`. Keep scope small and paper-safe.",
            ]
        )
        issues.append(LaunchIssue(title=title, labels=labels, body=body))

    validate_launch_issues(issues, errors)
    return issues


def validate_launch_issues(issues: list[LaunchIssue], errors: list[str] | None = None) -> None:
    errors = list(errors or [])
    labels = defined_labels()
    seen_titles: set[str] = set()

    for issue in issues:
        if issue.title in seen_titles:
            errors.append(f"duplicate issue title: {issue.title}")
        seen_titles.add(issue.title)

        for label in issue.labels:
            if label not in labels:
                errors.append(f"{issue.title}: undefined label: {label}")

        if len(issue.body) > 65_536:
            errors.append(f"{issue.title}: body exceeds GitHub issue body limit")

    if errors:
        raise LaunchIssueError("\n".join(errors))


def run_gh(args: list[str], *, input_text: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["gh", *args],
        cwd=ROOT,
        input=input_text,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def fetch_remote_issues(repo: str) -> dict[str, dict[str, object]]:
    result = run_gh(
        [
            "issue",
            "list",
            "--repo",
            repo,
            "--state",
            "all",
            "--limit",
            "1000",
            "--json",
            "number,title,state,labels,url",
        ]
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "failed to fetch GitHub issues")

    issues = json.loads(result.stdout)
    return {issue["title"]: issue for issue in issues}


def print_plan(issues: list[LaunchIssue], existing: dict[str, dict[str, object]]) -> None:
    missing = [issue for issue in issues if issue.title not in existing]
    present = [issue for issue in issues if issue.title in existing]

    for issue in missing:
        print(f"create: {issue.title} labels={','.join(issue.labels)}")
    for issue in present:
        remote = existing[issue.title]
        print(f"exists: #{remote['number']} {issue.title} state={remote['state']}")

    print(f"launch issue plan: {len(missing)} create, {len(present)} already present")


def create_missing_issues(
    repo: str,
    issues: list[LaunchIssue],
    existing: dict[str, dict[str, object]],
    *,
    dry_run: bool,
) -> None:
    for issue in issues:
        if issue.title in existing:
            continue

        args = [
            "issue",
            "create",
            "--repo",
            repo,
            "--title",
            issue.title,
            "--body-file",
            "-",
        ]
        for label in issue.labels:
            args.extend(["--label", label])

        if dry_run:
            print("dry-run:", "gh", " ".join(args))
            continue

        result = run_gh(args, input_text=issue.body)
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or f"failed creating issue: {issue.title}")
        print(result.stdout.strip())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=os.environ.get("ZERO_GITHUB_REPO", DEFAULT_REPO))
    parser.add_argument("--validate-config", action="store_true", help="validate local seed issues only")
    parser.add_argument("--check", action="store_true", help="fail if remote seed issues are missing")
    parser.add_argument("--apply", action="store_true", help="create missing remote seed issues")
    parser.add_argument("--dry-run", action="store_true", help="print gh mutations without applying them")
    args = parser.parse_args()

    try:
        issues = parse_launch_issues()

        if args.validate_config:
            print(f"launch issue config valid: {len(issues)} seed issues")
            return 0

        if not args.check and not args.apply:
            parser.error("choose --validate-config, --check, or --apply")

        existing = fetch_remote_issues(args.repo)
        print_plan(issues, existing)
        missing = [issue for issue in issues if issue.title not in existing]

        if args.check and missing:
            return 1

        if args.apply:
            create_missing_issues(args.repo, issues, existing, dry_run=args.dry_run)
            if args.dry_run:
                print("launch issue sync dry-run complete")
            else:
                print(f"launch issue sync applied: {len(missing)} created")
        return 0
    except (LaunchIssueError, RuntimeError, json.JSONDecodeError) as exc:
        print(exc, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
