#!/usr/bin/env python3
"""Generate and verify ZERO's non-publishing registry launch packet."""

from __future__ import annotations

import argparse
import json
import tomllib
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "contracts" / "distribution" / "registry-launch.json"
SCHEMA_VERSION = "zero.registry_launch_packet.v1"
GENERATED_AT = "2026-05-04T00:00:00Z"


def load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def crate_names() -> list[str]:
    names: list[str] = []
    for manifest in sorted((ROOT / "cli" / "crates").glob("*/Cargo.toml")):
        data = load_toml(manifest)
        names.append(data.get("package", {}).get("name", manifest.parent.name))
    return names


def contains_any(source: str, needles: list[str]) -> bool:
    return any(needle in source for needle in needles)


def build_packet() -> dict[str, Any]:
    pyproject = load_toml(ROOT / "engine" / "pyproject.toml")
    cargo = load_toml(ROOT / "cli" / "Cargo.toml")
    formula = text("Formula/zero.rb")
    release_workflow = text(".github/workflows/release.yml")
    release_docs = text("docs/release.md")
    distribution_docs = text("docs/distribution.md")
    release_evidence = text("docs/releases/v0.1.2-evidence.md")

    project = pyproject["project"]
    workspace_package = cargo["workspace"]["package"]

    publishing_markers = {
        "pypi": ["pypa/gh-action-pypi-publish", "twine upload", "uv publish"],
        "crates": ["cargo publish", "cargo-release"],
        "container": ["docker/login-action", "ghcr.io/", "docker push"],
    }
    forbidden_found = {
        channel: [marker for marker in markers if marker in release_workflow]
        for channel, markers in publishing_markers.items()
    }

    channels = [
        {
            "channel": "github_release",
            "candidate": "zero-intel/zero",
            "status": "published",
            "current_release": "v0.1.2",
            "required_before_enablement": [],
            "evidence": [
                "docs/releases/v0.1.2-evidence.md",
                "Formula/zero.rb",
                ".github/workflows/release.yml",
            ],
        },
        {
            "channel": "homebrew_tap",
            "candidate": "zero-intel/zero",
            "status": "ready",
            "current_release": "v0.1.2",
            "required_before_enablement": [],
            "evidence": ["Formula/zero.rb", "docs/distribution.md", "docs/release.md"],
        },
        {
            "channel": "pypi",
            "candidate": project["name"],
            "status": "blocked",
            "current_release": None,
            "required_before_enablement": [
                "maintainer-controlled PyPI project or documented name-claim path",
                "PyPI Trusted Publishing configured for GitHub Actions",
                "test install from TestPyPI or staged package index",
                "rollback/deprecation procedure documented in release notes",
            ],
            "evidence": ["engine/pyproject.toml", "docs/distribution.md"],
        },
        {
            "channel": "crates_io",
            "candidate": ",".join(crate_names()),
            "status": "blocked",
            "current_release": None,
            "required_before_enablement": [
                "crate namespace review completed",
                "cargo owner evidence recorded after reservation/publication",
                "publish order documented for the workspace crate graph",
                "rollback/yank procedure documented in release notes",
            ],
            "evidence": ["cli/Cargo.toml", "cli/crates/*/Cargo.toml", "docs/distribution.md"],
        },
        {
            "channel": "container_registry",
            "candidate": "zero-intel/zero-paper",
            "status": "blocked",
            "current_release": None,
            "required_before_enablement": [
                "registry namespace is maintainer-controlled",
                "image name and labels state paper mode by default",
                "provenance and SBOM are attached to the image publication",
                "rollback/delete procedure documented in release notes",
            ],
            "evidence": ["Dockerfile", ".github/workflows/release.yml", "docs/distribution.md"],
        },
    ]

    checks = {
        "schema_version": SCHEMA_VERSION,
        "package_registry_publication_disabled": not any(forbidden_found.values()),
        "release_workflow_has_no_pypi_publish": not forbidden_found["pypi"],
        "release_workflow_has_no_cargo_publish": not forbidden_found["crates"],
        "release_workflow_has_no_container_push": not forbidden_found["container"],
        "pypi_candidate_matches_pyproject": project["name"] == "zero-engine",
        "cargo_workspace_version": workspace_package["version"],
        "homebrew_formula_tracks_current_release": (
            'version "0.1.2"' in formula
            and "releases/download/v0.1.2/zero-macos" in formula
            and "releases/download/v0.1.2/zero-linux" in formula
        ),
        "release_evidence_current": (
            "Release tag: `v0.1.2`" in release_evidence
            and "verification.fail=0" in release_evidence
            and "homebrew_formula_matches_committed=true" in release_evidence
        ),
        "docs_name_trusted_publishing": "Trusted Publishing" in distribution_docs,
        "docs_name_cargo_owner": "cargo owner" in distribution_docs,
        "docs_name_registry_rollback": "Registry Rollback" in distribution_docs,
        "docs_release_states_no_registry_publish": contains_any(
            release_docs,
            [
                "does not publish to PyPI",
                "Do not publish package-registry artifacts",
            ],
        ),
    }

    return {
        "schema_version": SCHEMA_VERSION,
        "generated_at": GENERATED_AT,
        "summary": {
            "default_distribution": "GitHub Release plus public Homebrew tap",
            "package_registries_enabled": False,
            "current_release": "v0.1.2",
            "policy": "Package registries stay blocked until ownership, tokenless publishing, and rollback evidence are recorded.",
        },
        "channels": channels,
        "checks": checks,
        "forbidden_publish_markers_found": forbidden_found,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Write the packet to a file")
    parser.add_argument("--check", action="store_true", help="Fail if the committed packet is stale")
    parser.add_argument("--json", action="store_true", help="Print the packet JSON")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    packet = build_packet()
    rendered = json.dumps(packet, indent=2, sort_keys=True) + "\n"

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")

    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != rendered:
            raise SystemExit(f"{OUTPUT.relative_to(ROOT)} is stale; run scripts/registry_launch_packet.py --output {OUTPUT.relative_to(ROOT)}")
        failed = [name for name, ok in packet["checks"].items() if isinstance(ok, bool) and not ok]
        if failed:
            raise SystemExit(f"registry launch packet checks failed: {', '.join(failed)}")

    if args.json or (not args.output and not args.check):
        print(rendered, end="")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
