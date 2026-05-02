import json
from pathlib import Path

from zero_engine.bus import DurableRuntimeBus, RUNTIME_AUDIT_SCHEMA_VERSION


def test_runtime_bus_appends_checksum_chained_events_and_snapshot(tmp_path: Path) -> None:
    bus = DurableRuntimeBus(tmp_path / "bus")

    first = bus.append("runtime.health", {"status": "ok"}, as_of=123.0, trace_id="trace-1")
    second = bus.append("operator.command", {"command": "pause"}, as_of=124.0, trace_id="trace-2")
    snapshot = bus.write_snapshot(
        {"health": {"status": "paused"}},
        as_of=124.0,
        source_event_id=second.event_id,
    )

    assert first.previous_checksum is None
    assert second.previous_checksum == first.checksum
    assert snapshot["event_count"] == 2
    assert snapshot["last_checksum"] == second.checksum
    assert bus.read_snapshot() == snapshot

    audit = bus.export_audit()
    assert audit["schema_version"] == RUNTIME_AUDIT_SCHEMA_VERSION
    assert audit["integrity"]["ok"] is True
    assert audit["snapshot_integrity"]["consistent"] is True
    assert audit["summary"]["events"] == 2
    assert audit["summary"]["event_types"] == {
        "operator.command": 1,
        "runtime.health": 1,
    }


def test_runtime_bus_integrity_detects_tampering(tmp_path: Path) -> None:
    bus = DurableRuntimeBus(tmp_path / "bus")
    bus.append("runtime.health", {"status": "ok"}, as_of=123.0)
    event = json.loads(bus.events_path.read_text(encoding="utf-8").splitlines()[0])
    event["payload"]["status"] = "mutated"
    bus.events_path.write_text(json.dumps(event, sort_keys=True) + "\n", encoding="utf-8")

    integrity = bus.verify_integrity()

    assert integrity.ok is False
    assert integrity.reason == "checksum mismatch at event 1"


def test_runtime_bus_snapshot_integrity_detects_tail_deletion(tmp_path: Path) -> None:
    bus = DurableRuntimeBus(tmp_path / "bus")
    bus.append("runtime.health", {"status": "ok"}, as_of=123.0)
    second = bus.append("runtime.health", {"status": "still-ok"}, as_of=124.0)
    bus.write_snapshot({"health": {"status": "still-ok"}}, as_of=124.0, source_event_id=second.event_id)
    first_line = bus.events_path.read_text(encoding="utf-8").splitlines()[0]
    bus.events_path.write_text(first_line + "\n", encoding="utf-8")

    snapshot_integrity = bus.verify_snapshot()

    assert snapshot_integrity["consistent"] is False
    assert snapshot_integrity["reason"] == "snapshot references missing tail events"
