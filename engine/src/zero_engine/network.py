from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from zero_engine.paper import PaperEngine

HANDLE_RE = re.compile(r"^[a-zA-Z0-9_-]{3,32}$")
PROFILE_SCHEMA_VERSION = "zero.network.profile.v1"
LEADERBOARD_SCHEMA_VERSION = "zero.network.leaderboard.v1"


@dataclass(frozen=True)
class PublicProfileConfig:
    handle: str = "local-operator"
    display_name: str | None = None
    publish_enabled: bool = False

    def __post_init__(self) -> None:
        if not HANDLE_RE.match(self.handle):
            raise ValueError("network handle must be 3-32 chars: letters, numbers, _ or -")
        if self.display_name is not None and len(self.display_name.strip()) > 80:
            raise ValueError("network display name must be 80 chars or fewer")


def public_profile(
    engine: PaperEngine,
    *,
    config: PublicProfileConfig | None = None,
    generated_at: str,
    mode: str = "paper",
    live_execution_count: int = 0,
) -> dict[str, Any]:
    cfg = config or PublicProfileConfig()
    metrics = public_metrics(engine, live_execution_count=live_execution_count)
    proof_payload = {
        "schema_version": "zero.network.proof.v1",
        "handle": cfg.handle,
        "mode": mode,
        "metrics": metrics,
    }
    proof_hash = sha256_json(proof_payload)
    profile = {
        "schema_version": PROFILE_SCHEMA_VERSION,
        "generated_at": generated_at,
        "mode": mode,
        "profile": {
            "handle": cfg.handle,
            "display_name": cfg.display_name or cfg.handle,
            "publish_enabled": cfg.publish_enabled,
        },
        "verification": {
            "status": "verified" if metrics["decisions"] else "empty",
            "proof_hash": proof_hash,
            "badges": verification_badges(mode, metrics, proof_hash),
        },
        "metrics": metrics,
        "privacy": privacy_policy(),
        "leaderboard_row": leaderboard_row(cfg.handle, mode, metrics, proof_hash),
    }
    assert_public_profile_safe(profile)
    return profile


def public_metrics(engine: PaperEngine, *, live_execution_count: int = 0) -> dict[str, Any]:
    decisions = len(engine.decisions)
    fills = len(engine.fills)
    rejections = len(engine.rejections)
    open_positions = len([p for p in engine.positions.values() if p.quantity != 0])
    accepted = len([record for record in engine.decisions if record.decision.allowed])
    total_notional = sum(record.intent.notional_usd for record in engine.decisions)
    rejection_rate = rejections / decisions if decisions else 0.0
    acceptance_rate = accepted / decisions if decisions else 0.0
    return {
        "decisions": decisions,
        "fills": fills,
        "rejections": rejections,
        "open_positions": open_positions,
        "acceptance_rate": round(acceptance_rate, 4),
        "rejection_rate": round(rejection_rate, 4),
        "total_notional_usd": round(total_notional, 2),
        "live_execution_count": live_execution_count,
        "journal_durable": engine.recovery.durable or engine.journal is not None,
    }


def verification_badges(
    mode: str,
    metrics: dict[str, Any],
    proof_hash: str,
) -> list[dict[str, Any]]:
    badges = [
        {
            "name": "paper_verified",
            "status": "verified" if metrics["decisions"] else "empty",
            "evidence": proof_hash,
        }
    ]
    if mode == "live" or metrics["live_execution_count"] > 0:
        badges.append(
            {
                "name": "live_observed",
                "status": "verified" if metrics["live_execution_count"] > 0 else "not_observed",
                "evidence": proof_hash,
            }
        )
    if metrics["journal_durable"]:
        badges.append({"name": "durable_journal", "status": "verified", "evidence": proof_hash})
    return badges


def leaderboard_row(
    handle: str,
    mode: str,
    metrics: dict[str, Any],
    proof_hash: str,
) -> dict[str, Any]:
    score = min(
        100.0,
        (metrics["decisions"] * 1.0)
        + (metrics["rejections"] * 1.5)
        + (10.0 if metrics["journal_durable"] else 0.0),
    )
    return {
        "handle": handle,
        "mode": mode,
        "decisions": metrics["decisions"],
        "rejection_rate": metrics["rejection_rate"],
        "open_positions": metrics["open_positions"],
        "verification_score": round(score, 2),
        "proof_hash": proof_hash,
    }


def public_leaderboard(
    profiles: list[dict[str, Any]] | tuple[dict[str, Any], ...],
    *,
    generated_at: str,
    limit: int = 100,
) -> dict[str, Any]:
    if limit <= 0:
        raise ValueError("leaderboard limit must be positive")
    rows = [_public_leaderboard_row(profile) for profile in profiles]
    rows.sort(
        key=lambda row: (
            -float(row["verification_score"]),
            -int(row["decisions"]),
            -float(row["rejection_rate"]),
            str(row["handle"]),
        )
    )
    ranked_rows = [
        {
            "rank": index,
            **row,
        }
        for index, row in enumerate(rows[:limit], start=1)
    ]
    payload = {
        "schema_version": LEADERBOARD_SCHEMA_VERSION,
        "generated_at": generated_at,
        "row_count": len(ranked_rows),
        "rows": ranked_rows,
        "rules": {
            "ranking": [
                "verification_score desc",
                "decisions desc",
                "rejection_rate desc",
                "handle asc",
            ],
            "purpose": "proof-of-process, not financial advice",
        },
        "privacy": privacy_policy(),
    }
    assert_public_profile_safe(payload)
    return payload


def load_public_profiles(path: str | Path) -> tuple[dict[str, Any], ...]:
    profiles = []
    with Path(path).open(encoding="utf-8") as fh:
        for line_number, line in enumerate(fh, start=1):
            stripped = line.strip()
            if not stripped:
                continue
            profile = json.loads(stripped)
            if not isinstance(profile, dict):
                raise ValueError(f"profile log line {line_number} must be a JSON object")
            _public_leaderboard_row(profile)
            profiles.append(profile)
    return tuple(profiles)


def publish_profile(
    profile: dict[str, Any],
    *,
    consent: bool,
    publish_path: str | None,
) -> dict[str, Any]:
    if not consent:
        return {
            "ok": False,
            "published": False,
            "reason": "explicit consent required",
            "profile": profile,
        }
    if not publish_path:
        return {
            "ok": False,
            "published": False,
            "reason": "ZERO_NETWORK_PUBLISH_PATH is not configured",
            "profile": profile,
        }
    assert_public_profile_safe(profile)
    path = Path(publish_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(profile, sort_keys=True, separators=(",", ":")) + "\n")
    return {
        "ok": True,
        "published": True,
        "reason": "published to local ZERO Network proof log",
        "path": str(path),
        "proof_hash": profile["verification"]["proof_hash"],
        "profile": profile,
    }


def privacy_policy() -> dict[str, Any]:
    return {
        "default": "private",
        "publication": "opt-in",
        "included": [
            "aggregate decision counts",
            "aggregate fill and rejection counts",
            "aggregate notional",
            "verification badge status",
            "proof hash",
        ],
        "excluded": [
            "raw decisions",
            "trace ids",
            "idempotency keys",
            "wallet addresses",
            "exchange order ids",
            "private notes",
            "strategy source labels",
            "per-trade symbols",
        ],
    }


def assert_public_profile_safe(payload: dict[str, Any]) -> None:
    body = json.dumps(payload, sort_keys=True).lower()
    forbidden = [
        "trace_id",
        "idempotency_key",
        "wallet_address",
        "private_key",
        "exchange_response",
        "api:/execute",
        "strategy:",
        "0x" + ("1" * 16),
    ]
    for token in forbidden:
        if token in body:
            raise ValueError(f"public profile contains forbidden token: {token}")


def sha256_json(payload: dict[str, Any]) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def _public_leaderboard_row(profile: dict[str, Any]) -> dict[str, Any]:
    if profile.get("schema_version") != PROFILE_SCHEMA_VERSION:
        raise ValueError("public profile schema_version must be zero.network.profile.v1")
    assert_public_profile_safe(profile)

    row = profile.get("leaderboard_row")
    if not isinstance(row, dict):
        raise ValueError("public profile missing leaderboard_row")

    handle = str(profile.get("profile", {}).get("handle", ""))
    proof_hash = str(profile.get("verification", {}).get("proof_hash", ""))
    if row.get("handle") != handle:
        raise ValueError("leaderboard row handle must match profile handle")
    if row.get("proof_hash") != proof_hash:
        raise ValueError("leaderboard row proof_hash must match profile proof_hash")

    return {
        "handle": handle,
        "display_name": str(profile.get("profile", {}).get("display_name") or handle),
        "mode": str(row.get("mode", profile.get("mode", "paper"))),
        "decisions": int(row.get("decisions", 0)),
        "rejection_rate": float(row.get("rejection_rate", 0.0)),
        "open_positions": int(row.get("open_positions", 0)),
        "verification_score": float(row.get("verification_score", 0.0)),
        "proof_hash": proof_hash,
    }
