#!/usr/bin/env python3
"""Render the ZERO live canary policy lifecycle for a bundle or operator report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "engine" / "src"))

from zero_engine.live_canary_policy import build_live_canary_policy, inputs_from_rehearsal


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Evaluate live canary readiness, policy arm/disarm, bounded launch, "
            "evidence, qualification, and follow-through review from local artifacts."
        )
    )
    parser.add_argument(
        "path",
        type=Path,
        help="Canary bundle directory, operator workflow directory, manifest.json, or operator_report.json.",
    )
    parser.add_argument("--json", action="store_true", help="Emit the full policy JSON.")
    return parser.parse_args()


def load_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        payload = json.load(handle)
    return payload if isinstance(payload, dict) else {}


def embedded_policy(path: Path) -> dict[str, Any] | None:
    if path.is_file():
        payload = load_json(path)
        if payload.get("schema_version") == "zero.live_canary_policy.v1":
            return payload
        if payload.get("schema_version") == "zero.live_canary_operator.v1":
            policy = payload.get("policy")
            return policy if isinstance(policy, dict) else None
        policy = payload.get("policy")
        if isinstance(policy, dict) and policy.get("schema_version") == "zero.live_canary_policy.v1":
            return policy
    if path.is_dir():
        operator_path = path / "operator_report.json"
        if operator_path.is_file():
            policy = load_json(operator_path).get("policy")
            if isinstance(policy, dict) and policy.get("schema_version") == "zero.live_canary_policy.v1":
                return policy
        drill_packet = path / "10_live_canary_policy.json"
        if drill_packet.is_file():
            payload = load_json(drill_packet)
            packet_payload = payload.get("payload")
            if (
                isinstance(packet_payload, dict)
                and packet_payload.get("schema_version") == "zero.live_canary_policy.v1"
            ):
                return packet_payload
        manifest_path = path / "manifest.json"
        if manifest_path.is_file():
            policy = load_json(manifest_path).get("policy")
            if isinstance(policy, dict) and policy.get("schema_version") == "zero.live_canary_policy.v1":
                return policy
    return None


def resolve(path: Path) -> tuple[dict[str, Any], dict[str, Any] | None]:
    if path.is_file() and path.name == "operator_report.json":
        operator = load_json(path)
        bundle = Path(str(operator.get("bundle") or path.parent / "bundle"))
        if not bundle.is_absolute():
            bundle = path.parent / bundle
        return load_json(bundle / "manifest.json"), operator
    if path.is_file():
        payload = load_json(path)
        if payload.get("schema_version") == "zero.live_canary_operator.v1":
            bundle = Path(str(payload.get("bundle") or path.parent / "bundle"))
            if not bundle.is_absolute():
                bundle = path.parent / bundle
            return load_json(bundle / "manifest.json"), payload
        return payload, None
    operator_path = path / "operator_report.json"
    if operator_path.is_file():
        operator = load_json(operator_path)
        bundle = Path(str(operator.get("bundle") or path / "bundle"))
        if not bundle.is_absolute():
            bundle = path / bundle
        return load_json(bundle / "manifest.json"), operator
    return load_json(path / "manifest.json"), None


def render_text(policy: dict[str, Any]) -> str:
    summary = policy["summary"]
    recommendation = policy["recommendation"]
    return (
        "zero live canary policy: "
        f"qualified={summary['qualified']} "
        f"publishable={summary['publishable_canary_evidence']} "
        f"refusal_qualified={summary['refusal_evidence_qualified']} "
        f"next={recommendation['action']}"
    )


def main() -> int:
    args = parse_args()
    policy = embedded_policy(args.path)
    if policy is not None:
        if args.json:
            print(json.dumps(policy, indent=2, sort_keys=True))
        else:
            print(render_text(policy))
        return 0
    try:
        manifest, operator = resolve(args.path)
    except FileNotFoundError as exc:
        print(f"zero live canary policy: missing artifact: {exc.filename}", file=sys.stderr)
        return 2
    policy = build_live_canary_policy(inputs_from_rehearsal(manifest, operator_report=operator))
    if args.json:
        print(json.dumps(policy, indent=2, sort_keys=True))
    else:
        print(render_text(policy))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
