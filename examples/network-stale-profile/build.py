#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path

from zero_engine import network_profile_freshness


def main() -> None:
    parser = argparse.ArgumentParser(description="Build a stale ZERO Network profile fixture.")
    parser.add_argument(
        "--profile",
        default="docs/proof/network/profile.json",
        help="Public zero.network.profile.v1 packet to evaluate.",
    )
    parser.add_argument(
        "--evaluated-at",
        default="2026-05-04T00:00:00+00:00",
        help="ISO-8601 timestamp used to evaluate deterministic freshness.",
    )
    parser.add_argument(
        "--output",
        default="examples/network-stale-profile/stale-profile.json",
        help="Output path for the stale profile fixture.",
    )
    args = parser.parse_args()

    profile = json.loads(Path(args.profile).read_text(encoding="utf-8"))
    freshness = network_profile_freshness(profile, evaluated_at=args.evaluated_at)
    payload = {
        "schema_version": "zero.network.stale_profile_fixture.v1",
        "description": "Deterministic public-safe fixture: proof valid, freshness stale.",
        "profile": profile,
        "proof": freshness["proof"],
        "freshness": freshness["freshness"],
        "claim_boundary": freshness["claim_boundary"],
        "privacy": freshness["privacy"],
    }

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {output}")


if __name__ == "__main__":
    main()
