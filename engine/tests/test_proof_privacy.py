from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def test_proof_privacy_regression_fixtures_are_refused() -> None:
    repo_root = Path(__file__).resolve().parents[2]

    result = subprocess.run(
        [sys.executable, "scripts/proof_privacy_regression.py"],
        cwd=repo_root,
        check=True,
        text=True,
        capture_output=True,
    )

    assert "2 negative fixtures refused" in result.stdout
