from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path

from zero_engine.api import PaperApi, PaperApiState
from zero_engine.intelligence import intelligence_catalog, intelligence_commercial_contract
from zero_engine.journal import DecisionJournal
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
            intelligence_public_delay_s=600,
        )
    )
    api.post(
        "/execute",
        {
            "coin": "BTC",
            "side": "buy",
            "size": 0.01,
            "idempotency_key": "intelligence-fill",
        },
        trace_id="trace-intelligence-fill",
    )
    api.post(
        "/execute",
        {
            "coin": "ETH",
            "side": "buy",
            "size": 10,
            "idempotency_key": "intelligence-reject",
        },
        trace_id="trace-intelligence-reject",
    )
    return api


def test_intelligence_snapshot_is_delayed_aggregate_product(tmp_path) -> None:
    api = seed_api(tmp_path)

    status, snapshot = api.get("/intelligence/snapshot", {})

    assert status == 200
    assert snapshot["schema_version"] == "zero.intelligence.snapshot.v1"
    assert snapshot["access"]["class"] == "public_delayed"
    assert snapshot["access"]["delay_s"] == 600
    assert snapshot["access"]["commercial_realtime"] is True
    assert snapshot["source"]["proof_hash"].startswith("sha256:")
    assert snapshot["source"]["deployment_claim_hash"].startswith("sha256:")
    assert snapshot["source"]["deployment_heartbeat_hash"].startswith("sha256:")
    assert snapshot["aggregates"]["decisions"] == 2
    assert snapshot["aggregates"]["rejections"] == 1
    assert snapshot["signals"]["journal_quality"] == "durable"
    body = json.dumps(snapshot)
    assert "intelligence-fill" not in body
    assert "trace-intelligence" not in body
    assert "BTC" not in body
    assert "ETH" not in body
    assert "api:/execute" not in body


def test_intelligence_catalog_names_commercial_metering_without_gating_runtime() -> None:
    api = PaperApi(PaperApiState(clock=lambda: FIXED_DT, started_at=FIXED_DT))

    status, catalog = api.get("/intelligence/catalog", {})

    assert status == 200
    assert catalog["schema_version"] == "zero.intelligence.catalog.v1"
    assert catalog["public"]["runtime"] == "open-source"
    assert catalog["public"]["model_gateway_status"]["schema_version"] == "zero.model_gateway.status.v1"
    assert catalog["public"]["model_gateway_health"]["schema_version"] == "zero.model_gateway.health.v1"
    assert catalog["public"]["model_gateway_audit"]["schema_version"] == "zero.model_gateway.audit.v1"
    assert "local runtime use" in catalog["commercial"]["not_metered_by"]
    assert "freshness" in catalog["commercial"]["metered_by"]
    assert catalog["hosted_api_contract"]["auth"]["scheme"] == "bearer"
    assert catalog["hosted_api_contract"]["auth"]["runtime_required"] is False
    assert catalog["hosted_api_contract"]["schema_version"] == "zero.intelligence.commercial.v1"
    assert catalog["hosted_api_contract"]["endpoint"] == "GET /intelligence/commercial"
    assert "intelligence:redistribute" in catalog["commercial"]["plans"][-1]["scopes"]


def test_intelligence_commercial_contract_is_billing_ready_public_safe(tmp_path) -> None:
    api = seed_api(tmp_path)

    status, contract = api.get("/intelligence/commercial", {})

    assert status == 200
    assert contract["schema_version"] == "zero.intelligence.commercial.v1"
    assert contract["auth"]["scheme"] == "bearer"
    assert contract["auth"]["runtime_required"] is False
    assert contract["plans"][0]["id"] == "free"
    assert contract["plans"][-1]["id"] == "enterprise"
    assert "intelligence:read:delayed" in [scope["name"] for scope in contract["scopes"]]
    assert "intelligence:redistribute" in contract["plans"][-1]["scopes"]
    assert "x-zero-ratelimit-policy" in contract["rate_limits"]["headers"]
    assert "snapshot.realtime.read" in [event["name"] for event in contract["usage_events"]]
    assert contract["privacy"]["exchange_credentials_collected"] is False
    assert contract["privacy"]["operator_secrets_included"] is False
    body = json.dumps(contract)
    assert "intelligence-fill" not in body
    assert "trace-intelligence" not in body
    assert "BTC" not in body
    assert "ETH" not in body


def test_intelligence_commercial_contract_fixture_is_fresh() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    expected = json.loads((repo_root / "contracts/intelligence/commercial.json").read_text())

    contract = intelligence_commercial_contract(
        generated_at="2026-05-01T00:00:00+00:00",
        public_delay_s=900,
    )

    assert contract == expected


def test_intelligence_catalog_fixture_is_fresh() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    expected = json.loads((repo_root / "contracts/intelligence/catalog.json").read_text())

    catalog = intelligence_catalog(
        generated_at="2026-05-01T00:00:00+00:00",
        public_delay_s=900,
    )

    assert catalog == expected


def test_intelligence_export_requires_consent_and_path(tmp_path) -> None:
    api = seed_api(tmp_path)

    no_consent_status, no_consent = api.post("/intelligence/export", {"consent": False})
    no_path_status, no_path = api.post("/intelligence/export", {"consent": True})

    assert no_consent_status == 200
    assert no_consent["ok"] is False
    assert no_consent["reason"] == "explicit consent required"
    assert no_path_status == 200
    assert no_path["ok"] is False
    assert no_path["reason"] == "ZERO_INTELLIGENCE_EXPORT_PATH is not configured"


def test_intelligence_export_writes_redacted_packet(tmp_path) -> None:
    export_path = tmp_path / "intelligence" / "snapshots.jsonl"
    api = seed_api(tmp_path)
    api.state.intelligence_export_path = str(export_path)

    status, payload = api.post("/intelligence/export", {"consent": True})

    assert status == 200
    assert payload["ok"] is True
    assert payload["exported"] is True
    assert payload["proof_hash"].startswith("sha256:")
    written = export_path.read_text()
    assert "zero.intelligence.snapshot.v1" in written
    assert "intelligence-fill" not in written
    assert "trace-intelligence" not in written
    assert "BTC" not in written
    assert "ETH" not in written
