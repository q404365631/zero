#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path

from zero_engine import PaperEngine, PublicProfileConfig, public_profile


def main() -> None:
    parser = argparse.ArgumentParser(description="Build an empty ZERO Network profile fixture.")
    parser.add_argument(
        "--generated-at",
        default=datetime(2026, 5, 1, tzinfo=UTC).isoformat(),
        help="ISO-8601 timestamp for deterministic builds",
    )
    parser.add_argument(
        "--output",
        default="examples/network-empty-profile/empty-profile.json",
        help="Output path for the empty public profile packet.",
    )
    args = parser.parse_args()

    profile = public_profile(
        PaperEngine(),
        config=PublicProfileConfig(
            handle="zero_empty",
            display_name="ZERO Empty",
            publish_enabled=True,
        ),
        generated_at=args.generated_at,
    )

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(profile, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {output}")


if __name__ == "__main__":
    main()
