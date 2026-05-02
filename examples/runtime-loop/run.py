#!/usr/bin/env python3
from __future__ import annotations

import json
import tempfile
from pathlib import Path

from zero_engine.runtime import RuntimeLoop, load_runtime_config


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory(prefix="zero-runtime-loop-") as tmp:
        config = load_runtime_config(
            scenario_path=repo_root / "examples" / "paper-trading" / "scenario.json",
            decision_journal_path=Path(tmp) / "decisions.jsonl",
            cycle_journal_path=Path(tmp) / "cycles.jsonl",
            runtime_bus_path=Path(tmp) / "runtime-bus",
            interval_s=0,
        )
        loop = RuntimeLoop.from_config(config)
        loop.engine.clock = lambda: 1777646400.0
        record = loop.run_once()
        bus_audit = loop.bus.export_audit() if loop.bus is not None else None
        print(
            json.dumps(
                {
                    **record.to_dict(),
                    "bus": {
                        "integrity": bus_audit["integrity"] if bus_audit else None,
                        "summary": bus_audit["summary"] if bus_audit else None,
                    },
                },
                indent=2,
                sort_keys=True,
            )
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
