#!/usr/bin/env python3
"""Verify public-safe ZERO Network profile packets and identity evidence."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
ENGINE_SRC = ROOT / "engine" / "src"
if str(ENGINE_SRC) not in sys.path:
    sys.path.insert(0, str(ENGINE_SRC))
if str(Path(__file__).resolve().parent) not in sys.path:
    sys.path.insert(0, str(Path(__file__).resolve().parent))

from deployment_identity_evidence import (  # noqa: E402
    BUNDLE_SCHEMA_VERSION,
    SIGNATURE_SCHEMA_VERSION,
    identity_findings,
    read_sha256s,
    sha256_file,
    sha256_json,
    signed_payload,
    verify_signature,
)
from zero_engine.network import (  # noqa: E402
    PROFILE_SCHEMA_VERSION,
    SHA256_RE,
    assert_public_profile_safe,
    expected_profile_proof_hash,
    ingest_public_profiles,
)

REPORT_SCHEMA_VERSION = "zero.network.profile_verification.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Verify a zero.network.profile.v1 packet, its proof hash, deployment "
            "claim/heartbeat binding, and an optional signed deployment identity bundle."
        )
    )
    parser.add_argument("profile", type=Path, help="Path to a zero.network.profile.v1 JSON packet.")
    parser.add_argument(
        "--identity-bundle",
        type=Path,
        default=None,
        help="Optional directory created by scripts/deployment_identity_evidence.py.",
    )
    parser.add_argument(
        "--require-signed-identity",
        action="store_true",
        help="Fail unless --identity-bundle has a valid IDENTITY_SIGNATURE.json artifact.",
    )
    parser.add_argument(
        "--require-consent",
        action="store_true",
        help="Require profile.publish_enabled=true and hosted-compatible ingestion acceptance.",
    )
    parser.add_argument("--forbid-token", action="append", default=[], help="Raw token that must not appear.")
    parser.add_argument("--json", action="store_true", help="Emit full verification report JSON.")
    return parser.parse_args()


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def add_finding(findings: list[dict[str, str]], ok: bool, name: str, detail: str) -> None:
    findings.append({"status": "ok" if ok else "fail", "name": name, "detail": detail})


def public_safe_text(*values: Any) -> str:
    return "\n".join(json.dumps(value, sort_keys=True) for value in values)


def verify_profile(
    profile: dict[str, Any],
    *,
    require_consent: bool,
    forbid_tokens: list[str],
) -> list[dict[str, str]]:
    findings: list[dict[str, str]] = []
    add = lambda ok, name, detail: add_finding(findings, ok, name, detail)

    add(profile.get("schema_version") == PROFILE_SCHEMA_VERSION, "profile_schema", PROFILE_SCHEMA_VERSION)
    try:
        assert_public_profile_safe(profile)
    except ValueError as exc:
        add(False, "profile_public_safety", str(exc))
    else:
        add(True, "profile_public_safety", "profile excludes forbidden private runtime data")

    verification = profile.get("verification", {})
    proof_hash = verification.get("proof_hash")
    add(isinstance(proof_hash, str) and SHA256_RE.match(proof_hash) is not None, "proof_hash_shape", "proof hash is sha256")
    try:
        expected = expected_profile_proof_hash(profile)
    except (TypeError, ValueError, KeyError) as exc:
        add(False, "proof_hash", f"could not recompute proof hash: {exc}")
    else:
        add(proof_hash == expected, "proof_hash", "profile proof hash recomputes")

    row = profile.get("leaderboard_row", {})
    add(isinstance(row, dict), "leaderboard_row_shape", "leaderboard_row is object")
    add(row.get("proof_hash") == proof_hash, "leaderboard_proof_binding", "leaderboard row binds proof hash")
    add(
        row.get("deployment_claim_hash") == verification.get("deployment_claim_hash"),
        "leaderboard_claim_binding",
        "leaderboard row binds deployment claim hash",
    )
    add(
        row.get("deployment_heartbeat_hash") == verification.get("deployment_heartbeat_hash"),
        "leaderboard_heartbeat_binding",
        "leaderboard row binds deployment heartbeat hash",
    )

    claim = profile.get("deployment_claim")
    heartbeat = profile.get("deployment_heartbeat")
    claim_hash = verification.get("deployment_claim_hash")
    heartbeat_hash = verification.get("deployment_heartbeat_hash")
    add(isinstance(claim, dict), "deployment_claim_present", "profile includes deployment claim")
    add(isinstance(heartbeat, dict), "deployment_heartbeat_present", "profile includes deployment heartbeat")
    if isinstance(claim, dict):
        add(claim.get("claim_hash") == claim_hash, "profile_claim_binding", "claim hash matches verification")
    if isinstance(heartbeat, dict):
        add(heartbeat.get("heartbeat_hash") == heartbeat_hash, "profile_heartbeat_binding", "heartbeat hash matches verification")
        add(heartbeat.get("deployment_claim_hash") == claim_hash, "profile_heartbeat_claim_binding", "heartbeat binds claim hash")
    if isinstance(claim, dict) and isinstance(heartbeat, dict):
        findings.extend(identity_findings(claim, heartbeat, forbid_tokens=forbid_tokens))

    if require_consent:
        ingestion = ingest_public_profiles([profile], generated_at=str(profile.get("generated_at", "")))
        record = ingestion["records"][0]
        add(record["decision"] == "accepted", "hosted_ingestion_acceptance", "profile is accepted by hosted-compatible intake")
    else:
        add(True, "hosted_ingestion_acceptance", "not required")

    text = public_safe_text(profile)
    for idx, token in enumerate(forbid_tokens, start=1):
        add(token not in text, f"profile_redaction:forbid_token_{idx}", "forbidden token absent")
    return findings


def verify_identity_bundle(
    bundle_dir: Path,
    profile: dict[str, Any],
    *,
    require_signed_identity: bool,
    forbid_tokens: list[str],
) -> tuple[list[dict[str, str]], bool]:
    findings: list[dict[str, str]] = []
    add = lambda ok, name, detail: add_finding(findings, ok, name, detail)
    signature_present = False

    bundle_path = bundle_dir / "identity_bundle.json"
    claim_path = bundle_dir / "deployment_claim.json"
    heartbeat_path = bundle_dir / "deployment_heartbeat.json"
    sha_path = bundle_dir / "SHA256SUMS"
    add(bundle_dir.is_dir(), "identity_bundle_dir", "identity bundle directory exists")
    add(bundle_path.is_file(), "identity_bundle", "identity_bundle.json present")
    add(claim_path.is_file(), "identity_claim_file", "deployment_claim.json present")
    add(heartbeat_path.is_file(), "identity_heartbeat_file", "deployment_heartbeat.json present")
    add(sha_path.is_file(), "identity_sha256s", "SHA256SUMS present")
    if not all(path.is_file() for path in (bundle_path, claim_path, heartbeat_path, sha_path)):
        return findings, signature_present

    bundle = load_json(bundle_path)
    claim = load_json(claim_path)
    heartbeat = load_json(heartbeat_path)
    verification = profile.get("verification", {})
    add(bundle.get("schema_version") == BUNDLE_SCHEMA_VERSION, "identity_bundle_schema", BUNDLE_SCHEMA_VERSION)
    add(bundle.get("ok") is True, "identity_bundle_ok", "identity bundle reports ok")
    add(claim.get("claim_hash") == verification.get("deployment_claim_hash"), "identity_claim_profile_binding", "identity claim matches profile")
    add(
        heartbeat.get("heartbeat_hash") == verification.get("deployment_heartbeat_hash"),
        "identity_heartbeat_profile_binding",
        "identity heartbeat matches profile",
    )
    add(
        heartbeat.get("deployment_claim_hash") == claim.get("claim_hash"),
        "identity_heartbeat_claim_binding",
        "identity heartbeat binds identity claim",
    )
    findings.extend(identity_findings(claim, heartbeat, forbid_tokens=forbid_tokens))

    entries = read_sha256s(sha_path)
    expected_files = {path.name for path in bundle_dir.iterdir() if path.is_file() and path.name != "SHA256SUMS"}
    add(set(entries) == expected_files, "identity_sha256_inventory", "SHA256SUMS covers identity files")
    for name, digest in sorted(entries.items()):
        path = bundle_dir / name
        add(path.is_file() and sha256_file(path) == digest, f"identity_sha256:{name}", "hash matches")

    signature_path = bundle_dir / "IDENTITY_SIGNATURE.json"
    signature_present = signature_path.is_file()
    add(signature_present or not require_signed_identity, "identity_signature_present", "signature presence policy satisfied")
    if signature_present:
        signature = load_json(signature_path)
        public_key = str(signature.get("public_key", ""))
        payload = signed_payload(bundle, public_key)
        raw_signature = str(signature.get("signature", ""))
        add(signature.get("schema_version") == SIGNATURE_SCHEMA_VERSION, "identity_signature_schema", SIGNATURE_SCHEMA_VERSION)
        add(signature.get("algorithm") == "openssl-dgst-sha256", "identity_signature_algorithm", "openssl-dgst-sha256")
        add(signature.get("key_material_included") is False, "identity_signature_key_material", "private key omitted")
        add(signature.get("public_key_sha256") == payload["public_key_sha256"], "identity_signature_public_key_hash", "public key hash matches")
        add(signature.get("signed_payload") == payload, "identity_signature_payload", "signed payload matches bundle")
        add(signature.get("signed_payload_hash") == sha256_json(payload), "identity_signature_payload_hash", "signed payload hash matches")
        add(
            verify_signature(payload, raw_signature.removeprefix("base64:"), public_key),
            "identity_signature_value",
            "signature verifies",
        )

    text = "\n".join(path.read_text(encoding="utf-8", errors="replace") for path in bundle_dir.iterdir() if path.is_file())
    for idx, token in enumerate(forbid_tokens, start=1):
        add(token not in text, f"identity_redaction:forbid_token_{idx}", "forbidden token absent")
    return findings, signature_present


def main() -> int:
    args = parse_args()
    profile = load_json(args.profile)
    findings = verify_profile(
        profile,
        require_consent=args.require_consent,
        forbid_tokens=args.forbid_token,
    )
    identity = {
        "provided": args.identity_bundle is not None,
        "signature_present": False,
        "require_signed_identity": bool(args.require_signed_identity),
    }
    if args.identity_bundle is not None:
        identity_findings_report, signature_present = verify_identity_bundle(
            args.identity_bundle,
            profile,
            require_signed_identity=args.require_signed_identity,
            forbid_tokens=args.forbid_token,
        )
        findings.extend(identity_findings_report)
        identity["signature_present"] = signature_present
    elif args.require_signed_identity:
        add_finding(findings, False, "identity_bundle_required", "signed identity requires --identity-bundle")

    fail = len([finding for finding in findings if finding["status"] == "fail"])
    report = {
        "schema_version": REPORT_SCHEMA_VERSION,
        "profile": str(args.profile),
        "ok": fail == 0,
        "summary": {"ok": len(findings) - fail, "fail": fail},
        "identity": identity,
        "findings": findings,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"zero network profile verify: ok={report['ok']} checks={report['summary']['ok']} fail={fail}")
        for finding in findings:
            if finding["status"] == "fail":
                print(f"- {finding['name']}: {finding['detail']}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
