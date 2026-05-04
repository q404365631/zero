#!/usr/bin/env python3
"""Build the deterministic public-safe ZERO Network proof pack."""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import shutil
import sys
import tempfile
from datetime import UTC, datetime
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
    identity_findings,
    write_json,
    write_sha256s,
)
from network_profile_verify import (  # noqa: E402
    REPORT_SCHEMA_VERSION,
    verify_identity_bundle,
    verify_profile,
)
from zero_engine.api import PaperApi, PaperApiState  # noqa: E402
from zero_engine.journal import DecisionJournal  # noqa: E402
from zero_engine.models import OrderIntent, Side  # noqa: E402
from zero_engine.network import ingest_public_profiles, public_leaderboard  # noqa: E402
from zero_engine.paper import PaperEngine  # noqa: E402

OUTPUT_DIR = ROOT / "docs" / "proof" / "network"
GENERATED_AT = "2026-05-01T00:00:00Z"
FIXED_DT = datetime(2026, 5, 1, tzinfo=UTC)
FIXED_TS = FIXED_DT.timestamp()
FORBID_TOKENS = ("network-proof-fill", "network-proof-reject", "trace-network-proof")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def sha256_json(payload: dict[str, Any]) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return f"sha256:{hashlib.sha256(encoded).hexdigest()}"


def build_api(work_dir: Path) -> PaperApi:
    journal = DecisionJournal(work_dir / "decisions.jsonl")
    api = PaperApi(
        PaperApiState(
            engine=PaperEngine(clock=lambda: FIXED_TS, journal=journal),
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
            network_handle="zero_network_demo",
            network_display_name="ZERO Network Demo",
            network_publish_enabled=True,
            deployment_id="zero-network-demo",
            deployment_owner="zero-network-demo",
            deployment_version="0.1.1",
        )
    )
    api.execute(
        {
            "coin": "BTC",
            "side": "buy",
            "size": 0.01,
            "idempotency_key": "network-proof-fill",
        },
        trace_id="trace-network-proof-fill",
    )
    api.execute(
        {
            "coin": "ETH",
            "side": "buy",
            "size": 10.0,
            "idempotency_key": "network-proof-reject",
        },
        trace_id="trace-network-proof-reject",
    )
    return api


def write_identity_bundle(output_dir: Path, profile: dict[str, Any]) -> Path:
    identity_dir = output_dir / "identity"
    identity_dir.mkdir(parents=True, exist_ok=True)
    claim = profile["deployment_claim"]
    heartbeat = profile["deployment_heartbeat"]
    write_json(identity_dir / "deployment_claim.json", claim)
    write_json(identity_dir / "deployment_heartbeat.json", heartbeat)
    checks = identity_findings(claim, heartbeat, forbid_tokens=list(FORBID_TOKENS))
    fail = len([check for check in checks if check["status"] == "fail"])
    bundle = {
        "schema_version": BUNDLE_SCHEMA_VERSION,
        "generated_at": GENERATED_AT,
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
            "claim_sha256": sha256_file(identity_dir / "deployment_claim.json"),
            "heartbeat": "deployment_heartbeat.json",
            "heartbeat_sha256": sha256_file(identity_dir / "deployment_heartbeat.json"),
        },
        "privacy": {
            "public_safe": fail == 0,
            "contains_private_key": False,
            "contains_exchange_credentials": False,
            "contains_wallet_material": False,
        },
        "checks": checks,
    }
    write_json(identity_dir / "identity_bundle.json", bundle)
    write_sha256s(identity_dir)
    return identity_dir


def verification_report(profile: dict[str, Any], identity_dir: Path) -> dict[str, Any]:
    findings = verify_profile(
        profile,
        require_consent=True,
        forbid_tokens=list(FORBID_TOKENS),
    )
    identity_findings_report, signature_present = verify_identity_bundle(
        identity_dir,
        profile,
        require_signed_identity=False,
        forbid_tokens=list(FORBID_TOKENS),
    )
    findings.extend(identity_findings_report)
    fail = len([finding for finding in findings if finding["status"] == "fail"])
    return {
        "schema_version": REPORT_SCHEMA_VERSION,
        "profile": "profile.json",
        "ok": fail == 0,
        "summary": {"ok": len(findings) - fail, "fail": fail},
        "identity": {
            "provided": True,
            "signature_present": signature_present,
            "require_signed_identity": False,
        },
        "findings": findings,
    }


def write_readme(path: Path) -> None:
    path.write_text(
        """# ZERO Network Proof Pack

This directory is generated by `scripts/network_proof_pack.py`.

It is a deterministic public-safe proof chain for ZERO Network:

- `profile.json` is a redacted `zero.network.profile.v1` packet;
- `leaderboard.json` is derived from that same profile;
- `identity/` binds the deployment claim and heartbeat hashes;
- `profile-verification.json` recomputes the proof and binding checks;
- `network-proof-pack.json` hash-addresses every artifact.

The pack is paper-mode only. It does not claim live trading, PnL, or
paper-vs-live correlation.

Verify it with:

```bash
PYTHONPATH="$PWD/engine/src" scripts/network_proof_pack.py --check
```
""",
        encoding="utf-8",
    )


def artifact_hashes(output_dir: Path) -> dict[str, str]:
    names = (
        "README.md",
        "profile.json",
        "leaderboard.json",
        "profile-verification.json",
        "identity/deployment_claim.json",
        "identity/deployment_heartbeat.json",
        "identity/identity_bundle.json",
        "identity/SHA256SUMS",
    )
    return {name: sha256_file(output_dir / name) for name in names}


def build(output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        api = build_api(Path(tmp))
        profile = api.network_profile()

    write_readme(output_dir / "README.md")
    write_json(output_dir / "profile.json", profile)
    leaderboard = public_leaderboard([profile], generated_at=GENERATED_AT)
    ingestion = ingest_public_profiles([profile], generated_at=GENERATED_AT)
    write_json(output_dir / "leaderboard.json", leaderboard)
    identity_dir = write_identity_bundle(output_dir, profile)
    report = verification_report(profile, identity_dir)
    write_json(output_dir / "profile-verification.json", report)

    manifest = {
        "schema_version": "zero.network_proof_pack.v1",
        "generated_at": GENERATED_AT,
        "name": "demo-network-proof-chain",
        "mode": "paper",
        "profile_schema_version": profile["schema_version"],
        "verification_schema_version": report["schema_version"],
        "ok": report["ok"] and ingestion["summary"]["accepted"] == 1,
        "claim_boundary": {
            "paper_mode_verified": True,
            "hosted_ingestion_compatible": ingestion["summary"]["accepted"] == 1,
            "live_trading_claimed": False,
            "paper_vs_live_correlation_claimed": False,
            "pnl_claimed": False,
        },
        "bindings": {
            "proof_hash": profile["verification"]["proof_hash"],
            "deployment_claim_hash": profile["verification"]["deployment_claim_hash"],
            "deployment_heartbeat_hash": profile["verification"]["deployment_heartbeat_hash"],
            "leaderboard_proof_hash": leaderboard["rows"][0]["proof_hash"],
        },
        "identity": {
            "bundle_schema_version": BUNDLE_SCHEMA_VERSION,
            "signature_present": False,
            "signature_required_for_static_fixture": False,
            "signed_identity_smoke_tested_in_ci": True,
        },
        "verification": {
            "ok": report["ok"],
            "checks_ok": report["summary"]["ok"],
            "checks_failed": report["summary"]["fail"],
            "hosted_ingestion_accepted": ingestion["summary"]["accepted"],
        },
        "privacy": {
            "contains_exchange_credentials": False,
            "contains_wallet_material": False,
            "contains_raw_decisions": False,
            "contains_trace_tokens": False,
            "contains_idempotency_tokens": False,
        },
        "artifacts": artifact_hashes(output_dir),
    }
    manifest["proof_hash"] = sha256_json(manifest)
    write_json(output_dir / "network-proof-pack.json", manifest)


def compare_dirs(current: Path, expected: Path) -> int:
    status = 0
    paths = sorted({path.relative_to(current) for path in current.rglob("*") if path.is_file()})
    paths.extend(
        path.relative_to(expected)
        for path in expected.rglob("*")
        if path.is_file() and path.relative_to(expected) not in paths
    )
    for rel in sorted(set(paths)):
        left = current / rel
        right = expected / rel
        left_text = left.read_text(encoding="utf-8") if left.exists() else ""
        right_text = right.read_text(encoding="utf-8") if right.exists() else ""
        if left_text == right_text:
            continue
        status = 1
        diff = difflib.unified_diff(
            left_text.splitlines(),
            right_text.splitlines(),
            fromfile=str(left),
            tofile=str(right),
            lineterm="",
        )
        print("\n".join(list(diff)[:240]), file=sys.stderr)
    return status


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if generated Network proof pack is stale")
    args = parser.parse_args()

    if args.check:
        with tempfile.TemporaryDirectory() as tmp:
            expected = Path(tmp) / "network"
            build(expected)
            status = compare_dirs(OUTPUT_DIR, expected)
            if status:
                print("docs/proof/network is stale; run scripts/network_proof_pack.py", file=sys.stderr)
            return status

    if OUTPUT_DIR.exists():
        shutil.rmtree(OUTPUT_DIR)
    build(OUTPUT_DIR)
    print(f"wrote {OUTPUT_DIR.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
