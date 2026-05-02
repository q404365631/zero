#!/usr/bin/env python3
"""Check package-registry readiness without publishing anything."""

from __future__ import annotations

import argparse
import json
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
PYPROJECT = ROOT / "engine" / "pyproject.toml"
CLI_WORKSPACE = ROOT / "cli" / "Cargo.toml"
CRATE_MANIFESTS = sorted((ROOT / "cli" / "crates").glob("*/Cargo.toml"))
SCHEMA_VERSION = "zero.registry_readiness.v1"


@dataclass(frozen=True)
class Finding:
    name: str
    status: str
    message: str
    evidence: dict[str, Any]


def load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def ok(name: str, message: str, **evidence: Any) -> Finding:
    return Finding(name, "ok", message, evidence)


def fail(name: str, message: str, **evidence: Any) -> Finding:
    return Finding(name, "fail", message, evidence)


def workspace_inherited(value: Any) -> bool:
    return isinstance(value, dict) and value.get("workspace") is True


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check package-registry readiness without publishing anything."
    )
    parser.add_argument("--json", action="store_true", help="Emit machine-readable JSON")
    return parser.parse_args()


def check_pyproject() -> list[Finding]:
    findings: list[Finding] = []
    data = load_toml(PYPROJECT)
    project = data.get("project", {})
    urls = project.get("urls", {})

    expected = {
        "name": "zero-engine",
        "license": "Apache-2.0",
        "readme": "README.md",
    }
    mismatches = {
        key: {"expected": expected_value, "actual": project.get(key)}
        for key, expected_value in expected.items()
        if project.get(key) != expected_value
    }
    if mismatches:
        findings.append(
            fail(
                "pypi.identity",
                "PyPI package identity metadata is incomplete",
                mismatches=mismatches,
            )
        )
    else:
        findings.append(
            ok("pypi.identity", "PyPI package identity metadata is present", package=project["name"])
        )

    required_urls = {"Homepage", "Repository", "Documentation", "Issues", "Changelog"}
    missing_urls = sorted(required_urls - set(urls))
    if missing_urls:
        findings.append(fail("pypi.urls", "PyPI project URLs are missing", missing=missing_urls))
    else:
        findings.append(ok("pypi.urls", "PyPI project URLs are present", urls=sorted(urls)))

    classifiers = set(project.get("classifiers", []))
    required_classifiers = {
        "Programming Language :: Python :: 3 :: Only",
        "Topic :: Office/Business :: Financial",
        "Typing :: Typed",
    }
    missing_classifiers = sorted(required_classifiers - classifiers)
    if missing_classifiers:
        findings.append(
            fail("pypi.classifiers", "PyPI classifiers are missing", missing=missing_classifiers)
        )
    else:
        findings.append(ok("pypi.classifiers", "PyPI classifiers are present", count=len(classifiers)))

    keywords = project.get("keywords", [])
    if not isinstance(keywords, list) or len(keywords) < 4:
        findings.append(fail("pypi.keywords", "PyPI keywords must describe the package", keywords=keywords))
    else:
        findings.append(ok("pypi.keywords", "PyPI keywords are present", keywords=keywords))

    optional_deps = project.get("optional-dependencies", {})
    if "live" not in optional_deps:
        findings.append(fail("pypi.live_extra", "live exchange dependencies must stay optional"))
    else:
        findings.append(
            ok(
                "pypi.live_extra",
                "live exchange dependencies are isolated",
                live=optional_deps["live"],
            )
        )

    return findings


def check_cargo() -> list[Finding]:
    findings: list[Finding] = []
    workspace = load_toml(CLI_WORKSPACE)
    package = workspace.get("workspace", {}).get("package", {})
    required_workspace = {
        "license": "Apache-2.0",
        "repository": "https://github.com/zero-intel/zero",
        "homepage": "https://getzero.dev",
        "documentation": "https://github.com/zero-intel/zero/tree/main/docs",
    }
    mismatches = {
        key: {"expected": expected, "actual": package.get(key)}
        for key, expected in required_workspace.items()
        if package.get(key) != expected
    }
    if mismatches:
        findings.append(
            fail(
                "crates.workspace_identity",
                "Cargo workspace package metadata is incomplete",
                mismatches=mismatches,
            )
        )
    else:
        findings.append(ok("crates.workspace_identity", "Cargo workspace package metadata is present"))

    if len(package.get("keywords", [])) < 4 or not package.get("categories"):
        findings.append(
            fail(
                "crates.workspace_discovery",
                "Cargo workspace needs keywords and categories for registry discovery",
                keywords=package.get("keywords"),
                categories=package.get("categories"),
            )
        )
    else:
        findings.append(
            ok(
                "crates.workspace_discovery",
                "Cargo workspace registry discovery metadata is present",
                keywords=package["keywords"],
                categories=package["categories"],
            )
        )

    required_inherited = [
        "version",
        "edition",
        "rust-version",
        "license",
        "repository",
        "homepage",
        "documentation",
        "keywords",
        "categories",
    ]
    crate_failures: dict[str, list[str]] = {}
    crate_names: list[str] = []
    for manifest in CRATE_MANIFESTS:
        data = load_toml(manifest)
        crate_package = data.get("package", {})
        name = crate_package.get("name", manifest.parent.name)
        crate_names.append(name)
        missing = [
            field
            for field in required_inherited
            if not workspace_inherited(crate_package.get(field))
        ]
        if not crate_package.get("description"):
            missing.append("description")
        if missing:
            crate_failures[name] = missing

    if crate_failures:
        findings.append(
            fail(
                "crates.package_metadata",
                "one or more crate manifests are missing publish metadata",
                crates=crate_failures,
            )
        )
    else:
        findings.append(
            ok(
                "crates.package_metadata",
                "all crate manifests inherit registry metadata",
                crates=crate_names,
            )
        )

    if "zero" not in crate_names:
        findings.append(fail("crates.zero_binary", "workspace must include the zero binary crate", crates=crate_names))
    else:
        findings.append(ok("crates.zero_binary", "zero binary crate is present"))

    return findings


def check_docs() -> list[Finding]:
    findings: list[Finding] = []
    distribution = (ROOT / "docs" / "distribution.md").read_text(encoding="utf-8")
    release = (ROOT / "docs" / "release.md").read_text(encoding="utf-8")
    template = (ROOT / ".github" / "RELEASE_TEMPLATE.md").read_text(encoding="utf-8")
    required_phrases = {
        "docs/distribution.md": [
            "Trusted Publishing",
            "cargo owner",
            "Homebrew Formula Requirements",
            "Registry Rollback",
        ],
        "docs/release.md": [
            "just registry-readiness",
            "Trusted Publishing",
            "does not publish to PyPI",
        ],
        ".github/RELEASE_TEMPLATE.md": [
            "just registry-readiness",
            "package registry publication remains disabled",
        ],
    }
    sources = {
        "docs/distribution.md": distribution,
        "docs/release.md": release,
        ".github/RELEASE_TEMPLATE.md": template,
    }
    missing = {
        path: [phrase for phrase in phrases if phrase not in sources[path]]
        for path, phrases in required_phrases.items()
    }
    missing = {path: phrases for path, phrases in missing.items() if phrases}
    if missing:
        findings.append(fail("registry.docs", "registry guardrail docs are incomplete", missing=missing))
    else:
        findings.append(ok("registry.docs", "registry guardrail docs are present"))
    return findings


def build_report() -> dict[str, Any]:
    findings = [*check_pyproject(), *check_cargo(), *check_docs()]
    fail_count = sum(1 for finding in findings if finding.status == "fail")
    ok_count = sum(1 for finding in findings if finding.status == "ok")
    return {
        "schema_version": SCHEMA_VERSION,
        "summary": {
            "status": "fail" if fail_count else "ok",
            "ok": ok_count,
            "fail": fail_count,
        },
        "findings": [
            {
                "name": finding.name,
                "status": finding.status,
                "message": finding.message,
                "evidence": finding.evidence,
            }
            for finding in findings
        ],
    }


def emit_text(report: dict[str, Any]) -> None:
    summary = report["summary"]
    print(f"zero registry readiness: {summary['status']} ({summary['ok']} ok, {summary['fail']} fail)")
    for finding in report["findings"]:
        print(f"{finding['status']:>4} {finding['name']}: {finding['message']}")


def main() -> int:
    args = parse_args()
    report = build_report()
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        emit_text(report)
    return 1 if report["summary"]["fail"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
