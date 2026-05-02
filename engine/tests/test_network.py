from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path

import pytest
from zero_engine.api import PaperApi, PaperApiState
from zero_engine.deployment import DeploymentIdentityConfig, deployment_claim
from zero_engine.journal import DecisionJournal
from zero_engine.network import (
    PublicProfileConfig,
    load_public_profiles,
    public_leaderboard,
    public_leaderboard_page,
    public_network_index_page,
    public_profile,
    public_profile_page,
)
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
    assert profile["verification"]["deployment_claim_hash"].startswith("sha256:")
    assert (
        profile["deployment_claim"]["claim_hash"]
        == profile["verification"]["deployment_claim_hash"]
    )
    assert profile["deployment_claim"]["signature"]["status"] == "unsigned_local"
    body = json.dumps(profile)
    assert "network-fill" not in body
    assert "trace-network" not in body
    assert "BTC" not in body
    assert "ETH" not in body
    assert "api:/execute" not in body


def test_deployment_claim_is_public_safe_and_signature_ready(tmp_path) -> None:
    api = seed_api(tmp_path)

    status, claim = api.get("/deployment/claim", {})

    assert status == 200
    assert claim["schema_version"] == "zero.deployment.claim.v1"
    assert claim["deployment"]["deployment_id"] == "local-paper"
    assert claim["operator"]["handle"] == "local-operator"
    assert claim["runtime"]["mode"] == "paper"
    assert claim["evidence"]["decisions"] == 2
    assert claim["claim_hash"].startswith("sha256:")
    assert claim["signature"]["status"] == "unsigned_local"
    assert claim["signature"]["signed_claim_hash"] == claim["claim_hash"]
    body = json.dumps(claim)
    assert "network-fill" not in body
    assert "trace-network" not in body
    assert "BTC" not in body
    assert "ETH" not in body
    assert "api:/execute" not in body


def test_deployment_claim_accepts_external_signature_metadata() -> None:
    claim = deployment_claim(
        config=DeploymentIdentityConfig(
            deployment_id="railway-paper-1",
            deployment_kind="railway",
            environment="paper",
            owner="zero-team",
            public_key="ed25519-public",
            signature="ed25519-signature",
            signer="ci",
        ),
        generated_at=FIXED_DT.isoformat(),
        operator_context={"handle": "zero_team", "role": "owner", "scope": "paper"},
        runtime={"mode": "paper", "market_source": "paper:static"},
        evidence={"decisions": 0},
    )

    assert claim["claim_hash"].startswith("sha256:")
    assert claim["signature"]["status"] == "signed_external"
    assert claim["signature"]["signed_claim_hash"] == claim["claim_hash"]


def test_network_leaderboard_uses_same_public_proof(tmp_path) -> None:
    api = seed_api(tmp_path)

    profile_status, profile = api.get("/network/profile", {})
    leaderboard_status, leaderboard = api.get("/network/leaderboard", {})

    assert profile_status == 200
    assert leaderboard_status == 200
    assert leaderboard["schema_version"] == "zero.network.leaderboard.v1"
    assert leaderboard["row_count"] == 1
    assert leaderboard["rows"][0]["rank"] == 1
    assert leaderboard["rows"][0]["handle"] == "zero_test"
    assert leaderboard["rows"][0]["proof_hash"] == profile["verification"]["proof_hash"]
    assert (
        leaderboard["rows"][0]["deployment_claim_hash"]
        == profile["verification"]["deployment_claim_hash"]
    )
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
    assert "deployment_claim" in written
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


def test_public_leaderboard_ranks_redacted_profiles(tmp_path) -> None:
    first = seed_api(tmp_path / "first").network_profile()
    second = seed_api(tmp_path / "second").network_profile()
    second["profile"]["handle"] = "zero_alpha"
    second["profile"]["display_name"] = "ZERO Alpha"
    second["leaderboard_row"]["handle"] = "zero_alpha"
    second["leaderboard_row"]["decisions"] = 8
    second["leaderboard_row"]["verification_score"] = 24
    second["verification"]["proof_hash"] = "sha256:alpha"
    second["leaderboard_row"]["proof_hash"] = "sha256:alpha"
    second["verification"]["deployment_claim_hash"] = "sha256:claim-alpha"
    second["leaderboard_row"]["deployment_claim_hash"] = "sha256:claim-alpha"

    leaderboard = public_leaderboard(
        [first, second],
        generated_at=FIXED_DT.isoformat(),
    )

    assert leaderboard["schema_version"] == "zero.network.leaderboard.v1"
    assert leaderboard["row_count"] == 2
    assert leaderboard["rows"][0]["rank"] == 1
    assert leaderboard["rows"][0]["handle"] == "zero_alpha"
    assert leaderboard["rows"][1]["rank"] == 2
    assert leaderboard["rows"][1]["handle"] == "zero_test"


def test_public_leaderboard_rejects_unsafe_profile(tmp_path) -> None:
    profile = seed_api(tmp_path).network_profile()
    profile["debug"] = {"idempotency_key": "must-not-leak"}

    with pytest.raises(ValueError, match="forbidden token"):
        public_leaderboard([profile], generated_at=FIXED_DT.isoformat())


def test_network_leaderboard_example_profiles_load_from_jsonl() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    profiles = load_public_profiles(repo_root / "examples/network-leaderboard/profiles.jsonl")
    leaderboard = public_leaderboard(profiles, generated_at=FIXED_DT.isoformat())

    assert leaderboard["row_count"] == 3
    assert leaderboard["rows"][0]["rank"] == 1
    assert leaderboard["rows"][0]["handle"] == "zero_alpha"
    assert leaderboard["rows"][0]["proof_hash"].startswith("sha256:")


def test_public_leaderboard_page_renders_public_rows_only() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    profiles = load_public_profiles(repo_root / "examples/network-leaderboard/profiles.jsonl")
    leaderboard = public_leaderboard(profiles, generated_at=FIXED_DT.isoformat())

    page = public_leaderboard_page(leaderboard, generated_at=FIXED_DT.isoformat())

    assert "<!doctype html>" in page
    assert "ZERO Network Leaderboard" in page
    assert "@zero_alpha" in page
    assert "70.5" in page
    assert leaderboard["rows"][0]["proof_hash"] in page
    assert "network-fill" not in page
    assert "trace-network" not in page
    assert "BTC" not in page
    assert "ETH" not in page


def test_public_leaderboard_page_escapes_row_text() -> None:
    leaderboard = json.loads(
        (Path(__file__).resolve().parents[2] / "contracts/network/leaderboard.json").read_text()
    )
    leaderboard["rows"][0]["display_name"] = "<script>alert(1)</script>"

    page = public_leaderboard_page(leaderboard, generated_at=FIXED_DT.isoformat())

    assert "<script>" not in page
    assert "&lt;script&gt;alert(1)&lt;/script&gt;" in page


def test_public_network_index_page_links_contract_pages_only() -> None:
    page = public_network_index_page(generated_at=FIXED_DT.isoformat())

    assert "<!doctype html>" in page
    assert "<title>ZERO Network</title>" in page
    assert 'href="profile.html"' in page
    assert 'href="leaderboard.html"' in page
    assert "Opt-in aggregate behavior" in page
    assert "network-fill" not in page
    assert "trace-network" not in page
    assert "BTC" not in page
    assert "ETH" not in page


def test_public_network_index_page_rejects_remote_links() -> None:
    with pytest.raises(ValueError, match="local contract path"):
        public_network_index_page(
            generated_at=FIXED_DT.isoformat(),
            profile_href="https://example.com/profile.html",
        )


def test_public_profile_page_renders_aggregate_html_only(tmp_path) -> None:
    profile = seed_api(tmp_path).network_profile()

    page = public_profile_page(profile, generated_at=FIXED_DT.isoformat())

    assert "<!doctype html>" in page
    assert "ZERO Network" in page
    assert "@zero_test" in page
    assert "Aggregate Behavior" in page
    assert profile["verification"]["proof_hash"] in page
    assert "network-fill" not in page
    assert "trace-network" not in page
    assert "BTC" not in page
    assert "ETH" not in page


def test_public_profile_page_escapes_profile_text(tmp_path) -> None:
    profile = seed_api(tmp_path).network_profile()
    profile["profile"]["display_name"] = "<script>alert(1)</script>"

    page = public_profile_page(profile, generated_at=FIXED_DT.isoformat())

    assert "<script>" not in page
    assert "&lt;script&gt;alert(1)&lt;/script&gt;" in page


def test_network_profile_page_contract_is_fresh() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    profile = json.loads((repo_root / "contracts/network/profile.json").read_text())
    expected = (repo_root / "contracts/network/profile.html").read_text()

    page = public_profile_page(profile, generated_at=FIXED_DT.isoformat())

    assert page == expected


def test_network_leaderboard_page_contract_is_fresh() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    leaderboard = json.loads((repo_root / "contracts/network/leaderboard.json").read_text())
    expected = (repo_root / "contracts/network/leaderboard.html").read_text()

    page = public_leaderboard_page(leaderboard, generated_at=FIXED_DT.isoformat())

    assert page == expected


def test_network_index_page_contract_is_fresh() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    expected = (repo_root / "contracts/network/index.html").read_text()

    page = public_network_index_page(generated_at=FIXED_DT.isoformat())

    assert page == expected
