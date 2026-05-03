from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path

from zero_engine import research

ROOT = Path(__file__).resolve().parents[2]
FIXED_DT = datetime(2026, 5, 1, tzinfo=UTC)


def test_research_report_runs_full_public_command_chain() -> None:
    report = research.build_report(ROOT, now=FIXED_DT)

    assert report["schema_version"] == "zero.research.report.v1"
    assert report["mode"] == "paper-only"
    assert report["paper_only"] is True
    assert report["applies_code_changes"] is False
    assert report["pushes_to_remote"] is False
    assert report["claims_live_pnl"] is False
    assert report["commands"] == [
        "hunt",
        "edge",
        "convergence",
        "thesis",
        "score",
        "meta",
        "sharpen",
    ]
    assert report["reports"]["hunt"]["candidate_count"] == 3
    assert report["reports"]["edge"]["sample_size"] == 2
    assert report["reports"]["edge"]["minimum_sample_met"] is False
    assert report["reports"]["convergence"]["status"] == "insufficient-public-sample"
    assert report["reports"]["sharpen"]["proposals"]


def test_research_snapshot_is_public_safe() -> None:
    snapshot = research.snapshot_from_fixture(ROOT, now=FIXED_DT)
    serialized = json.dumps(snapshot).lower()

    assert snapshot["schema_version"] == "zero.research.snapshot.v1"
    assert snapshot["paper_only"] is True
    assert snapshot["summary"]["sample_size"] == 2
    assert snapshot["privacy"]["contains_wallet_material"] is False
    assert "notional_usd" not in serialized
    assert "wallet_address" not in serialized
    assert "secret_material" in serialized
    assert "private_key" not in serialized
    assert "exchange_order_id" not in serialized


def test_research_installed_package_fallback_is_read_only(tmp_path: Path) -> None:
    snapshot = research.snapshot_from_fixture(tmp_path, now=FIXED_DT)

    assert snapshot["schema_version"] == "zero.research.snapshot.v1"
    assert snapshot["available"] is False
    assert snapshot["paper_only"] is True
    assert snapshot["applies_code_changes"] is False
    assert snapshot["pushes_to_remote"] is False
    assert snapshot["claims_live_pnl"] is False


def test_research_cli_writes_report_and_status(tmp_path: Path, capsys) -> None:
    output = tmp_path / "research.json"

    assert (
        research.main(
            [
                "run",
                "--repo-root",
                str(ROOT),
                "--output",
                str(output),
                "--now",
                "2026-05-01T00:00:00Z",
            ]
        )
        == 0
    )
    run_status = json.loads(capsys.readouterr().out)
    assert output.is_file()
    assert run_status["schema_version"] == "zero.research.status.v1"
    assert run_status["paper_only"] is True

    assert (
        research.main(
            [
                "status",
                "--report",
                str(output),
                "--now",
                "2026-05-01T00:00:00Z",
            ]
        )
        == 0
    )
    status = json.loads(capsys.readouterr().out)
    assert status["summary"]["sample_size"] == 2
