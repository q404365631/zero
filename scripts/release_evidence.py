#!/usr/bin/env python3
"""Verify a published ZERO GitHub Release from a clean download."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Download and verify a published ZERO GitHub Release."
    )
    parser.add_argument("tag", help="Release tag, for example v0.1.1")
    parser.add_argument("--repo", default="zero-intel/zero", help="GitHub repository")
    parser.add_argument(
        "--keep-dir",
        action="store_true",
        help="Keep the temporary download directory for manual inspection.",
    )
    parser.add_argument("--json", action="store_true", help="Emit machine-readable JSON")
    return parser.parse_args()


def run(command: list[str], *, cwd: Path | None = None, capture: bool = False) -> str:
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            check=True,
            text=True,
            stdout=subprocess.PIPE if capture else None,
            stderr=subprocess.PIPE if capture else None,
        )
    except FileNotFoundError as exc:
        raise SystemExit(f"missing executable: {command[0]}") from exc
    except subprocess.CalledProcessError as exc:
        output = ""
        if exc.stdout:
            output += exc.stdout
        if exc.stderr:
            output += exc.stderr
        raise SystemExit(output.strip() or f"command failed: {' '.join(command)}") from exc
    return completed.stdout or ""


def github_release(repo: str, tag: str) -> dict[str, Any]:
    raw = run(
        [
            "gh",
            "release",
            "view",
            tag,
            "--repo",
            repo,
            "--json",
            "tagName,isDraft,isPrerelease,publishedAt,url,assets",
        ],
        capture=True,
    )
    data = json.loads(raw)
    return {
        "tag": data["tagName"],
        "draft": data["isDraft"],
        "prerelease": data["isPrerelease"],
        "published_at": data["publishedAt"],
        "url": data["url"],
        "assets": [
            {"name": asset["name"], "size": asset["size"], "url": asset["url"]}
            for asset in data["assets"]
        ],
    }


def release_verify(download_dir: Path) -> dict[str, Any]:
    raw = run(
        [
            sys.executable,
            str(ROOT / "scripts" / "release_verify.py"),
            str(download_dir),
            "--json",
        ],
        capture=True,
    )
    return json.loads(raw)


def shasum_verify(download_dir: Path) -> None:
    run(["shasum", "-a", "256", "-c", "SHA256SUMS"], cwd=download_dir, capture=True)


def verify_attestations(repo: str, download_dir: Path) -> list[str]:
    verified = []
    for asset in ("zero-linux", "zero-macos"):
        if (download_dir / asset).is_file():
            run(["gh", "attestation", "verify", asset, "--repo", repo], cwd=download_dir)
            verified.append(asset)
    return verified


def render_homebrew(download_dir: Path, tag: str) -> Path:
    output = download_dir / "zero.rb"
    run(
        [
            sys.executable,
            str(ROOT / "scripts" / "homebrew_formula.py"),
            str(download_dir),
            "--tag",
            tag,
            "--output",
            str(output),
        ]
    )
    if not output.is_file() or output.stat().st_size == 0:
        raise SystemExit("Homebrew formula renderer produced no output")
    return output


def main() -> int:
    args = parse_args()
    download_dir = Path(tempfile.mkdtemp(prefix=f"zero-release-{args.tag}-"))
    try:
        release = github_release(args.repo, args.tag)
        if release["draft"]:
            raise SystemExit(f"release is still a draft: {args.tag}")

        run(["gh", "release", "download", args.tag, "--repo", args.repo, "--dir", str(download_dir)])
        shasum_verify(download_dir)
        verify_report = release_verify(download_dir)
        if verify_report["summary"]["status"] != "ok":
            raise SystemExit(json.dumps(verify_report, indent=2, sort_keys=True))
        attestations = verify_attestations(args.repo, download_dir)
        formula = render_homebrew(download_dir, args.tag)

        report = {
            "schema_version": "zero.release_evidence.v1",
            "release": release,
            "download_dir": str(download_dir) if args.keep_dir else None,
            "download_dir_kept": args.keep_dir,
            "verification": verify_report["summary"],
            "attestations": attestations,
            "homebrew_formula": str(formula) if args.keep_dir else None,
        }
        if args.json:
            print(json.dumps(report, indent=2, sort_keys=True))
        else:
            print(
                "zero release evidence: ok "
                f"({args.tag}, assets={len(release['assets'])}, attestations={len(attestations)})"
            )
            print(f"release: {release['url']}")
            if args.keep_dir:
                print(f"download_dir: {download_dir}")
                print(f"homebrew_formula: {formula}")
        return 0
    finally:
        if args.keep_dir:
            print(f"kept release evidence directory: {download_dir}", file=sys.stderr)
        else:
            shutil.rmtree(download_dir, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
