from __future__ import annotations

import json
from datetime import UTC, datetime

from zero_engine.api import PaperApi, PaperApiState
from zero_engine.journal import DecisionJournal
from zero_engine.network import PublicProfileConfig, public_profile
from zero_engine.paper import PaperEngine

FIXED_DT = datetime(2026, 5, 1, tzinfo=UTC)
FIXED_TS = FIXED_DT.timestamp()


def seed_api(tmp_path) -> PaperApi:
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    api = PaperApi(
        PaperApiState(
            engine=PaperEngine(clock=lambda: FIXED_TS, journal=journal),
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
            network_handle="zero_test",
        )
    )
    api.post(
        "/execute",
        {
            "coin": "BTC",
            "side": "buy",
            "size": 0.01,
            "idempotency_key": "network-fill",
        },
        trace_id="trace-network-fill",
    )
    api.post(
        "/execute",
        {
            "coin": "ETH",
            "side": "buy",
            "size": 10,
            "idempotency_key": "network-reject",
        },
        trace_id="trace-network-reject",
    )
    return api


def test_public_profile_is_aggregate_and_private_by_default(tmp_path) -> None:
    api = seed_api(tmp_path)

    status, profile = api.get("/network/profile", {})

    assert status == 200
    assert profile["schema_version"] == "zero.network.profile.v1"
    assert profile["profile"]["handle"] == "zero_test"
    assert profile["profile"]["publish_enabled"] is False
    assert profile["metrics"]["decisions"] == 2
    assert profile["metrics"]["fills"] == 1
    assert profile["metrics"]["rejections"] == 1
    assert profile["metrics"]["journal_durable"] is True
    assert profile["verification"]["proof_hash"].startswith("sha256:")
    body = json.dumps(profile)
    assert "network-fill" not in body
    assert "trace-network" not in body
    assert "BTC" not in body
    assert "ETH" not in body
    assert "api:/execute" not in body


def test_network_leaderboard_uses_same_public_proof(tmp_path) -> None:
    api = seed_api(tmp_path)

    profile_status, profile = api.get("/network/profile", {})
    leaderboard_status, leaderboard = api.get("/network/leaderboard", {})

    assert profile_status == 200
    assert leaderboard_status == 200
    assert leaderboard["schema_version"] == "zero.network.leaderboard.v1"
    assert leaderboard["rows"][0]["handle"] == "zero_test"
    assert leaderboard["rows"][0]["proof_hash"] == profile["verification"]["proof_hash"]
    assert leaderboard["rows"][0]["decisions"] == 2


def test_network_publish_requires_consent_and_path(tmp_path) -> None:
    api = seed_api(tmp_path)

    no_consent_status, no_consent = api.post("/network/publish", {"consent": False})
    no_path_status, no_path = api.post("/network/publish", {"consent": True})

    assert no_consent_status == 200
    assert no_consent["ok"] is False
    assert no_consent["reason"] == "explicit consent required"
    assert no_path_status == 200
    assert no_path["ok"] is False
    assert no_path["reason"] == "ZERO_NETWORK_PUBLISH_PATH is not configured"


def test_network_publish_writes_redacted_profile_packet(tmp_path) -> None:
    publish_path = tmp_path / "network" / "published.jsonl"
    api = seed_api(tmp_path)
    api.state.network_publish_path = str(publish_path)

    status, payload = api.post(
        "/network/publish",
        {"consent": True, "handle": "public_zero", "display_name": "Public ZERO"},
    )

    assert status == 200
    assert payload["ok"] is True
    assert payload["published"] is True
    assert payload["proof_hash"].startswith("sha256:")
    written = publish_path.read_text()
    assert "public_zero" in written
    assert "network-fill" not in written
    assert "trace-network" not in written
    assert "BTC" not in written
    assert "ETH" not in written


def test_public_profile_rejects_invalid_handles() -> None:
    engine = PaperEngine()

    try:
        public_profile(
            engine,
            config=PublicProfileConfig(handle="bad handle"),
            generated_at=FIXED_DT.isoformat(),
        )
    except ValueError as exc:
        assert "network handle" in str(exc)
    else:
        raise AssertionError("invalid handle should fail")
