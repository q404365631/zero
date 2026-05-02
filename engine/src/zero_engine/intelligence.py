from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from zero_engine.network import assert_public_profile_safe

COMMERCIAL_CONTRACT_SCHEMA_VERSION = "zero.intelligence.commercial.v1"


@dataclass(frozen=True)
class IntelligenceConfig:
    public_delay_s: int = 900
    export_path: str | None = None

    def __post_init__(self) -> None:
        if self.public_delay_s < 0:
            raise ValueError("intelligence public delay must be non-negative")


def intelligence_snapshot(
    profile: dict[str, Any],
    *,
    generated_at: str,
    config: IntelligenceConfig | None = None,
) -> dict[str, Any]:
    cfg = config or IntelligenceConfig()
    metrics = profile.get("metrics", {})
    proof_hash = profile.get("verification", {}).get("proof_hash", "")
    deployment_claim_hash = profile.get("verification", {}).get("deployment_claim_hash", "")
    deployment_heartbeat_hash = profile.get("verification", {}).get("deployment_heartbeat_hash", "")
    snapshot = {
        "schema_version": "zero.intelligence.snapshot.v1",
        "generated_at": generated_at,
        "access": {
            "class": "public_delayed",
            "delay_s": cfg.public_delay_s,
            "commercial_realtime": True,
            "commercial_history": True,
            "commercial_redistribution": True,
        },
        "source": {
            "schema_version": profile.get("schema_version"),
            "proof_hash": proof_hash,
            "deployment_claim_hash": deployment_claim_hash,
            "deployment_heartbeat_hash": deployment_heartbeat_hash,
            "mode": profile.get("mode", "paper"),
            "verification_status": profile.get("verification", {}).get("status", "empty"),
        },
        "signals": {
            "activity_level": activity_level(int(metrics.get("decisions", 0))),
            "rejection_discipline": rejection_discipline(float(metrics.get("rejection_rate", 0.0))),
            "execution_pressure": execution_pressure(float(metrics.get("acceptance_rate", 0.0))),
            "journal_quality": "durable" if metrics.get("journal_durable") else "ephemeral",
            "live_observed": int(metrics.get("live_execution_count", 0)) > 0,
        },
        "aggregates": {
            "decisions": int(metrics.get("decisions", 0)),
            "fills": int(metrics.get("fills", 0)),
            "rejections": int(metrics.get("rejections", 0)),
            "open_positions": int(metrics.get("open_positions", 0)),
            "acceptance_rate": float(metrics.get("acceptance_rate", 0.0)),
            "rejection_rate": float(metrics.get("rejection_rate", 0.0)),
            "total_notional_usd": float(metrics.get("total_notional_usd", 0.0)),
        },
        "commercial_unlocks": [
            "realtime feed",
            "longer history",
            "cohort analytics",
            "benchmark analytics",
            "webhooks",
            "bulk exports",
            "commercial redistribution rights",
            "enterprise reliability commitments",
        ],
        "privacy": {
            "default": "aggregate-only",
            "decision_records_included": False,
            "contains_exchange_credentials": False,
            "contains_private_notes": False,
            "inherits": profile.get("privacy", {}),
        },
    }
    assert_intelligence_safe(snapshot)
    return snapshot


def intelligence_catalog(*, generated_at: str, public_delay_s: int = 900) -> dict[str, Any]:
    commercial_contract = intelligence_commercial_contract(
        generated_at=generated_at,
        public_delay_s=public_delay_s,
    )
    catalog = {
        "schema_version": "zero.intelligence.catalog.v1",
        "generated_at": generated_at,
        "positioning": "commercial data product created by verified autonomous behavior",
        "public": {
            "runtime": "open-source",
            "network_profiles": "open",
            "leaderboards": "open",
            "model_gateway_status": {
                "schema_version": "zero.model_gateway.status.v1",
                "endpoint": "GET /intelligence/model-gateway",
                "default": "fail_closed unless an operator configures a provider",
            },
            "model_gateway_health": {
                "schema_version": "zero.model_gateway.health.v1",
                "endpoint": "GET /intelligence/model-gateway/health",
                "default": "config-only; explicit network=true required for provider probe",
            },
            "model_gateway_audit": {
                "schema_version": "zero.model_gateway.audit.v1",
                "endpoint": "GET /intelligence/model-gateway/audit",
                "default": "production model operations bundle without prompts or raw outputs",
            },
            "delayed_snapshots": {
                "schema_version": "zero.intelligence.snapshot.v1",
                "delay_s": public_delay_s,
                "endpoint": "GET /intelligence/snapshot",
            },
        },
        "commercial": {
            "metered_by": ["freshness", "history", "scale", "webhooks", "exports", "SLA"],
            "not_metered_by": ["local runtime use", "paper mode", "self-custodial operation"],
            "plans": [
                {
                    "name": "free",
                    "scopes": ["intelligence:read:delayed"],
                    "limits": "low public quota",
                },
                {
                    "name": "pro_operator",
                    "scopes": [
                        "intelligence:read:realtime",
                        "intelligence:read:history",
                        "intelligence:webhooks",
                    ],
                    "limits": "subscription quota",
                },
                {
                    "name": "team_fund",
                    "scopes": [
                        "intelligence:read:realtime",
                        "intelligence:read:history",
                        "intelligence:cohorts",
                        "intelligence:exports",
                        "intelligence:webhooks",
                    ],
                    "limits": "subscription plus usage",
                },
                {
                    "name": "enterprise",
                    "scopes": [
                        "intelligence:read:realtime",
                        "intelligence:read:history",
                        "intelligence:cohorts",
                        "intelligence:exports",
                        "intelligence:webhooks",
                        "intelligence:redistribute",
                    ],
                    "limits": "contract SLO and redistribution terms",
                },
            ],
        },
        "hosted_api_contract": {
            "schema_version": COMMERCIAL_CONTRACT_SCHEMA_VERSION,
            "endpoint": "GET /intelligence/commercial",
            "auth": commercial_contract["auth"],
            "rate_limit_headers": commercial_contract["rate_limits"]["headers"],
            "datasets": [dataset["name"] for dataset in commercial_contract["datasets"]],
            "endpoints": [endpoint["path"] for endpoint in commercial_contract["endpoints"]],
        },
        "data_rules": {
            "source": "verified redacted network proof packets",
            "raw_operator_data": "excluded unless explicitly consented and contracted",
            "exchange_credentials": "never collected",
            "custody": "never transferred",
        },
    }
    assert_intelligence_safe(catalog)
    return catalog


def intelligence_commercial_contract(
    *,
    generated_at: str,
    public_delay_s: int = 900,
) -> dict[str, Any]:
    contract = {
        "schema_version": COMMERCIAL_CONTRACT_SCHEMA_VERSION,
        "generated_at": generated_at,
        "positioning": "ZERO Intelligence API monetizes verified autonomous behavior, not runtime access",
        "boundary": {
            "open": [
                "local runtime",
                "paper mode",
                "self-custodial operation",
                "public profiles",
                "public leaderboards",
                "delayed public snapshots",
            ],
            "commercial": [
                "fresh realtime access",
                "history",
                "cohorts",
                "benchmarks",
                "webhooks",
                "bulk exports",
                "commercial redistribution",
                "reliability commitments",
            ],
            "not_sold": [
                "custody",
                "basic execution safety",
                "local private journals",
                "operator secrets",
            ],
        },
        "auth": {
            "scheme": "bearer",
            "credential": "hosted ZERO Intelligence API token",
            "runtime_required": False,
            "local_runtime_enforcement": "not enforced by the open-source runtime",
        },
        "plans": [
            {
                "id": "free",
                "name": "Free",
                "billing": "public quota",
                "scopes": ["intelligence:read:delayed"],
                "freshness": f"delayed >= {public_delay_s}s",
                "included_usage_events": ["snapshot.delayed.read"],
            },
            {
                "id": "pro_operator",
                "name": "Pro Operator",
                "billing": "subscription",
                "scopes": [
                    "intelligence:read:realtime",
                    "intelligence:read:history",
                    "intelligence:webhooks",
                ],
                "freshness": "realtime",
                "included_usage_events": [
                    "snapshot.realtime.read",
                    "history.query",
                    "webhook.delivery",
                ],
            },
            {
                "id": "team_fund",
                "name": "Team/Fund",
                "billing": "subscription plus usage",
                "scopes": [
                    "intelligence:read:realtime",
                    "intelligence:read:history",
                    "intelligence:cohorts",
                    "intelligence:benchmarks",
                    "intelligence:exports",
                    "intelligence:webhooks",
                ],
                "freshness": "realtime plus historical",
                "included_usage_events": [
                    "snapshot.realtime.read",
                    "history.query",
                    "cohort.query",
                    "benchmark.query",
                    "export.created",
                    "webhook.delivery",
                ],
            },
            {
                "id": "enterprise",
                "name": "Enterprise",
                "billing": "contract",
                "scopes": [
                    "intelligence:read:realtime",
                    "intelligence:read:history",
                    "intelligence:cohorts",
                    "intelligence:benchmarks",
                    "intelligence:exports",
                    "intelligence:webhooks",
                    "intelligence:redistribute",
                ],
                "freshness": "contract SLO",
                "included_usage_events": [
                    "snapshot.realtime.read",
                    "history.query",
                    "cohort.query",
                    "benchmark.query",
                    "export.created",
                    "webhook.delivery",
                    "redistribution.reported",
                ],
            },
        ],
        "scopes": [
            {
                "name": "intelligence:read:delayed",
                "description": "read delayed aggregate public snapshots",
                "commercial": False,
            },
            {
                "name": "intelligence:read:realtime",
                "description": "read fresh verified behavior snapshots",
                "commercial": True,
            },
            {
                "name": "intelligence:read:history",
                "description": "query historical verified behavior",
                "commercial": True,
            },
            {
                "name": "intelligence:cohorts",
                "description": "query cohort analytics",
                "commercial": True,
            },
            {
                "name": "intelligence:benchmarks",
                "description": "query benchmark analytics",
                "commercial": True,
            },
            {
                "name": "intelligence:exports",
                "description": "create bulk exports",
                "commercial": True,
            },
            {
                "name": "intelligence:webhooks",
                "description": "subscribe to event delivery",
                "commercial": True,
            },
            {
                "name": "intelligence:redistribute",
                "description": "redistribute intelligence commercially",
                "commercial": True,
            },
        ],
        "datasets": [
            {
                "name": "verified_behavior_snapshots",
                "source": "accepted ZERO Network ingestion packets",
                "public_delay_s": public_delay_s,
                "raw_private_data": False,
            },
            {
                "name": "risk_operations_history",
                "source": "aggregate risk, rejection, liveness, and breaker history",
                "public_delay_s": public_delay_s,
                "raw_private_data": False,
            },
            {
                "name": "cohort_benchmarks",
                "source": "aggregated cohorts from verified public-safe packets",
                "public_delay_s": public_delay_s,
                "raw_private_data": False,
            },
            {
                "name": "leaderboard_history",
                "source": "accepted leaderboard rows over time",
                "public_delay_s": public_delay_s,
                "raw_private_data": False,
            },
        ],
        "endpoints": [
            {
                "path": "GET /v1/intelligence/snapshots",
                "required_scope": "intelligence:read:delayed or intelligence:read:realtime",
                "usage_event": "snapshot.delayed.read or snapshot.realtime.read",
            },
            {
                "path": "GET /v1/intelligence/history",
                "required_scope": "intelligence:read:history",
                "usage_event": "history.query",
            },
            {
                "path": "GET /v1/intelligence/cohorts",
                "required_scope": "intelligence:cohorts",
                "usage_event": "cohort.query",
            },
            {
                "path": "GET /v1/intelligence/benchmarks",
                "required_scope": "intelligence:benchmarks",
                "usage_event": "benchmark.query",
            },
            {
                "path": "POST /v1/intelligence/webhooks",
                "required_scope": "intelligence:webhooks",
                "usage_event": "webhook.subscription.created",
            },
            {
                "path": "POST /v1/intelligence/exports",
                "required_scope": "intelligence:exports",
                "usage_event": "export.created",
            },
        ],
        "rate_limits": {
            "headers": [
                "x-zero-ratelimit-limit",
                "x-zero-ratelimit-remaining",
                "x-zero-ratelimit-reset",
                "x-zero-ratelimit-policy",
            ],
            "policy": [
                {
                    "plan": "free",
                    "window": "1h",
                    "unit": "requests",
                    "public_quota": True,
                },
                {
                    "plan": "pro_operator",
                    "window": "1m",
                    "unit": "requests plus webhook deliveries",
                    "public_quota": False,
                },
                {
                    "plan": "team_fund",
                    "window": "1m",
                    "unit": "requests, exports, and webhook deliveries",
                    "public_quota": False,
                },
                {
                    "plan": "enterprise",
                    "window": "contract",
                    "unit": "SLO-backed capacity",
                    "public_quota": False,
                },
            ],
        },
        "usage_events": [
            {
                "name": "snapshot.delayed.read",
                "metered": False,
                "billable": False,
                "required_fields": ["account_id", "scope", "dataset", "timestamp"],
            },
            {
                "name": "snapshot.realtime.read",
                "metered": True,
                "billable": True,
                "required_fields": ["account_id", "scope", "dataset", "timestamp", "freshness_ms"],
            },
            {
                "name": "history.query",
                "metered": True,
                "billable": True,
                "required_fields": ["account_id", "scope", "dataset", "timestamp", "rows_returned"],
            },
            {
                "name": "webhook.delivery",
                "metered": True,
                "billable": True,
                "required_fields": ["account_id", "scope", "event_type", "timestamp", "delivery_status"],
            },
            {
                "name": "export.created",
                "metered": True,
                "billable": True,
                "required_fields": ["account_id", "scope", "dataset", "timestamp", "rows_exported"],
            },
            {
                "name": "redistribution.reported",
                "metered": True,
                "billable": True,
                "required_fields": ["account_id", "scope", "dataset", "timestamp", "distribution_channel"],
            },
        ],
        "webhooks": {
            "event_types": [
                "snapshot.accepted",
                "cohort.updated",
                "benchmark.updated",
                "leaderboard.updated",
                "risk_regime.changed",
            ],
            "delivery": {
                "signing": "hosted webhook signatures required",
                "retries": "bounded retry with dead-letter visibility",
                "payloads": "aggregate-only",
            },
        },
        "exports": {
            "formats": ["jsonl", "csv"],
            "contents": "aggregate-only datasets selected by scope",
            "raw_private_data": False,
            "redistribution_requires_scope": "intelligence:redistribute",
        },
        "reliability": {
            "free": "best effort",
            "pro_operator": "status-page backed",
            "team_fund": "priority support",
            "enterprise": "contract SLO",
        },
        "privacy": {
            "exchange_credentials_collected": False,
            "custody_transferred": False,
            "raw_journals_required": False,
            "raw_model_prompts_included": False,
            "operator_secrets_included": False,
            "source_packets": "redacted ZERO Network packets only",
        },
    }
    assert_intelligence_safe(contract)
    return contract


def export_intelligence_snapshot(
    snapshot: dict[str, Any],
    *,
    consent: bool,
    export_path: str | None,
) -> dict[str, Any]:
    if not consent:
        return {
            "ok": False,
            "exported": False,
            "reason": "explicit consent required",
            "snapshot": snapshot,
        }
    if not export_path:
        return {
            "ok": False,
            "exported": False,
            "reason": "ZERO_INTELLIGENCE_EXPORT_PATH is not configured",
            "snapshot": snapshot,
        }
    assert_intelligence_safe(snapshot)
    path = Path(export_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(snapshot, sort_keys=True, separators=(",", ":")) + "\n")
    return {
        "ok": True,
        "exported": True,
        "reason": "exported to local ZERO Intelligence packet log",
        "path": str(path),
        "proof_hash": snapshot["source"]["proof_hash"],
        "snapshot": snapshot,
    }


def activity_level(decisions: int) -> str:
    if decisions <= 0:
        return "none"
    if decisions < 10:
        return "low"
    if decisions < 100:
        return "moderate"
    return "high"


def rejection_discipline(rejection_rate: float) -> str:
    if rejection_rate <= 0:
        return "none"
    if rejection_rate < 0.5:
        return "loose"
    if rejection_rate < 0.9:
        return "selective"
    return "strict"


def execution_pressure(acceptance_rate: float) -> str:
    if acceptance_rate <= 0:
        return "none"
    if acceptance_rate < 0.1:
        return "very_low"
    if acceptance_rate < 0.35:
        return "low"
    if acceptance_rate < 0.65:
        return "balanced"
    return "high"


def assert_intelligence_safe(payload: dict[str, Any]) -> None:
    assert_public_profile_safe(payload)
    body = json.dumps(payload, sort_keys=True).lower()
    forbidden = [
        "wallet material",
        "api private key",
        "exchange credential",
    ]
    for token in forbidden:
        if token in body:
            raise ValueError(f"intelligence packet contains forbidden token: {token}")
