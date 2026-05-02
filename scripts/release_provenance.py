#!/usr/bin/env python3
"""Generate dependency-free SBOM and provenance files for ZERO releases."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import tomllib
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCHEMA_VERSION = "zero.release_provenance.v1"
SPDX_VERSION = "SPDX-2.3"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Write SBOM.spdx.json and PROVENANCE.json into a release directory."
    )
    parser.add_argument("release_dir", type=Path, help="Release asset directory")
    return parser.parse_args()


def read_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def git_value(*args: str) -> str | None:
    try:
        return subprocess.check_output(
            ["git", *args],
            cwd=ROOT,
            stderr=subprocess.DEVNULL,
            text=True,
        ).strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def spdx_id(value: str) -> str:
    safe = "".join(ch if ch.isalnum() else "-" for ch in value)
    return "SPDXRef-" + "-".join(part for part in safe.split("-") if part)


def cargo_lock_packages() -> list[dict[str, Any]]:
    lock_path = ROOT / "cli" / "Cargo.lock"
    if not lock_path.is_file():
        return []
    lock = read_toml(lock_path)
    packages = []
    for package in lock.get("package", []):
        packages.append(
            {
                "name": package.get("name"),
                "version": package.get("version"),
                "source": package.get("source", "workspace"),
                "checksum": package.get("checksum"),
            }
        )
    return sorted(packages, key=lambda item: (item["name"] or "", item["version"] or ""))


def python_dependencies() -> list[dict[str, Any]]:
    project = read_toml(ROOT / "engine" / "pyproject.toml").get("project", {})
    deps = [
        {"group": "runtime", "requirement": requirement}
        for requirement in project.get("dependencies", [])
    ]
    for group, requirements in project.get("optional-dependencies", {}).items():
        deps.extend({"group": group, "requirement": requirement} for requirement in requirements)
    return sorted(deps, key=lambda item: (item["group"], item["requirement"]))


def workspace_crates() -> list[dict[str, str]]:
    crates = []
    for manifest in sorted((ROOT / "cli" / "crates").glob("*/Cargo.toml")):
        package = read_toml(manifest).get("package", {})
        crates.append(
            {
                "name": package.get("name", manifest.parent.name),
                "manifest": str(manifest.relative_to(ROOT)),
                "description": package.get("description", ""),
            }
        )
    return crates


def release_assets(release_dir: Path) -> list[dict[str, Any]]:
    excluded = {"SHA256SUMS", "SBOM.spdx.json", "PROVENANCE.json"}
    assets = []
    for path in sorted(release_dir.iterdir()):
        if path.is_file() and path.name not in excluded:
            assets.append(
                {
                    "name": path.name,
                    "size": path.stat().st_size,
                    "sha256": sha256(path),
                }
            )
    return assets


def build_sbom(generated_at: str, assets: list[dict[str, Any]]) -> dict[str, Any]:
    project = read_toml(ROOT / "engine" / "pyproject.toml").get("project", {})
    workspace = read_toml(ROOT / "cli" / "Cargo.toml").get("workspace", {}).get("package", {})
    commit = git_value("rev-parse", "HEAD") or "unknown"
    namespace = f"https://github.com/zero-intel/zero/releases/sbom/{commit}"

    packages: list[dict[str, Any]] = [
        {
            "SPDXID": "SPDXRef-zero-engine",
            "name": project.get("name", "zero-engine"),
            "versionInfo": project.get("version", "unknown"),
            "downloadLocation": "https://github.com/zero-intel/zero/tree/main/engine",
            "filesAnalyzed": False,
            "licenseConcluded": project.get("license", "NOASSERTION"),
            "licenseDeclared": project.get("license", "NOASSERTION"),
            "supplier": "Organization: ZERO contributors",
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": f"pkg:pypi/{project.get('name', 'zero-engine')}@{project.get('version', 'unknown')}",
                }
            ],
        },
        {
            "SPDXID": "SPDXRef-zero-cli-workspace",
            "name": "zero-cli-workspace",
            "versionInfo": workspace.get("version", "unknown"),
            "downloadLocation": "https://github.com/zero-intel/zero/tree/main/cli",
            "filesAnalyzed": False,
            "licenseConcluded": workspace.get("license", "NOASSERTION"),
            "licenseDeclared": workspace.get("license", "NOASSERTION"),
            "supplier": "Organization: zero-intel",
        },
    ]

    for crate in workspace_crates():
        packages.append(
            {
                "SPDXID": spdx_id(f"crate-{crate['name']}"),
                "name": crate["name"],
                "versionInfo": workspace.get("version", "unknown"),
                "downloadLocation": f"https://github.com/zero-intel/zero/blob/main/{crate['manifest']}",
                "filesAnalyzed": False,
                "licenseConcluded": workspace.get("license", "NOASSERTION"),
                "licenseDeclared": workspace.get("license", "NOASSERTION"),
                "supplier": "Organization: zero-intel",
            }
        )

    for package in cargo_lock_packages():
        external_ref = {
            "referenceCategory": "PACKAGE-MANAGER",
            "referenceType": "purl",
            "referenceLocator": f"pkg:cargo/{package['name']}@{package['version']}",
        }
        packages.append(
            {
                "SPDXID": spdx_id(f"cargo-{package['name']}-{package['version']}"),
                "name": package["name"],
                "versionInfo": package["version"],
                "downloadLocation": package["source"],
                "filesAnalyzed": False,
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": "NOASSERTION",
                "supplier": "NOASSERTION",
                "checksums": (
                    [{"algorithm": "SHA256", "checksumValue": package["checksum"]}]
                    if package.get("checksum")
                    else []
                ),
                "externalRefs": [external_ref],
            }
        )

    for dep in python_dependencies():
        packages.append(
            {
                "SPDXID": spdx_id(f"python-{dep['group']}-{dep['requirement']}"),
                "name": dep["requirement"],
                "versionInfo": "NOASSERTION",
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": False,
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": "NOASSERTION",
                "supplier": "NOASSERTION",
                "annotations": [
                    {
                        "annotationDate": generated_at,
                        "annotationType": "OTHER",
                        "annotator": "Tool: scripts/release_provenance.py",
                        "comment": f"Python optional-dependency group: {dep['group']}",
                    }
                ],
            }
        )

    for asset in assets:
        packages.append(
            {
                "SPDXID": spdx_id(f"release-asset-{asset['name']}"),
                "name": asset["name"],
                "versionInfo": project.get("version", "unknown"),
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": False,
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": "NOASSERTION",
                "supplier": "Organization: zero-intel",
                "checksums": [{"algorithm": "SHA256", "checksumValue": asset["sha256"]}],
            }
        )

    return {
        "spdxVersion": SPDX_VERSION,
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "ZERO release SBOM",
        "documentNamespace": namespace,
        "creationInfo": {
            "created": generated_at,
            "creators": ["Tool: scripts/release_provenance.py", "Organization: zero-intel"],
        },
        "packages": packages,
        "relationships": [
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": "SPDXRef-zero-engine",
            },
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": "SPDXRef-zero-cli-workspace",
            },
        ],
    }


def build_provenance(generated_at: str, assets: list[dict[str, Any]]) -> dict[str, Any]:
    status = git_value("status", "--porcelain", "--untracked-files=no") or ""
    return {
        "schema_version": SCHEMA_VERSION,
        "generated_at": generated_at,
        "source": {
            "repository": "https://github.com/zero-intel/zero",
            "commit": git_value("rev-parse", "HEAD"),
            "branch": git_value("rev-parse", "--abbrev-ref", "HEAD"),
            "tag": git_value("describe", "--tags", "--exact-match"),
            "dirty": bool(status),
        },
        "release": {
            "assets": assets,
            "checksums": "SHA256SUMS",
            "sbom": "SBOM.spdx.json",
            "attestation": "GitHub artifact attestations generated by actions/attest",
        },
        "policy": {
            "paper_mode_default": True,
            "live_execution_claimed": False,
            "package_registry_publication": False,
            "requires_release_verify": True,
            "requires_attestation_verify": True,
        },
        "dependency_inputs": {
            "python": "engine/pyproject.toml",
            "cargo": "cli/Cargo.lock",
            "github_actions": ".github/dependabot.yml",
        },
    }


def main() -> int:
    args = parse_args()
    release_dir = args.release_dir.resolve()
    if not release_dir.is_dir():
        raise SystemExit(f"release directory does not exist: {release_dir}")

    generated_at = datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    assets = release_assets(release_dir)
    sbom = build_sbom(generated_at, assets)
    provenance = build_provenance(generated_at, assets)

    (release_dir / "SBOM.spdx.json").write_text(
        json.dumps(sbom, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (release_dir / "PROVENANCE.json").write_text(
        json.dumps(provenance, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"wrote SBOM.spdx.json and PROVENANCE.json to {release_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
