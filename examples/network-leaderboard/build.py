from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path

from zero_engine import load_public_profiles, public_leaderboard


def main() -> None:
    parser = argparse.ArgumentParser(description="Build a public ZERO Network leaderboard.")
    parser.add_argument(
        "--profiles",
        default="examples/network-leaderboard/profiles.jsonl",
        help="JSONL file containing zero.network.profile.v1 packets",
    )
    parser.add_argument(
        "--generated-at",
        default=datetime(2026, 5, 1, tzinfo=UTC).isoformat(),
        help="ISO-8601 timestamp for deterministic builds",
    )
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--output", help="Optional path to write leaderboard JSON")
    args = parser.parse_args()

    profiles = load_public_profiles(args.profiles)
    leaderboard = public_leaderboard(profiles, generated_at=args.generated_at, limit=args.limit)
    body = json.dumps(leaderboard, indent=2, sort_keys=True) + "\n"

    if args.output:
        output_path = Path(args.output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(body, encoding="utf-8")
    else:
        print(body, end="")


if __name__ == "__main__":
    main()
