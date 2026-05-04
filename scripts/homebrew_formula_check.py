#!/usr/bin/env python3
"""Validate the committed Homebrew formula is generated and public-safe."""

from __future__ import annotations

import difflib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
FORMULA_PATH = ROOT / "Formula" / "zero.rb"
RENDERER = ROOT / "scripts" / "homebrew_formula.py"


def fail(message: str) -> None:
    raise SystemExit(message)


def formula_text() -> str:
    if not FORMULA_PATH.is_file():
        fail(f"missing formula: {FORMULA_PATH.relative_to(ROOT)}")
    return FORMULA_PATH.read_text(encoding="utf-8")


def extract_release(text: str, asset: str) -> tuple[str, str]:
    pattern = re.compile(
        rf'releases/download/(?P<tag>v\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)/{asset}"\n'
        rf'\s+sha256 "(?P<sha>[a-f0-9]{{64}})"'
    )
    match = pattern.search(text)
    if match is None:
        fail(f"Formula/zero.rb missing {asset} release URL and sha256")
    return match.group("tag"), match.group("sha")


def assert_public_safe(text: str) -> None:
    required = [
        "class Zero < Formula",
        'desc "Operator terminal for self-custodial onchain operations"',
        'homepage "https://getzero.dev"',
        'license "Apache-2.0"',
        "The public runtime defaults to paper mode.",
        "docs/release.md",
        "docs/safety-model.md",
        "github.com/zero-intel/zero/releases/download/",
    ]
    for needle in required:
        if needle not in text:
            fail(f"Formula/zero.rb missing required text: {needle}")

    forbidden = [
        "private_key",
        "wallet_address",
        "ZERO_LIVE",
        "getzero/" + "zero-" + "private",
        "squaeragent/" + "zero",
        "PROPRIETARY",
    ]
    for needle in forbidden:
        if needle in text:
            fail(f"Formula/zero.rb contains forbidden text: {needle}")


def render_from_formula_checksums(text: str) -> str:
    macos_tag, macos_sha = extract_release(text, "zero-macos")
    linux_tag, linux_sha = extract_release(text, "zero-linux")
    if macos_tag != linux_tag:
        fail(f"Formula/zero.rb asset tags differ: {macos_tag} vs {linux_tag}")

    with tempfile.TemporaryDirectory() as raw_tmp:
        tmp = Path(raw_tmp)
        release_dir = tmp / "release"
        release_dir.mkdir()
        (release_dir / "SHA256SUMS").write_text(
            f"{linux_sha}  zero-linux\n{macos_sha}  zero-macos\n",
            encoding="utf-8",
        )
        rendered = tmp / "zero.rb"
        result = subprocess.run(
            [
                sys.executable,
                str(RENDERER),
                str(release_dir),
                "--tag",
                macos_tag,
                "--output",
                str(rendered),
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode != 0:
            fail(result.stderr.strip() or result.stdout.strip() or "formula renderer failed")
        return rendered.read_text(encoding="utf-8")


def assert_generated(text: str) -> None:
    rendered = render_from_formula_checksums(text)
    if text != rendered:
        diff = "\n".join(
            difflib.unified_diff(
                text.splitlines(),
                rendered.splitlines(),
                fromfile="Formula/zero.rb",
                tofile="scripts/homebrew_formula.py output",
                lineterm="",
            )
        )
        fail(f"Formula/zero.rb is not renderer-generated:\n{diff}")


def assert_ruby_syntax() -> None:
    if shutil.which("ruby") is None:
        return
    result = subprocess.run(
        ["ruby", "-c", str(FORMULA_PATH)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(result.stderr.strip() or result.stdout.strip() or "ruby syntax check failed")


def main() -> int:
    text = formula_text()
    assert_public_safe(text)
    assert_generated(text)
    assert_ruby_syntax()
    print("homebrew formula check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
