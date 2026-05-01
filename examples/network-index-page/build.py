from __future__ import annotations

import argparse
from datetime import UTC, datetime
from pathlib import Path

from zero_engine import public_network_index_page


def main() -> None:
    parser = argparse.ArgumentParser(description="Build a static ZERO Network index page.")
    parser.add_argument(
        "--generated-at",
        default=datetime(2026, 5, 1, tzinfo=UTC).isoformat(),
        help="ISO-8601 timestamp for deterministic builds",
    )
    parser.add_argument("--profile-href", default="profile.html")
    parser.add_argument("--leaderboard-href", default="leaderboard.html")
    parser.add_argument("--output", help="Optional path to write HTML")
    args = parser.parse_args()

    page = public_network_index_page(
        generated_at=args.generated_at,
        profile_href=args.profile_href,
        leaderboard_href=args.leaderboard_href,
    )

    if args.output:
        output_path = Path(args.output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(page, encoding="utf-8")
    else:
        print(page, end="")


if __name__ == "__main__":
    main()
