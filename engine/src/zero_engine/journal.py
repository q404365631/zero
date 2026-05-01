from __future__ import annotations

import json
import os
from collections.abc import Mapping
from pathlib import Path
from typing import Any


class DecisionJournal:
    """Append-only JSONL journal for replayable engine decisions."""

    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)

    def append(self, record: Mapping[str, Any]) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        line = json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n"
        fd = os.open(self.path, os.O_APPEND | os.O_CREAT | os.O_WRONLY, 0o600)
        with os.fdopen(fd, "a", encoding="utf-8") as handle:
            handle.write(line)
            handle.flush()
            os.fsync(handle.fileno())

    def tail(self, limit: int = 50) -> list[dict[str, Any]]:
        if limit <= 0:
            raise ValueError("limit must be positive")
        records = self.read_all()
        return records[-limit:]

    def read_all(self) -> list[dict[str, Any]]:
        if not self.path.exists():
            return []
        lines = self.path.read_text(encoding="utf-8").splitlines()
        return [json.loads(line) for line in lines if line.strip()]
