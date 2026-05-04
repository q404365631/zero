#!/usr/bin/env python3
"""Create and verify public-safe ZERO deployment identity evidence bundles."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


BUNDLE_SCHEMA_VERSION = "zero.deployment_identity_evidence.v1"
SIGNATURE_SCHEMA_VERSION = "zero.deployment_identity_signature.v1"
SIGNED_PAYLOAD_SCHEMA_VERSION = "zero.deployment_identity_signature_payload.v1"
CLAIM_SCHEMA_VERSION = "zero.deployment.claim.v1"
HEARTBEAT_SCHEMA_VERSION = "zero.deployment.heartbeat.v1"
SHA256_RE = re.compile(r"^sha256:[a-f0-9]{64}$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Create or verify a public-safe identity bundle that binds "
            "/deployment/claim, /deployment/heartbeat, and optional external "
            "OpenSSL signature metadata."
        )
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    create = subparsers.add_parser("create", help="Create an identity evidence bundle.")
    create.add_argument("claim", type=Path, help="JSON response from /deployment/claim.")
    create.add_argument("heartbeat", type=Path, help="JSON response from /deployment/heartbeat.")
    create.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output directory. Defaults to artifacts/deployment-identity/<timestamp>.",
    )
    create.add_argument(
        "--private-key",
        type=Path,
        default=None,
        help="Optional OpenSSL private key used to sign the identity payload.",
    )
    create.add_argument(
        "--public-key",
        type=Path,
        default=None,
        help="Optional OpenSSL public key. Required with --private-key.",
    )
    create.add_argument("--signer", default="local-operator", help="Signer label for the signature artifact.")
    create.add_argument(
        "--generated-at",
        default=None,
        help="Optional ISO-8601 timestamp for reproducible bundle fixtures.",
    )
    create.add_argument("--json", action="store_true", help="Emit bundle report JSON.")

    verify = subparsers.add_parser("verify", help="Verify an identity evidence bundle.")
    verify.add_argument("bundle", type=Path, help="Directory created by the create command.")
    verify.add_argument("--require-signature", action="store_true", help="Fail if the signature artifact is missing.")
    verify.add_argument("--forbid-token", action="append", default=[], help="Raw token that must not appear.")
    verify.add_argument("--json", action="store_true", help="Emit verification report JSON.")
    return parser.parse_args()


def default_output_dir() -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path("artifacts") / "deployment-identity" / stamp


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def sha256_json(payload: dict[str, Any]) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def canonical_packet_hash(packet: dict[str, Any], *, hash_key: str) -> str:
    body = {k: v for k, v in packet.items() if k not in {hash_key, "signature"}}
    return sha256_json(body)


def public_safe_text(*values: Any) -> str:
    return "\n".join(json.dumps(value, sort_keys=True) for value in values)


def identity_findings(claim: dict[str, Any], heartbeat: dict[str, Any], *, forbid_tokens: list[str] | None = None) -> list[dict[str, str]]:
    findings: list[dict[str, str]] = []

    def add(ok: bool, name: str, detail: str) -> None:
        findings.append({"status": "ok" if ok else "fail", "name": name, "detail": detail})

    add(claim.get("schema_version") == CLAIM_SCHEMA_VERSION, "claim_schema", CLAIM_SCHEMA_VERSION)
    add(heartbeat.get("schema_version") == HEARTBEAT_SCHEMA_VERSION, "heartbeat_schema", HEARTBEAT_SCHEMA_VERSION)
    claim_hash = claim.get("claim_hash")
    heartbeat_hash = heartbeat.get("heartbeat_hash")
    add(isinstance(claim_hash, str) and SHA256_RE.match(claim_hash) is not None, "claim_hash_shape", "claim hash is sha256")
    add(
        isinstance(heartbeat_hash, str) and SHA256_RE.match(heartbeat_hash) is not None,
        "heartbeat_hash_shape",
        "heartbeat hash is sha256",
    )
    add(claim_hash == canonical_packet_hash(claim, hash_key="claim_hash"), "claim_hash", "claim hash verifies")
    add(
        heartbeat_hash == canonical_packet_hash(heartbeat, hash_key="heartbeat_hash"),
        "heartbeat_hash",
        "heartbeat hash verifies",
    )
    add(heartbeat.get("deployment_claim_hash") == claim_hash, "heartbeat_binding", "heartbeat binds to claim")
    add(
        claim.get("signature", {}).get("signed_claim_hash") == claim_hash,
        "claim_signature_binding",
        "claim signature object binds claim hash",
    )
    add(
        heartbeat.get("signature", {}).get("signed_heartbeat_hash") == heartbeat_hash,
        "heartbeat_signature_binding",
        "heartbeat signature object binds heartbeat hash",
    )
    claim_privacy = claim.get("privacy", {})
    heartbeat_privacy = heartbeat.get("privacy", {})
    add(claim_privacy.get("contains_exchange_credentials") is False, "claim_no_credentials", "claim excludes credentials")
    add(heartbeat_privacy.get("contains_exchange_credentials") is False, "heartbeat_no_credentials", "heartbeat excludes credentials")
    add(claim_privacy.get("contains_wallet_material") is False, "claim_no_wallet", "claim excludes wallet material")
    add(heartbeat_privacy.get("contains_wallet_material") is False, "heartbeat_no_wallet", "heartbeat excludes wallet material")
    text = public_safe_text(claim, heartbeat).lower()
    for token in (
        "private_key",
        "secret_key",
        "wallet material",
        "exchange credential",
        "idempotency_key",
        "trace-",
        "bearer ",
    ):
        add(token not in text, f"redaction:{token.strip()}", "forbidden token absent")
    for idx, token in enumerate(forbid_tokens or [], start=1):
        add(token not in text, f"redaction:forbid_token_{idx}", "forbidden token absent")
    return findings


def signed_payload(bundle: dict[str, Any], public_key_text: str) -> dict[str, Any]:
    return {
        "schema_version": SIGNED_PAYLOAD_SCHEMA_VERSION,
        "bundle_schema_version": bundle["schema_version"],
        "claim_hash": bundle["claim"]["claim_hash"],
        "heartbeat_hash": bundle["heartbeat"]["heartbeat_hash"],
        "claim_file_sha256": bundle["files"]["claim_sha256"],
        "heartbeat_file_sha256": bundle["files"]["heartbeat_sha256"],
        "public_key_sha256": "sha256:" + sha256_text(public_key_text),
    }


def sign_payload(payload: dict[str, Any], private_key: Path) -> str:
    with tempfile.TemporaryDirectory() as tmp:
        payload_path = Path(tmp) / "payload.json"
        sig_path = Path(tmp) / "payload.sig"
        payload_path.write_text(json.dumps(payload, sort_keys=True, separators=(",", ":")), encoding="utf-8")
        subprocess.run(
            ["openssl", "dgst", "-sha256", "-sign", str(private_key), "-out", str(sig_path), str(payload_path)],
            check=True,
            capture_output=True,
            text=True,
        )
        return base64.b64encode(sig_path.read_bytes()).decode("ascii")


def verify_signature(payload: dict[str, Any], signature_b64: str, public_key_text: str) -> bool:
    try:
        signature = base64.b64decode(signature_b64.encode("ascii"), validate=True)
    except (ValueError, TypeError):
        return False
    with tempfile.TemporaryDirectory() as tmp:
        payload_path = Path(tmp) / "payload.json"
        sig_path = Path(tmp) / "payload.sig"
        public_key_path = Path(tmp) / "public.pem"
        payload_path.write_text(json.dumps(payload, sort_keys=True, separators=(",", ":")), encoding="utf-8")
        sig_path.write_bytes(signature)
        public_key_path.write_text(public_key_text, encoding="utf-8")
        result = subprocess.run(
            ["openssl", "dgst", "-sha256", "-verify", str(public_key_path), "-signature", str(sig_path), str(payload_path)],
            check=False,
            capture_output=True,
            text=True,
        )
        return result.returncode == 0


def write_sha256s(output_dir: Path) -> None:
    lines = []
    for path in sorted(output_dir.iterdir()):
        if path.is_file() and path.name != "SHA256SUMS":
            lines.append(f"{sha256_file(path)}  {path.name}")
    (output_dir / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def create_bundle(args: argparse.Namespace) -> int:
    if bool(args.private_key) != bool(args.public_key):
        print("--private-key and --public-key must be supplied together", file=sys.stderr)
        return 2

    claim = load_json(args.claim)
    heartbeat = load_json(args.heartbeat)
    output_dir = args.output or default_output_dir()
    output_dir.mkdir(parents=True, exist_ok=True)
    write_json(output_dir / "deployment_claim.json", claim)
    write_json(output_dir / "deployment_heartbeat.json", heartbeat)
    checks = identity_findings(claim, heartbeat)
    fail = len([check for check in checks if check["status"] == "fail"])
    bundle = {
        "schema_version": BUNDLE_SCHEMA_VERSION,
        "generated_at": args.generated_at or datetime.now(timezone.utc).isoformat(),
        "ok": fail == 0,
        "summary": {"ok": len(checks) - fail, "fail": fail},
        "claim": {
            "schema_version": claim.get("schema_version"),
            "claim_hash": claim.get("claim_hash"),
            "signature_status": claim.get("signature", {}).get("status"),
        },
        "heartbeat": {
            "schema_version": heartbeat.get("schema_version"),
            "heartbeat_hash": heartbeat.get("heartbeat_hash"),
            "deployment_claim_hash": heartbeat.get("deployment_claim_hash"),
            "signature_status": heartbeat.get("signature", {}).get("status"),
            "liveness_status": heartbeat.get("liveness", {}).get("status"),
        },
        "files": {
            "claim": "deployment_claim.json",
            "claim_sha256": "sha256:" + sha256_file(output_dir / "deployment_claim.json"),
            "heartbeat": "deployment_heartbeat.json",
            "heartbeat_sha256": "sha256:" + sha256_file(output_dir / "deployment_heartbeat.json"),
        },
        "privacy": {
            "public_safe": fail == 0,
            "contains_private_key": False,
            "contains_exchange_credentials": False,
            "contains_wallet_material": False,
        },
        "checks": checks,
    }
    write_json(output_dir / "identity_bundle.json", bundle)
    if args.private_key and args.public_key:
        public_key_text = args.public_key.read_text(encoding="utf-8")
        payload = signed_payload(bundle, public_key_text)
        signature = {
            "schema_version": SIGNATURE_SCHEMA_VERSION,
            "algorithm": "openssl-dgst-sha256",
            "signer": args.signer,
            "public_key": public_key_text,
            "public_key_sha256": payload["public_key_sha256"],
            "signed_payload_hash": sha256_json(payload),
            "signature": "base64:" + sign_payload(payload, args.private_key),
            "key_material_included": False,
            "signed_payload": payload,
        }
        write_json(output_dir / "IDENTITY_SIGNATURE.json", signature)
    write_sha256s(output_dir)
    if args.json:
        print(json.dumps(bundle, indent=2, sort_keys=True))
    else:
        print(f"zero deployment identity evidence: ok={bundle['ok']} output={output_dir}")
    return 0 if bundle["ok"] else 1


def read_sha256s(path: Path) -> dict[str, str]:
    entries: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        if not raw.strip():
            continue
        digest, separator, name = raw.partition("  ")
        if separator == "  ":
            entries[name] = digest
    return entries


def verify_bundle(args: argparse.Namespace) -> int:
    bundle_dir = args.bundle
    findings: list[dict[str, str]] = []

    def add(ok: bool, name: str, detail: str) -> None:
        findings.append({"status": "ok" if ok else "fail", "name": name, "detail": detail})

    bundle_path = bundle_dir / "identity_bundle.json"
    claim_path = bundle_dir / "deployment_claim.json"
    heartbeat_path = bundle_dir / "deployment_heartbeat.json"
    sha_path = bundle_dir / "SHA256SUMS"
    add(bundle_dir.is_dir(), "bundle_dir", "bundle directory exists")
    add(bundle_path.is_file(), "identity_bundle", "identity_bundle.json present")
    add(claim_path.is_file(), "claim_file", "deployment_claim.json present")
    add(heartbeat_path.is_file(), "heartbeat_file", "deployment_heartbeat.json present")
    add(sha_path.is_file(), "sha256s", "SHA256SUMS present")
    if not all(path.is_file() for path in (bundle_path, claim_path, heartbeat_path, sha_path)):
        return emit_verify(args, findings, signature_present=False)

    bundle = load_json(bundle_path)
    claim = load_json(claim_path)
    heartbeat = load_json(heartbeat_path)
    add(bundle.get("schema_version") == BUNDLE_SCHEMA_VERSION, "bundle_schema", BUNDLE_SCHEMA_VERSION)
    findings.extend(identity_findings(claim, heartbeat, forbid_tokens=args.forbid_token))
    entries = read_sha256s(sha_path)
    expected_files = {path.name for path in bundle_dir.iterdir() if path.is_file() and path.name != "SHA256SUMS"}
    add(set(entries) == expected_files, "sha256_inventory", "SHA256SUMS covers bundle files")
    for name, digest in sorted(entries.items()):
        add((bundle_dir / name).is_file() and sha256_file(bundle_dir / name) == digest, f"sha256:{name}", "hash matches")

    text = "\n".join(path.read_text(encoding="utf-8", errors="replace") for path in bundle_dir.iterdir() if path.is_file())
    for idx, token in enumerate(args.forbid_token, start=1):
        add(token not in text, f"bundle_redaction:forbid_token_{idx}", "forbidden token absent")

    signature_path = bundle_dir / "IDENTITY_SIGNATURE.json"
    signature_present = signature_path.is_file()
    add(signature_present or not args.require_signature, "signature_present", "signature presence policy satisfied")
    if signature_present:
        signature = load_json(signature_path)
        public_key = str(signature.get("public_key", ""))
        payload = signed_payload(bundle, public_key)
        raw_signature = str(signature.get("signature", ""))
        signature_b64 = raw_signature.removeprefix("base64:")
        add(signature.get("schema_version") == SIGNATURE_SCHEMA_VERSION, "signature_schema", SIGNATURE_SCHEMA_VERSION)
        add(signature.get("algorithm") == "openssl-dgst-sha256", "signature_algorithm", "openssl-dgst-sha256")
        add(signature.get("key_material_included") is False, "signature_key_material", "private key omitted")
        add(signature.get("public_key_sha256") == payload["public_key_sha256"], "signature_public_key_hash", "public key hash matches")
        add(signature.get("signed_payload") == payload, "signature_payload", "signed payload matches bundle")
        add(
            signature.get("signed_payload_hash") == sha256_json(payload),
            "signature_payload_hash",
            "signed payload hash matches",
        )
        add(verify_signature(payload, signature_b64, public_key), "signature_value", "signature verifies")

    return emit_verify(args, findings, signature_present=signature_present)


def emit_verify(args: argparse.Namespace, findings: list[dict[str, str]], *, signature_present: bool) -> int:
    fail = len([finding for finding in findings if finding["status"] == "fail"])
    report = {
        "schema_version": "zero.deployment_identity_evidence_verify.v1",
        "bundle": str(args.bundle),
        "ok": fail == 0,
        "summary": {"ok": len(findings) - fail, "fail": fail},
        "signature": {"present": signature_present},
        "findings": findings,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"zero deployment identity verify: ok={report['ok']} checks={report['summary']['ok']} fail={fail}")
        for finding in findings:
            if finding["status"] == "fail":
                print(f"- {finding['name']}: {finding['detail']}")
    return 0 if report["ok"] else 1


def main() -> int:
    args = parse_args()
    if args.command == "create":
        return create_bundle(args)
    if args.command == "verify":
        return verify_bundle(args)
    raise AssertionError(args.command)


if __name__ == "__main__":
    raise SystemExit(main())
