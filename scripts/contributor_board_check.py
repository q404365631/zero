#!/usr/bin/env python3
"""Validate the source-controlled contributor issue board."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
LAUNCH_ISSUES_PATH = ROOT / "docs/launch-issues.md"
BOARD_PATH = ROOT / "docs/contributor-issue-board.md"
SEED_TITLE_PREFIXES = (
    "Good First Issue:",
    "Help Wanted:",
    "Maintainer Task:",
)


@dataclass(frozen=True)
class SeedIssue:
    number: int
    title: str
    delivered: bool


def sections(text: str) -> dict[str, str]:
    matches = list(re.finditer(r"^## (?P<title>.+)$", text, flags=re.MULTILINE))
    result: dict[str, str] = {}
    for index, match in enumerate(matches):
        title = match.group("title").strip()
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        result[title] = text[start:end].strip()
    return result


def launch_seed_issues() -> list[SeedIssue]:
    text = LAUNCH_ISSUES_PATH.read_text(encoding="utf-8")
    result: list[SeedIssue] = []
    errors: list[str] = []

    for title, body in sections(text).items():
        if not title.startswith(SEED_TITLE_PREFIXES):
            continue

        number_match = re.search(r"https://github\.com/zero-intel/zero/issues/(\d+)", body)
        if number_match is None:
            errors.append(f"{title}: missing GitHub issue link")
            continue

        result.append(
            SeedIssue(
                number=int(number_match.group(1)),
                title=title,
                delivered=bool(re.search(r"^Status:\s*delivered\b", body, flags=re.MULTILINE)),
            )
        )

    if errors:
        raise ValueError("\n".join(errors))
    return result


def issue_numbers(markdown: str) -> set[int]:
    return {int(number) for number in re.findall(r"github\.com/zero-intel/zero/issues/(\d+)", markdown)}


def main() -> int:
    errors: list[str] = []

    try:
        seeds = launch_seed_issues()
    except ValueError as exc:
        print(exc, file=sys.stderr)
        return 1

    board = BOARD_PATH.read_text(encoding="utf-8")
    board_sections = sections(board)
    completed = issue_numbers(board_sections.get("Completed Seed Issues", ""))
    open_sections = {
        "Good First Issues": issue_numbers(board_sections.get("Good First Issues", "")),
        "Help Wanted": issue_numbers(board_sections.get("Help Wanted", "")),
    }
    open_numbers = set().union(*open_sections.values())

    for issue in seeds:
        if issue.delivered:
            if issue.number not in completed:
                errors.append(f"delivered issue #{issue.number} missing from Completed Seed Issues")
            for name, numbers in open_sections.items():
                if issue.number in numbers:
                    errors.append(f"delivered issue #{issue.number} still listed under {name}")
        else:
            if issue.number not in open_numbers:
                errors.append(f"open seed issue #{issue.number} missing from open board sections")
            if issue.number in completed:
                errors.append(f"open seed issue #{issue.number} incorrectly listed as completed")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    delivered = sum(1 for issue in seeds if issue.delivered)
    open_count = len(seeds) - delivered
    print(
        "contributor board check passed: "
        f"{delivered} delivered seed issues, {open_count} open seed issues"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
