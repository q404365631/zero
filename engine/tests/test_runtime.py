import json
import subprocess
import sys
from pathlib import Path

from zero_engine.runtime import CYCLE_SCHEMA_VERSION, RuntimeLoop, load_runtime_config


def scenario_path() -> Path:
    return Path(__file__).resolve().parents[2] / "examples" / "paper-trading" / "scenario.json"


def test_runtime_loop_runs_one_complete_ooda_cycle(tmp_path: Path) -> None:
    decision_journal = tmp_path / "decisions.jsonl"
    cycle_journal = tmp_path / "cycles.jsonl"
    runtime_bus = tmp_path / "runtime-bus"
    loop = RuntimeLoop.from_config(
        load_runtime_config(
            scenario_path=scenario_path(),
            decision_journal_path=decision_journal,
            cycle_journal_path=cycle_journal,
            runtime_bus_path=runtime_bus,
            interval_s=0,
        )
    )

    record = loop.run_once()

    assert record.schema_version == CYCLE_SCHEMA_VERSION
    assert record.cycle_id == 1
    assert record.observe["phase"] == "observe"
    assert record.orient["phase"] == "orient"
    assert record.decide["phase"] == "decide"
    assert record.act["phase"] == "act"
    assert record.learn["phase"] == "learn"
    assert record.act["accepted"] is True
    assert loop.engine.fills
    assert decision_journal.exists()
    assert cycle_journal.exists()
    assert (runtime_bus / "events.jsonl").exists()
    assert (runtime_bus / "state-snapshot.json").exists()
    assert loop.bus is not None
    assert loop.bus.verify_integrity().ok is True


def test_runtime_loop_recovers_and_continues_from_decision_journal(tmp_path: Path) -> None:
    decision_journal = tmp_path / "decisions.jsonl"
    cycle_journal = tmp_path / "cycles.jsonl"
    runtime_bus = tmp_path / "runtime-bus"
    config = load_runtime_config(
        scenario_path=scenario_path(),
        decision_journal_path=decision_journal,
        cycle_journal_path=cycle_journal,
        runtime_bus_path=runtime_bus,
        interval_s=0,
    )
    first_loop = RuntimeLoop.from_config(config)
    first = first_loop.run_once()

    second_loop = RuntimeLoop.from_config(config)
    second = second_loop.run_once()

    assert first.cycle_id == 1
    assert second.cycle_id == 2
    assert second.observe["recovery"]["status"] == "recovered"
    assert second.observe["decisions_seen"] == 1
    assert second.act["decision"]["symbol"] == "ETH"
    assert len(decision_journal.read_text().splitlines()) == 2
    assert len(cycle_journal.read_text().splitlines()) == 2
    assert second_loop.bus is not None
    audit = second_loop.bus.export_audit()
    assert audit["integrity"]["ok"] is True
    assert audit["summary"]["event_types"]["runtime.cycle"] == 2
    assert audit["summary"]["event_types"]["decision.record"] == 2
    assert audit["snapshot"]["payload"]["health"]["last_cycle_id"] == 2
    assert audit["events"][0]["event_type"] == "runtime.cycle"


def test_runtime_bus_audit_reconstructs_session_from_disk_only(tmp_path: Path) -> None:
    decision_journal = tmp_path / "decisions.jsonl"
    runtime_bus = tmp_path / "runtime-bus"
    loop = RuntimeLoop.from_config(
        load_runtime_config(
            scenario_path=scenario_path(),
            decision_journal_path=decision_journal,
            runtime_bus_path=runtime_bus,
            interval_s=0,
        )
    )

    first = loop.run_once()
    second = loop.run_once()

    disk_bus = RuntimeLoop.from_config(
        load_runtime_config(
            scenario_path=scenario_path(),
            decision_journal_path=decision_journal,
            runtime_bus_path=runtime_bus,
            interval_s=0,
        )
    ).bus
    assert disk_bus is not None
    audit = disk_bus.export_audit()

    assert first.cycle_id == 1
    assert second.cycle_id == 2
    assert audit["integrity"]["ok"] is True
    assert audit["summary"]["event_types"]["fill.record"] == 1
    assert audit["summary"]["event_types"]["rejection.record"] == 1
    assert audit["snapshot"]["payload"]["health"]["decisions"] == 2
    assert audit["snapshot"]["payload"]["health"]["fills"] == 1
    assert audit["snapshot"]["payload"]["health"]["rejections"] == 1


def test_zero_engine_run_cli_emits_cycle_record(tmp_path: Path) -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "zero_engine.runtime",
            "--scenario",
            str(scenario_path()),
            "--journal",
            str(tmp_path / "decisions.jsonl"),
            "--cycle-journal",
            str(tmp_path / "cycles.jsonl"),
            "--runtime-bus",
            str(tmp_path / "runtime-bus"),
            "--once",
            "--interval",
            "0",
        ],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    payload = json.loads(result.stdout)
    assert payload["schema_version"] == CYCLE_SCHEMA_VERSION
    assert payload["observe"]["phase"] == "observe"
    assert payload["act"]["accepted"] is True
    assert (tmp_path / "runtime-bus" / "events.jsonl").exists()
