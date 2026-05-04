from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "live_trading_evidence.py"


def test_live_trading_evidence_builds_redacted_packet(tmp_path: Path) -> None:
    fills = tmp_path / "fills.json"
    fills.write_text(
        json.dumps(
            {
                "fills": [
                    {
                        "coin": "BTC",
                        "side": "B",
                        "sz": "0.001",
                        "px": "50000",
                        "time": "2026-05-04T01:23:45Z",
                        "oid": 123456789,
                        "cloid": "0x11111111111111111111111111111111",
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    decisions = tmp_path / "decisions.jsonl"
    decisions.write_text(
        json.dumps(
            {
                "ts": "2026-05-04T01:24:00Z",
                "coin": "BTC",
                "verdict": "accepted",
                "direction": "LONG",
                "reason": "fixture accepted",
            }
        )
        + "\n",
        encoding="utf-8",
    )
    output = tmp_path / "live-evidence.json"

    subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            "build",
            "--fills",
            str(fills),
            "--decisions",
            str(decisions),
            "--output",
            str(output),
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )

    packet = json.loads(output.read_text(encoding="utf-8"))
    rendered = output.read_text(encoding="utf-8")
    assert packet["schema_version"] == "zero.live_trading_evidence.v1"
    assert packet["summary"]["live_execution_observed"] is True
    assert packet["summary"]["fill_records"] == 1
    assert packet["summary"]["decision_records"] == 1
    assert packet["privacy"]["raw_order_ids_included"] is False
    assert packet["records"]["fills"][0]["symbol_hash"].startswith("sha256:")
    assert "BTC" not in rendered
    assert "123456789" not in rendered
    assert "0x11111111111111111111111111111111" not in rendered

    verify = subprocess.run(
        [sys.executable, str(SCRIPT), "verify", str(output)],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    assert "ok=True" in verify.stdout


def test_live_trading_evidence_verifier_rejects_raw_wallet_material(tmp_path: Path) -> None:
    packet = {
        "schema_version": "zero.live_trading_evidence.v1",
        "privacy": {
            "raw_wallet_addresses_included": False,
            "raw_private_keys_included": False,
            "raw_order_ids_included": False,
            "raw_client_order_ids_included": False,
            "raw_idempotency_keys_included": False,
            "raw_trace_ids_included": False,
            "raw_exchange_payloads_included": False,
        },
        "source": {"raw_included": False},
        "summary": {"live_execution_observed": True, "fill_records": 1, "trade_records": 0},
        "records": {"fills": [{"wallet_address": "0x" + "1" * 40}]},
    }
    packet["evidence_hash"] = "sha256:bad"
    path = tmp_path / "bad.json"
    path.write_text(json.dumps(packet), encoding="utf-8")

    result = subprocess.run(
        [sys.executable, str(SCRIPT), "verify", str(path)],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 1
    assert "wallet" in result.stdout
