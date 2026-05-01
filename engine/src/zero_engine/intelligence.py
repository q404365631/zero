from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from zero_engine.network import assert_public_profile_safe


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
    catalog = {
        "schema_version": "zero.intelligence.catalog.v1",
        "generated_at": generated_at,
        "positioning": "commercial data product created by verified autonomous behavior",
        "public": {
            "runtime": "open-source",
            "network_profiles": "open",
            "leaderboards": "open",
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
            "auth": "bearer API key",
            "rate_limit_headers": [
                "x-zero-ratelimit-limit",
                "x-zero-ratelimit-remaining",
                "x-zero-ratelimit-reset",
            ],
            "datasets": [
                "verified_behavior_snapshots",
                "cohort_benchmarks",
                "risk_operations_history",
                "leaderboard_history",
            ],
            "endpoints": [
                "GET /v1/intelligence/snapshots",
                "GET /v1/intelligence/cohorts",
                "GET /v1/intelligence/benchmarks",
                "POST /v1/intelligence/webhooks",
                "POST /v1/intelligence/exports",
            ],
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
