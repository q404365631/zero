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


def test_strategy_plugin_example_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/strategy-plugin/run.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    payload = json.loads(result.stdout)
    assert payload["mode"] == "paper"
    assert payload["plugin"]["name"] == "close-strength"
    assert payload["plugin"]["paper_only"] is True
    assert payload["proposed"] is True
    assert payload["allowed"] is True
    assert payload["fills"] == 1
    assert payload["decisions"][0]["source"] == "strategy-plugin:close-strength"


def test_network_leaderboard_example_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/network-leaderboard/build.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    payload = json.loads(result.stdout)
    assert payload["schema_version"] == "zero.network.leaderboard.v1"
    assert payload["row_count"] == 3
    assert payload["rows"][0]["rank"] == 1
    assert payload["rows"][0]["handle"] == "zero_alpha"
    assert payload["rows"][0]["verification_score"] == 70.5


def test_network_profile_page_example_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/network-profile-page/build.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    page = result.stdout
    assert "<!doctype html>" in page
    assert "<title>ZERO Local · ZERO Network</title>" in page
    assert "@zero_local" in page
    assert "Aggregate Behavior" in page
    assert "sha256:1111111111111111111111111111111111111111111111111111111111111111" in page


def test_network_leaderboard_page_example_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/network-leaderboard-page/build.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    page = result.stdout
    assert "<!doctype html>" in page
    assert "<title>ZERO Network Leaderboard</title>" in page
    assert "@zero_local" in page
    assert "91.67%" in page
    assert "sha256:1111111111111111111111111111111111111111111111111111111111111111" in page
