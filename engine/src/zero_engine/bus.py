from __future__ import annotations

import hashlib
import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any


RUNTIME_EVENT_SCHEMA_VERSION = "zero.runtime.event.v1"
RUNTIME_SNAPSHOT_SCHEMA_VERSION = "zero.runtime.snapshot.v1"
RUNTIME_AUDIT_SCHEMA_VERSION = "zero.runtime.audit.v1"


@dataclass(frozen=True)
class RuntimeBusEvent:
    schema_version: str
    event_id: str
    event_index: int
    event_type: str
    as_of: float
    payload: dict[str, Any]
    previous_checksum: str | None
    checksum: str
    trace_id: str | None = None

    def to_dict(self) -> dict[str, Any]:
        payload = {
            "schema_version": self.schema_version,
            "event_id": self.event_id,
            "event_index": self.event_index,
            "event_type": self.event_type,
            "as_of": self.as_of,
            "payload": self.payload,
            "previous_checksum": self.previous_checksum,
            "checksum": self.checksum,
        }
        if self.trace_id:
            payload["trace_id"] = self.trace_id
        return payload

    @classmethod
    def from_dict(cls, payload: dict[str, Any]) -> "RuntimeBusEvent":
        return cls(
            schema_version=str(payload["schema_version"]),
            event_id=str(payload["event_id"]),
            event_index=int(payload["event_index"]),
            event_type=str(payload["event_type"]),
            as_of=float(payload["as_of"]),
            payload=dict(payload["payload"]),
            previous_checksum=(
                str(payload["previous_checksum"]) if payload.get("previous_checksum") else None
            ),
            checksum=str(payload["checksum"]),
            trace_id=str(payload["trace_id"]) if payload.get("trace_id") else None,
        )


@dataclass(frozen=True)
class RuntimeBusIntegrity:
    ok: bool
    events: int
    last_checksum: str | None
    reason: str = "ok"

    def to_dict(self) -> dict[str, Any]:
        return {
            "ok": self.ok,
            "events": self.events,
            "last_checksum": self.last_checksum,
            "reason": self.reason,
        }


class DurableRuntimeBus:
    """Append-only local runtime bus with checksum chaining and state snapshots."""

    def __init__(self, root: str | Path) -> None:
        self.root = Path(root)
        self.events_path = self.root / "events.jsonl"
        self.snapshot_path = self.root / "state-snapshot.json"

    def append(
        self,
        event_type: str,
        payload: dict[str, Any],
        *,
        as_of: float,
        trace_id: str | None = None,
    ) -> RuntimeBusEvent:
        events = self.read_events()
        previous_checksum = events[-1].checksum if events else None
        event_index = len(events) + 1
        event_id = f"evt-{event_index:012d}"
        checksum = checksum_event(
            {
                "schema_version": RUNTIME_EVENT_SCHEMA_VERSION,
                "event_id": event_id,
                "event_index": event_index,
                "event_type": event_type,
                "as_of": as_of,
                "payload": payload,
                "previous_checksum": previous_checksum,
                **({"trace_id": trace_id} if trace_id else {}),
            }
        )
        event = RuntimeBusEvent(
            schema_version=RUNTIME_EVENT_SCHEMA_VERSION,
            event_id=event_id,
            event_index=event_index,
            event_type=event_type,
            as_of=as_of,
            payload=payload,
            previous_checksum=previous_checksum,
            checksum=checksum,
            trace_id=trace_id,
        )
        self.root.mkdir(parents=True, exist_ok=True)
        append_jsonl(self.events_path, event.to_dict())
        return event

    def write_snapshot(
        self,
        payload: dict[str, Any],
        *,
        as_of: float,
        source_event_id: str | None = None,
    ) -> dict[str, Any]:
        integrity = self.verify_integrity()
        snapshot = {
            "schema_version": RUNTIME_SNAPSHOT_SCHEMA_VERSION,
            "as_of": as_of,
            "source_event_id": source_event_id,
            "event_count": integrity.events,
            "last_checksum": integrity.last_checksum,
            "payload": payload,
        }
        self.root.mkdir(parents=True, exist_ok=True)
        write_json_atomic(self.snapshot_path, snapshot)
        return snapshot

    def read_snapshot(self) -> dict[str, Any] | None:
        if not self.snapshot_path.exists():
            return None
        return json.loads(self.snapshot_path.read_text(encoding="utf-8"))

    def read_events(self) -> list[RuntimeBusEvent]:
        if not self.events_path.exists():
            return []
        records: list[RuntimeBusEvent] = []
        for line in self.events_path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                records.append(RuntimeBusEvent.from_dict(json.loads(line)))
        return records

    def verify_integrity(self) -> RuntimeBusIntegrity:
        previous_checksum: str | None = None
        events = self.read_events()
        for expected_index, event in enumerate(events, start=1):
            if event.schema_version != RUNTIME_EVENT_SCHEMA_VERSION:
                return RuntimeBusIntegrity(
                    ok=False,
                    events=expected_index - 1,
                    last_checksum=previous_checksum,
                    reason=f"unsupported schema at event {expected_index}",
                )
            if event.event_index != expected_index:
                return RuntimeBusIntegrity(
                    ok=False,
                    events=expected_index - 1,
                    last_checksum=previous_checksum,
                    reason=f"non-sequential event index at event {expected_index}",
                )
            if event.previous_checksum != previous_checksum:
                return RuntimeBusIntegrity(
                    ok=False,
                    events=expected_index - 1,
                    last_checksum=previous_checksum,
                    reason=f"checksum chain break at event {expected_index}",
                )
            if checksum_event(event_core(event)) != event.checksum:
                return RuntimeBusIntegrity(
                    ok=False,
                    events=expected_index - 1,
                    last_checksum=previous_checksum,
                    reason=f"checksum mismatch at event {expected_index}",
                )
            previous_checksum = event.checksum
        return RuntimeBusIntegrity(ok=True, events=len(events), last_checksum=previous_checksum)

    def verify_snapshot(self) -> dict[str, Any]:
        snapshot = self.read_snapshot()
        if snapshot is None:
            return {"present": False, "consistent": True, "reason": "no snapshot"}
        events = self.read_events()
        event_count = int(snapshot["event_count"])
        if event_count > len(events):
            return {
                "present": True,
                "consistent": False,
                "reason": "snapshot references missing tail events",
            }
        if event_count == 0:
            expected_checksum = None
        else:
            expected_checksum = events[event_count - 1].checksum
        if snapshot.get("last_checksum") != expected_checksum:
            return {
                "present": True,
                "consistent": False,
                "reason": "snapshot checksum does not match event log",
            }
        return {"present": True, "consistent": True, "reason": "ok"}

    def export_audit(self) -> dict[str, Any]:
        integrity = self.verify_integrity()
        events = [event.to_dict() for event in self.read_events()]
        snapshot = self.read_snapshot()
        return {
            "schema_version": RUNTIME_AUDIT_SCHEMA_VERSION,
            "source": "durable-runtime-bus",
            "integrity": integrity.to_dict(),
            "snapshot_integrity": self.verify_snapshot(),
            "summary": summarize_events(events),
            "snapshot": snapshot,
            "events": events,
        }


def event_core(event: RuntimeBusEvent) -> dict[str, Any]:
    core = {
        "schema_version": event.schema_version,
        "event_id": event.event_id,
        "event_index": event.event_index,
        "event_type": event.event_type,
        "as_of": event.as_of,
        "payload": event.payload,
        "previous_checksum": event.previous_checksum,
    }
    if event.trace_id:
        core["trace_id"] = event.trace_id
    return core


def checksum_event(payload: dict[str, Any]) -> str:
    body = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return "sha256:" + hashlib.sha256(body.encode("utf-8")).hexdigest()


def append_jsonl(path: Path, payload: dict[str, Any]) -> None:
    line = json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n"
    fd = os.open(path, os.O_APPEND | os.O_CREAT | os.O_WRONLY, 0o600)
    with os.fdopen(fd, "a", encoding="utf-8") as handle:
        handle.write(line)
        handle.flush()
        os.fsync(handle.fileno())


def write_json_atomic(path: Path, payload: dict[str, Any]) -> None:
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    fd = os.open(tmp_path, os.O_CREAT | os.O_TRUNC | os.O_WRONLY, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, sort_keys=True, separators=(",", ":"))
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(tmp_path, path)


def summarize_events(events: list[dict[str, Any]]) -> dict[str, Any]:
    by_type: dict[str, int] = {}
    for event in events:
        event_type = str(event["event_type"])
        by_type[event_type] = by_type.get(event_type, 0) + 1
    return {
        "events": len(events),
        "event_types": by_type,
    }
