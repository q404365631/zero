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


def test_momentum_strategy_plugin_example_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/momentum-strategy-plugin/run.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    payload = json.loads(result.stdout)
    assert payload["mode"] == "paper"
    assert payload["plugin"]["name"] == "paper-momentum"
    assert payload["plugin"]["paper_only"] is True
    assert payload["signals"][0]["symbol"] == "BTC"
    assert payload["signals"][0]["proposed"] is True
    assert payload["signals"][0]["allowed"] is True
    assert payload["signals"][0]["source"] == "strategy-plugin:paper-momentum"
    assert payload["signals"][1]["symbol"] == "ETH"
    assert payload["signals"][1]["proposed"] is False
    assert payload["signals"][1]["allowed"] is None
    assert payload["fills"] == 1
    assert payload["rejections"] == 0
    assert payload["decisions"][0]["source"] == "strategy-plugin:paper-momentum"


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
    assert "Active aggregate proof" in page
    assert "Aggregate Behavior" in page
    assert "sha256:04fc16f2e39938c8e0aa2b91648c55aa00dd64a898ddb20a5f3004d359217c48" in page


def test_network_empty_profile_example_runs_from_repo_root(tmp_path: Path) -> None:
    repo_root = Path(__file__).resolve().parents[2]
    output = tmp_path / "empty-profile.json"
    result = subprocess.run(
        [
            sys.executable,
            "examples/network-empty-profile/build.py",
            "--output",
            str(output),
        ],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    assert f"wrote {output}" in result.stdout
    payload = json.loads(output.read_text())
    assert payload["verification"]["status"] == "empty"


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


def test_network_index_page_example_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "examples/network-index-page/build.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    page = result.stdout
    assert "<!doctype html>" in page
    assert "<title>ZERO Network</title>" in page
    assert 'href="profile.html"' in page
    assert 'href="empty-profile.html"' in page
    assert 'href="stale-profile.html"' in page
    assert 'href="leaderboard.html"' in page
    assert "Public Proof Surface" in page
    assert "Empty" in page
    assert "Active" in page
    assert "Stale" in page


def test_network_pages_smoke_runs_from_repo_root() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, "scripts/network_pages_smoke.py"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )

    assert "network pages smoke passed: 5 pages" in result.stdout
