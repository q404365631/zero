#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "openapi" / "zero-paper-api.v1.yaml"

REQUIRED_PATHS = {
    "/",
    "/health",
    "/v2/status",
    "/positions",
    "/risk",
    "/brief",
    "/regime",
    "/evaluate/{coin}",
    "/pulse",
    "/approaching",
    "/rejections",
    "/journal",
    "/metrics",
    "/audit/export",
    "/deployment/claim",
    "/deployment/heartbeat",
    "/network/profile",
    "/network/leaderboard",
    "/network/publish",
    "/intelligence/snapshot",
    "/intelligence/catalog",
    "/intelligence/model-gateway",
    "/intelligence/export",
    "/hl/account",
    "/hl/reconcile",
    "/hl/status",
    "/immune",
    "/live/cockpit",
    "/live/certification",
    "/market/quote",
    "/operator/state",
    "/operator/context",
    "/operator/events",
    "/execute",
    "/auto/toggle",
    "/live/preflight",
    "/live/heartbeat",
    "/live/pause",
    "/live/resume",
    "/live/kill",
    "/live/flatten",
}

REQUIRED_SCHEMAS = {
    "RootResponse",
    "HealthResponse",
    "V2StatusResponse",
    "PositionsResponse",
    "RiskResponse",
    "BriefResponse",
    "RegimeResponse",
    "EvaluateResponse",
    "PulseResponse",
    "ApproachingResponse",
    "RejectionsResponse",
    "JournalResponse",
    "MetricsResponse",
    "AuditExportResponse",
    "DeploymentClaimResponse",
    "DeploymentHeartbeatResponse",
    "NetworkProfileResponse",
    "NetworkLeaderboardResponse",
    "IntelligenceSnapshotResponse",
    "IntelligenceCatalogResponse",
    "ModelGatewayStatusResponse",
    "HyperliquidStatusResponse",
    "HyperliquidAccountResponse",
    "HyperliquidReconciliationResponse",
    "ImmuneResponse",
    "LiveCockpitResponse",
    "LiveCertificationResponse",
    "MarketQuoteResponse",
    "OperatorStateResponse",
    "OperatorContextResponse",
    "ExecuteRequest",
    "ExecuteResponse",
    "LivePreflightResponse",
    "LiveControlResponse",
    "LiveFlattenResponse",
    "ErrorResponse",
}

FIXTURE_REQUIREMENTS = {
    "contracts/paper-api/v2_status.json": {
        "schema": "V2StatusResponse",
        "keys": {
            "confidence",
            "market",
            "positions",
            "today",
            "approaching",
            "blind_spots",
            "alert",
            "recovery",
            "ts",
        },
    },
    "contracts/paper-api/positions.json": {
        "schema": "PositionsResponse",
        "keys": {"positions", "count", "account_value", "total_unrealized_pnl"},
    },
    "contracts/paper-api/risk.json": {
        "schema": "RiskResponse",
        "keys": {"account_value", "updated_at", "daily_pnl_usd", "open_count", "drawdown_pct"},
    },
    "contracts/paper-api/brief.json": {
        "schema": "BriefResponse",
        "keys": {"timestamp", "fear_greed", "open_positions", "positions", "last_cycle"},
    },
    "contracts/paper-api/rejections.json": {
        "schema": "RejectionsResponse",
        "keys": {"rejections"},
    },
    "contracts/paper-api/execute_accepted.json": {
        "schema": "ExecuteResponse",
        "keys": {"accepted", "simulated", "fill_id", "coin", "side", "size", "reason"},
    },
    "contracts/paper-api/execute_rejected.json": {
        "schema": "ExecuteResponse",
        "keys": {"accepted", "simulated", "fill_id", "coin", "side", "size", "reason"},
    },
    "contracts/intelligence/snapshot.json": {
        "schema": "IntelligenceSnapshotResponse",
        "keys": {
            "schema_version",
            "generated_at",
            "access",
            "source",
            "signals",
            "aggregates",
            "commercial_unlocks",
            "privacy",
        },
    },
    "contracts/intelligence/catalog.json": {
        "schema": "IntelligenceCatalogResponse",
        "keys": {
            "schema_version",
            "generated_at",
            "positioning",
            "public",
            "commercial",
            "hosted_api_contract",
            "data_rules",
        },
    },
    "contracts/intelligence/model_gateway.json": {
        "schema": "ModelGatewayStatusResponse",
        "keys": {
            "schema_version",
            "generated_at",
            "mode",
            "default_provider",
            "routing",
            "providers",
            "usage",
            "privacy",
        },
    },
    "contracts/deployment/claim.json": {
        "schema": "DeploymentClaimResponse",
        "keys": {
            "schema_version",
            "generated_at",
            "deployment",
            "operator",
            "runtime",
            "evidence",
            "privacy",
            "claim_hash",
            "signature",
        },
    },
    "contracts/deployment/heartbeat.json": {
        "schema": "DeploymentHeartbeatResponse",
        "keys": {
            "schema_version",
            "generated_at",
            "deployment",
            "deployment_claim_hash",
            "operator",
            "runtime",
            "liveness",
            "privacy",
            "heartbeat_hash",
            "signature",
        },
    },
}


def fail(message: str) -> None:
    print(f"openapi contract check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def extract_paths(spec_text: str) -> set[str]:
    return {match.group(1) for match in re.finditer(r"^  (/[^:]*):$", spec_text, re.MULTILINE)}


def extract_schemas(spec_text: str) -> set[str]:
    schemas: set[str] = set()
    in_schemas = False
    for line in spec_text.splitlines():
        if line == "  schemas:":
            in_schemas = True
            continue
        if in_schemas and line.startswith("  ") and not line.startswith("    "):
            break
        if in_schemas:
            match = re.match(r"^    ([A-Za-z][A-Za-z0-9]*):$", line)
            if match:
                schemas.add(match.group(1))
    return schemas


def main() -> None:
    spec_text = SPEC.read_text(encoding="utf-8")
    if "openapi: 3.1.0" not in spec_text:
        fail("spec must declare OpenAPI 3.1.0")

    paths = extract_paths(spec_text)
    missing_paths = sorted(REQUIRED_PATHS - paths)
    if missing_paths:
        fail(f"missing paths: {', '.join(missing_paths)}")

    schemas = extract_schemas(spec_text)
    missing_schemas = sorted(REQUIRED_SCHEMAS - schemas)
    if missing_schemas:
        fail(f"missing schemas: {', '.join(missing_schemas)}")

    missing_refs = sorted(
        name for name in REQUIRED_SCHEMAS if f"#/components/schemas/{name}" not in spec_text
    )
    if missing_refs:
        fail(f"schemas are not referenced by operations or responses: {', '.join(missing_refs)}")

    operation_ids = re.findall(r"^\s+operationId:\s+([A-Za-z0-9_]+)$", spec_text, re.MULTILINE)
    if len(operation_ids) != len(set(operation_ids)):
        fail("operationId values must be unique")
    if len(operation_ids) < len(REQUIRED_PATHS):
        fail("every required path should expose an operationId")

    for relative_path, requirement in FIXTURE_REQUIREMENTS.items():
        fixture_path = ROOT / relative_path
        if not fixture_path.exists():
            fail(f"missing fixture {relative_path}")
        payload = json.loads(fixture_path.read_text(encoding="utf-8"))
        missing_keys = sorted(requirement["keys"] - set(payload.keys()))
        if missing_keys:
            fail(f"{relative_path} missing keys: {', '.join(missing_keys)}")
        schema = requirement["schema"]
        if f"#/components/schemas/{schema}" not in spec_text:
            fail(f"{relative_path} points at unreferenced schema {schema}")

    print(
        "openapi contract check passed: "
        f"{len(paths)} paths, {len(schemas)} schemas, {len(FIXTURE_REQUIREMENTS)} fixtures"
    )


if __name__ == "__main__":
    main()
