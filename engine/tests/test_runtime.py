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
    loop = RuntimeLoop.from_config(
        load_runtime_config(
            scenario_path=scenario_path(),
            decision_journal_path=decision_journal,
            cycle_journal_path=cycle_journal,
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


def test_runtime_loop_recovers_and_continues_from_decision_journal(tmp_path: Path) -> None:
    decision_journal = tmp_path / "decisions.jsonl"
    cycle_journal = tmp_path / "cycles.jsonl"
    config = load_runtime_config(
        scenario_path=scenario_path(),
        decision_journal_path=decision_journal,
        cycle_journal_path=cycle_journal,
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
