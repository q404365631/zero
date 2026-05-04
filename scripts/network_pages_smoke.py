#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlparse


PRIVATE_PATTERNS = [
    re.compile(pattern, re.IGNORECASE)
    for pattern in [
        r"\btrace_id\b",
        r"\bidempotency_key\b",
        r"\bwallet_address\b",
        r"\bprivate_key\b",
        r"\bexchange_response\b",
        r"api:/execute",
        r"strategy:",
        r"\bnetwork-fill\b",
        r"\btrace-network\b",
        r"\bBTC\b",
        r"\bETH\b",
        r"\bSOL\b",
        r"0x1{16,}",
    ]
]


EXPECTED = {
    "index.html": {
        "title": "ZERO Network",
        "h1": "Public Proof Surface",
        "links": {"profile.html", "empty-profile.html", "stale-profile.html", "leaderboard.html"},
        "must_contain": {"Empty", "Active", "Stale"},
    },
    "profile.html": {
        "title": "ZERO Local \u00b7 ZERO Network",
        "h1": "ZERO Local",
        "links": set(),
        "must_contain": {"Active aggregate proof"},
    },
    "empty-profile.html": {
        "title": "ZERO Empty \u00b7 ZERO Network",
        "h1": "ZERO Empty",
        "links": set(),
        "must_contain": {"Empty public profile", "makes no PnL, custody, or live trading claim"},
    },
    "stale-profile.html": {
        "title": "ZERO Network Demo \u00b7 ZERO Network",
        "h1": "ZERO Network Demo",
        "links": set(),
        "must_contain": {"Stale archive proof", "Do not treat it as current operator status"},
    },
    "leaderboard.html": {
        "title": "ZERO Network Leaderboard",
        "h1": "Public Leaderboard",
        "links": set(),
        "must_contain": {"Empty", "Active", "Stale"},
    },
}


@dataclass
class ParsedPage:
    title: str = ""
    h1: str = ""
    links: set[str] = field(default_factory=set)
    scripts: int = 0
    remote_refs: list[str] = field(default_factory=list)
    event_handlers: list[str] = field(default_factory=list)
    _stack: list[str] = field(default_factory=list)


class PageParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.page = ParsedPage()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.page._stack.append(tag)
        attr_map = {name.lower(): value or "" for name, value in attrs}
        if tag == "script":
            self.page.scripts += 1
        for name, value in attr_map.items():
            if name.startswith("on"):
                self.page.event_handlers.append(f"{tag}.{name}")
            if name in {"href", "src", "poster"}:
                if is_remote_or_script_ref(value):
                    self.page.remote_refs.append(value)
        href = attr_map.get("href")
        if tag == "a" and href:
            self.page.links.add(href)

    def handle_endtag(self, tag: str) -> None:
        if tag in self.page._stack:
            self.page._stack = self.page._stack[: self.page._stack.index(tag)]

    def handle_data(self, data: str) -> None:
        current = self.page._stack[-1] if self.page._stack else ""
        text = " ".join(data.split())
        if not text:
            return
        if current == "title":
            self.page.title = f"{self.page.title} {text}".strip()
        elif current == "h1":
            self.page.h1 = f"{self.page.h1} {text}".strip()


def main() -> int:
    parser = argparse.ArgumentParser(description="Smoke-test checked ZERO Network HTML pages.")
    parser.add_argument("--root", default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()

    root = Path(args.root)
    network_dir = root / "contracts" / "network"
    failures: list[str] = []

    for filename, expected in EXPECTED.items():
        page_path = network_dir / filename
        if not page_path.exists():
            failures.append(f"{filename}: missing")
            continue
        body = page_path.read_text(encoding="utf-8")
        parsed = parse_page(body)
        failures.extend(validate_page(filename, body, parsed, expected, network_dir))

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    print(f"network pages smoke passed: {len(EXPECTED)} pages")
    return 0


def parse_page(body: str) -> ParsedPage:
    parser = PageParser()
    parser.feed(body)
    return parser.page


def validate_page(
    filename: str,
    body: str,
    parsed: ParsedPage,
    expected: dict[str, object],
    network_dir: Path,
) -> list[str]:
    failures: list[str] = []
    if parsed.title != expected["title"]:
        failures.append(f"{filename}: title {parsed.title!r} != {expected['title']!r}")
    if parsed.h1 != expected["h1"]:
        failures.append(f"{filename}: h1 {parsed.h1!r} != {expected['h1']!r}")
    if parsed.scripts:
        failures.append(f"{filename}: contains {parsed.scripts} script tag(s)")
    if parsed.event_handlers:
        failures.append(f"{filename}: contains event handlers {sorted(parsed.event_handlers)}")
    if parsed.remote_refs:
        failures.append(f"{filename}: contains remote/script refs {sorted(parsed.remote_refs)}")

    expected_links = expected["links"]
    if not isinstance(expected_links, set):
        raise TypeError("expected links must be a set")
    missing_links = expected_links - parsed.links
    if missing_links:
        failures.append(f"{filename}: missing links {sorted(missing_links)}")
    expected_text = expected.get("must_contain", set())
    if not isinstance(expected_text, set):
        raise TypeError("expected must_contain must be a set")
    for text in expected_text:
        if text not in body:
            failures.append(f"{filename}: missing expected text {text!r}")
    for href in parsed.links:
        if is_remote_or_script_ref(href):
            failures.append(f"{filename}: link is not local-safe: {href}")
            continue
        target = (network_dir / href).resolve()
        if not target.exists() or network_dir.resolve() not in target.parents:
            failures.append(f"{filename}: local link target missing or outside contracts/network: {href}")

    for pattern in PRIVATE_PATTERNS:
        if pattern.search(body):
            failures.append(f"{filename}: private runtime token matched {pattern.pattern!r}")
    return failures


def is_remote_or_script_ref(value: str) -> bool:
    stripped = value.strip()
    if not stripped:
        return False
    lowered = stripped.lower()
    if lowered.startswith(("javascript:", "data:", "//")):
        return True
    parsed = urlparse(stripped)
    return parsed.scheme in {"http", "https"}


if __name__ == "__main__":
    raise SystemExit(main())
