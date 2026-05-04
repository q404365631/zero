from __future__ import annotations

import json
import os
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

import pytest
from zero_engine.api import PaperApi, PaperApiState
from zero_engine.deployment import DeploymentIdentityConfig, deployment_claim, deployment_heartbeat
from zero_engine.journal import DecisionJournal
from zero_engine.models import OrderIntent, Side
from zero_engine.network import (
    PublicProfileConfig,
    expected_profile_proof_hash,
    ingest_public_profiles,
    load_public_profiles,
    network_profile_freshness,
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
    assert profile["verification"]["deployment_heartbeat_hash"].startswith("sha256:")
    assert (
        profile["deployment_heartbeat"]["heartbeat_hash"]
        == profile["verification"]["deployment_heartbeat_hash"]
    )
    assert (
        profile["deployment_heartbeat"]["deployment_claim_hash"]
        == profile["deployment_claim"]["claim_hash"]
    )
    assert profile["deployment_claim"]["signature"]["status"] == "unsigned_local"
    body = json.dumps(profile)
    assert "network-fill" not in body
    assert "trace-network" not in body
    assert "BTC" not in body
    assert "ETH" not in body
    assert "api:/execute" not in body


def test_expected_profile_proof_hash_matches_runtime_profile(tmp_path) -> None:
    profile = seed_api(tmp_path).network_profile()

    assert expected_profile_proof_hash(profile) == profile["verification"]["proof_hash"]


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


def test_deployment_heartbeat_is_public_safe_and_bound_to_claim(tmp_path) -> None:
    api = seed_api(tmp_path)
    claim_status, claim = api.get("/deployment/claim", {})

    status, heartbeat = api.get("/deployment/heartbeat", {})

    assert claim_status == 200
    assert status == 200
    assert heartbeat["schema_version"] == "zero.deployment.heartbeat.v1"
    assert heartbeat["deployment_claim_hash"] == claim["claim_hash"]
    assert heartbeat["heartbeat_hash"].startswith("sha256:")
    assert heartbeat["signature"]["status"] == "unsigned_local"
    assert heartbeat["signature"]["signed_heartbeat_hash"] == heartbeat["heartbeat_hash"]
    assert heartbeat["liveness"]["status"] == "paper_only"
    assert heartbeat["liveness"]["live_executor_configured"] is False
    body = json.dumps(heartbeat)
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


def test_deployment_heartbeat_accepts_external_signature_metadata() -> None:
    heartbeat = deployment_heartbeat(
        config=DeploymentIdentityConfig(
            deployment_id="railway-paper-1",
            deployment_kind="railway",
            environment="paper",
            owner="zero-team",
            heartbeat_public_key="heartbeat-public",
            heartbeat_signature="heartbeat-signature",
            heartbeat_signer="ci",
        ),
        generated_at=FIXED_DT.isoformat(),
        deployment_claim_hash="sha256:claim",
        operator_context={"handle": "zero_team", "role": "owner", "scope": "paper"},
        runtime={"mode": "paper", "market_source": "paper:static"},
        liveness={"status": "fresh", "live_executor_configured": True},
    )

    assert heartbeat["heartbeat_hash"].startswith("sha256:")
    assert heartbeat["signature"]["status"] == "signed_external"
    assert heartbeat["signature"]["signed_heartbeat_hash"] == heartbeat["heartbeat_hash"]


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
    assert (
        leaderboard["rows"][0]["deployment_heartbeat_hash"]
        == profile["verification"]["deployment_heartbeat_hash"]
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
    assert "deployment_heartbeat" in written
    assert "network-fill" not in written
    assert "trace-network" not in written
    assert "BTC" not in written
    assert "ETH" not in written


def test_network_ingestion_accepts_consented_runtime_profile(tmp_path) -> None:
    profile = seed_api(tmp_path).network_profile()
    profile["profile"]["publish_enabled"] = True

    ingestion = ingest_public_profiles([profile], generated_at=FIXED_DT.isoformat())

    assert ingestion["schema_version"] == "zero.network.ingestion.v1"
    assert ingestion["summary"] == {
        "submitted": 1,
        "accepted": 1,
        "refused": 0,
        "duplicates": 0,
        "leaderboard_rows": 1,
    }
    assert ingestion["records"][0]["decision"] == "accepted"
    assert ingestion["records"][0]["trust_tier"] == "unsigned_local"
    assert ingestion["records"][0]["leaderboard_eligible"] is True
    assert ingestion["records"][0]["anti_gaming_score"] > 0
    assert ingestion["leaderboard"]["rows"][0]["handle"] == "zero_test"
    body = json.dumps(ingestion)
    assert "network-fill" not in body
    assert "trace-network" not in body
    assert "BTC" not in body
    assert "ETH" not in body


def test_network_ingestion_refuses_missing_consent_and_proof_mismatch(tmp_path) -> None:
    profile = seed_api(tmp_path).network_profile()
    profile["verification"]["proof_hash"] = "sha256:" + ("a" * 64)
    profile["leaderboard_row"]["proof_hash"] = "sha256:" + ("a" * 64)

    ingestion = ingest_public_profiles([profile], generated_at=FIXED_DT.isoformat())

    assert ingestion["summary"]["accepted"] == 0
    assert ingestion["summary"]["refused"] == 1
    record = ingestion["records"][0]
    assert record["decision"] == "refused"
    assert "missing_consent" in record["risk_flags"]
    assert "proof_hash_mismatch" in record["risk_flags"]
    assert ingestion["leaderboard"]["row_count"] == 0


def test_network_ingestion_refuses_duplicate_accepted_packets(tmp_path) -> None:
    profile = seed_api(tmp_path).network_profile()
    profile["profile"]["publish_enabled"] = True

    ingestion = ingest_public_profiles([profile, profile], generated_at=FIXED_DT.isoformat())

    assert ingestion["summary"]["accepted"] == 1
    assert ingestion["summary"]["refused"] == 1
    assert ingestion["summary"]["duplicates"] == 1
    assert ingestion["records"][1]["decision"] == "refused"
    assert "duplicate_handle" in ingestion["records"][1]["risk_flags"]
    assert "duplicate_proof_hash" in ingestion["records"][1]["risk_flags"]


def test_network_ingestion_api_accepts_current_profile_packet(tmp_path) -> None:
    api = seed_api(tmp_path)
    profile = api.network_profile()
    profile["profile"]["publish_enabled"] = True

    status, ingestion = api.post("/network/ingest", {"profiles": [profile]})

    assert status == 200
    assert ingestion["schema_version"] == "zero.network.ingestion.v1"
    assert ingestion["summary"]["accepted"] == 1
    assert ingestion["leaderboard"]["rows"][0]["handle"] == "zero_test"


def test_network_profile_freshness_separates_valid_proof_from_stale_status() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    profile = json.loads((repo_root / "docs/proof/network/profile.json").read_text())

    freshness = network_profile_freshness(
        profile,
        evaluated_at="2026-05-04T00:00:00+00:00",
    )

    assert freshness["schema_version"] == "zero.network.profile_freshness.v1"
    assert freshness["proof"]["status"] == "valid"
    assert freshness["freshness"]["status"] == "stale"
    assert freshness["claim_boundary"]["active_operator_status_asserted"] is False
    body = json.dumps(freshness)
    assert "wallet_address" not in body
    assert "exchange_order_id" not in body
    assert "network-fill" not in body
    assert "trace-network" not in body


def test_network_stale_profile_example_fixture_is_public_safe_and_fresh() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    fixture = json.loads(
        (repo_root / "examples/network-stale-profile/stale-profile.json").read_text()
    )

    assert fixture["schema_version"] == "zero.network.stale_profile_fixture.v1"
    assert fixture["proof"]["status"] == "valid"
    assert fixture["freshness"]["status"] == "stale"
    assert fixture["claim_boundary"]["active_operator_status_asserted"] is False
    assert expected_profile_proof_hash(fixture["profile"]) == fixture["proof"]["proof_hash"]
    body = json.dumps(fixture)
    assert "wallet_address" not in body
    assert "exchange_order_id" not in body
    assert "network-fill" not in body
    assert "trace-network" not in body


def test_network_stale_profile_example_fixture_is_fresh(tmp_path) -> None:
    repo_root = Path(__file__).resolve().parents[2]
    output = tmp_path / "stale-profile.json"

    subprocess.run(
        [
            sys.executable,
            "examples/network-stale-profile/build.py",
            "--output",
            str(output),
        ],
        cwd=repo_root,
        check=True,
        text=True,
        capture_output=True,
        env={**os.environ, "PYTHONPATH": str(repo_root / "engine/src")},
    )

    expected = (repo_root / "examples/network-stale-profile/stale-profile.json").read_text()
    assert output.read_text() == expected


def test_network_ingestion_contract_is_fresh() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    expected = json.loads((repo_root / "contracts/network/ingestion.json").read_text())
    engine = PaperEngine(clock=lambda: FIXED_TS)
    engine.submit(OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9))
    engine.submit(OrderIntent("ETH", Side.BUY, quantity=1.0, price=3_000, confidence=0.9))
    profile = public_profile(
        engine,
        config=PublicProfileConfig(
            handle="zero_local",
            display_name="ZERO Local",
            publish_enabled=True,
        ),
        generated_at="2026-05-01T00:00:00+00:00",
    )

    payload = ingest_public_profiles(
        [profile],
        generated_at="2026-05-01T00:00:00+00:00",
    )

    assert payload == expected


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
    second["verification"]["deployment_heartbeat_hash"] = "sha256:heartbeat-alpha"
    second["leaderboard_row"]["deployment_heartbeat_hash"] = "sha256:heartbeat-alpha"

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
