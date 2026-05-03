#!/usr/bin/env python3
"""Verify a ZERO deployment evidence directory."""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import os
import re
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "zero.deployment_evidence_verify.v1"
EVIDENCE_SCHEMA_VERSION = "zero.deployment_evidence.v1"
SIGNATURE_SCHEMA_VERSION = "zero.deployment_evidence_signature.v1"
SHA256_RE = re.compile(r"^[a-f0-9]{64}$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify manifest, SHA256SUMS, redaction, and optional signature for deployment evidence."
    )
    parser.add_argument("bundle", type=Path, help="Directory created by scripts/deployment_evidence.py.")
    parser.add_argument(
        "--signing-key",
        default=os.environ.get("ZERO_DEPLOYMENT_EVIDENCE_SIGNING_KEY", ""),
        help="Optional HMAC-SHA256 signing key used to verify EVIDENCE_SIGNATURE.json.",
    )
    parser.add_argument(
        "--require-signature",
        action="store_true",
        help="Fail unless EVIDENCE_SIGNATURE.json is present and verifies with --signing-key.",
    )
    parser.add_argument(
        "--forbid-token",
        action="append",
        default=[],
        help="Additional raw token that must not appear in JSON or captured logs.",
    )
    parser.add_argument("--json", action="store_true", help="Emit the verification report as JSON.")
    return parser.parse_args()


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_sha256s(path: Path) -> tuple[dict[str, str], list[str]]:
    entries: dict[str, str] = {}
    errors: list[str] = []
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw.strip():
            continue
        digest, separator, name = raw.partition("  ")
        if separator != "  " or not SHA256_RE.match(digest) or not name:
            errors.append(f"line {line_number}: malformed SHA256SUMS entry")
            continue
        entries[name] = digest
    return entries, errors


def body_text(bundle: Path) -> str:
    chunks: list[str] = []
    for path in sorted(bundle.iterdir()):
        if path.is_file() and path.suffix in {".json", ".txt"}:
            chunks.append(path.read_text(encoding="utf-8", errors="replace"))
    return "\n".join(chunks)


def add(findings: list[dict[str, str]], ok: bool, name: str, good: str, bad: str) -> None:
    findings.append({"status": "ok" if ok else "fail", "name": name, "detail": good if ok else bad})


def signature_signed_payload(bundle: Path, entries: dict[str, str]) -> dict[str, Any]:
    return {
        "schema_version": "zero.deployment_evidence_signature_payload.v1",
        "manifest_sha256": sha256(bundle / "manifest.json"),
        "sha256s_sha256": sha256(bundle / "SHA256SUMS"),
        "files": entries,
    }


def signed_payload_hash(payload: dict[str, Any]) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def expected_signature(signing_key: str, payload_hash: str) -> str:
    digest = hmac.new(
        signing_key.encode("utf-8"),
        payload_hash.encode("utf-8"),
        hashlib.sha256,
    ).hexdigest()
    return f"v1={digest}"


def verify(args: argparse.Namespace) -> dict[str, Any]:
    bundle = args.bundle
    findings: list[dict[str, str]] = []

    add(findings, bundle.is_dir(), "bundle_dir", str(bundle), "bundle directory missing")
    manifest_path = bundle / "manifest.json"
    sha_path = bundle / "SHA256SUMS"
    signature_path = bundle / "EVIDENCE_SIGNATURE.json"
    add(findings, manifest_path.is_file(), "manifest", "manifest.json present", "manifest.json missing")
    add(findings, sha_path.is_file(), "sha256s", "SHA256SUMS present", "SHA256SUMS missing")
    if not bundle.is_dir() or not manifest_path.is_file() or not sha_path.is_file():
        return build_report(args, findings, manifest=None, signature=None)

    manifest = load_json(manifest_path)
    add(
        findings,
        manifest.get("schema_version") == EVIDENCE_SCHEMA_VERSION,
        "manifest_schema",
        EVIDENCE_SCHEMA_VERSION,
        f"expected {EVIDENCE_SCHEMA_VERSION}",
    )
    doctor = manifest.get("doctor", {}) if isinstance(manifest, dict) else {}
    doctor_summary = doctor.get("summary", {}) if isinstance(doctor, dict) else {}
    add(
        findings,
        doctor_summary.get("fail") == 0,
        "doctor_failures",
        "doctor has zero failures",
        "doctor reported failures",
    )

    entries, parse_errors = read_sha256s(sha_path)
    add(findings, not parse_errors, "sha256_parse", f"{len(entries)} entries", "; ".join(parse_errors))
    expected_files = {
        path.name
        for path in bundle.iterdir()
        if path.is_file() and path.name not in {"SHA256SUMS", "EVIDENCE_SIGNATURE.json"}
    }
    add(
        findings,
        set(entries) == expected_files,
        "sha256_inventory",
        f"{len(entries)} files covered",
        "SHA256SUMS does not match evidence files",
    )
    for name, expected in sorted(entries.items()):
        path = bundle / name
        add(
            findings,
            path.is_file() and sha256(path) == expected,
            f"sha256:{name}",
            "hash matches",
            "hash mismatch or file missing",
        )

    files = manifest.get("files", []) if isinstance(manifest.get("files"), list) else []
    manifest_inventory = {item.get("path"): item.get("sha256") for item in files if isinstance(item, dict)}
    evidence_entries = {name: digest for name, digest in entries.items() if name != "manifest.json"}
    add(
        findings,
        manifest_inventory == evidence_entries,
        "manifest_file_inventory",
        "manifest files match SHA256SUMS",
        "manifest files do not match SHA256SUMS",
    )

    packets = manifest.get("packets", []) if isinstance(manifest.get("packets"), list) else []
    for packet in packets:
        if not isinstance(packet, dict):
            add(findings, False, "packet_shape", "packet ok", "packet entry is not an object")
            continue
        file_name = str(packet.get("file", ""))
        add(
            findings,
            bool(file_name) and (bundle / file_name).is_file(),
            f"packet_file:{packet.get('name')}",
            file_name,
            f"missing packet file {file_name}",
        )
        add(
            findings,
            int(packet.get("status", 0)) == 200,
            f"packet_status:{packet.get('name')}",
            "http 200",
            f"expected http 200, got {packet.get('status')}",
        )

    text = body_text(bundle)
    secret_patterns = {
        "raw_trace_id": r"trace-[A-Za-z0-9._:-]+",
        "bearer_token": r"Bearer\s+(?!REDACTED\b)[A-Za-z0-9._~+/=-]+",
        "openai_key": r"sk-[A-Za-z0-9]{16,}",
        "raw_private_key": r"(?i)private[_-]?key[\"']?\s*[:=]\s*(?!REDACTED\b)[^\"'\s,}]+",
        "raw_signing_key": r"(?i)signing[_-]?key[\"']?\s*[:=]\s*(?!REDACTED\b)[^\"'\s,}]+",
    }
    for name, pattern in secret_patterns.items():
        add(findings, re.search(pattern, text) is None, f"redaction:{name}", "not present", f"{name} found")
    for idx, token in enumerate(args.forbid_token, start=1):
        add(findings, token not in text, f"redaction:forbid_token_{idx}", "not present", "forbidden token found")

    signature: dict[str, Any] | None = None
    if signature_path.is_file():
        signature = load_json(signature_path)
        expected_payload = signature_signed_payload(bundle, entries)
        expected_payload_hash = signed_payload_hash(expected_payload)
        add(
            findings,
            signature.get("schema_version") == SIGNATURE_SCHEMA_VERSION,
            "signature_schema",
            SIGNATURE_SCHEMA_VERSION,
            f"expected {SIGNATURE_SCHEMA_VERSION}",
        )
        add(
            findings,
            signature.get("algorithm") == "hmac-sha256",
            "signature_algorithm",
            "hmac-sha256",
            "unsupported signature algorithm",
        )
        add(
            findings,
            signature.get("key_material_included") is False,
            "signature_key_material",
            "key material omitted",
            "signature artifact must not include key material",
        )
        add(
            findings,
            signature.get("signed_payload") == expected_payload,
            "signature_payload",
            "signed payload matches bundle",
            "signed payload does not match bundle",
        )
        add(
            findings,
            signature.get("signed_payload_hash") == expected_payload_hash,
            "signature_payload_hash",
            "hash matches signed payload",
            "signed payload hash mismatch",
        )
        if args.signing_key:
            add(
                findings,
                signature.get("signature") == expected_signature(args.signing_key, expected_payload_hash),
                "signature_value",
                "signature verifies",
                "signature does not verify",
            )
        else:
            add(
                findings,
                not args.require_signature,
                "signature_key",
                "signature key not required",
                "signature key required to verify EVIDENCE_SIGNATURE.json",
            )
    else:
        add(
            findings,
            not args.require_signature,
            "signature_present",
            "signature not required",
            "EVIDENCE_SIGNATURE.json missing",
        )

    return build_report(args, findings, manifest=manifest, signature=signature)


def build_report(
    args: argparse.Namespace,
    findings: list[dict[str, str]],
    *,
    manifest: dict[str, Any] | None,
    signature: dict[str, Any] | None,
) -> dict[str, Any]:
    fail = len([finding for finding in findings if finding["status"] == "fail"])
    return {
        "schema_version": SCHEMA_VERSION,
        "bundle": str(args.bundle),
        "ok": fail == 0,
        "summary": {
            "ok": len([finding for finding in findings if finding["status"] == "ok"]),
            "fail": fail,
        },
        "manifest": {
            "schema_version": None if manifest is None else manifest.get("schema_version"),
            "generated_at": None if manifest is None else manifest.get("generated_at"),
            "target": None if manifest is None else manifest.get("target"),
        },
        "signature": {
            "present": signature is not None,
            "schema_version": None if signature is None else signature.get("schema_version"),
            "algorithm": None if signature is None else signature.get("algorithm"),
            "signer": None if signature is None else signature.get("signer"),
        },
        "findings": findings,
    }


def render_text(report: dict[str, Any]) -> str:
    summary = report["summary"]
    header = (
        f"zero deployment evidence verify: ok={report['ok']} "
        f"checks={summary['ok']} fail={summary['fail']}"
    )
    failures = [
        f"- {finding['name']}: {finding['detail']}"
        for finding in report["findings"]
        if finding["status"] == "fail"
    ]
    return "\n".join([header, *failures])


def main() -> int:
    args = parse_args()
    report = verify(args)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(render_text(report))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
