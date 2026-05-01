from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path

from zero_engine import public_profile_page


def main() -> None:
    parser = argparse.ArgumentParser(description="Build a static ZERO Network profile page.")
    parser.add_argument(
        "--profile",
        default="contracts/network/profile.json",
        help="JSON file containing one zero.network.profile.v1 packet",
    )
    parser.add_argument(
        "--generated-at",
        default=datetime(2026, 5, 1, tzinfo=UTC).isoformat(),
        help="ISO-8601 timestamp for deterministic builds",
    )
    parser.add_argument("--output", help="Optional path to write HTML")
    args = parser.parse_args()

    profile = json.loads(Path(args.profile).read_text(encoding="utf-8"))
    page = public_profile_page(profile, generated_at=args.generated_at)

    if args.output:
        output_path = Path(args.output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(page, encoding="utf-8")
    else:
        print(page, end="")


if __name__ == "__main__":
    main()
