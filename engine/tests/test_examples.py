import json
import subprocess
import sys
from pathlib import Path


def test_paper_trading_example_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/paper-trading/run.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    payload = json.loads(result.stdout)
    assert payload["mode"] == "paper"
    assert payload["scenario"] == "paper-launch-smoke"
    assert payload["fills"] == 2
    assert payload["rejections"] == 2
    assert payload["market"]["BTC"]["last"] == 40500
    assert payload["decisions"][0]["source"] == "scenario:paper-launch-smoke"
    assert "as_of" in payload["decisions"][0]
