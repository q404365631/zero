from __future__ import annotations

import json
import re
from dataclasses import dataclass
from typing import Any, Mapping

from zero_engine.network import assert_public_profile_safe, sha256_json

DEPLOYMENT_CLAIM_SCHEMA_VERSION = "zero.deployment.claim.v1"

SAFE_TEXT_RE = re.compile(r"[^a-zA-Z0-9_.:@/-]")


@dataclass(frozen=True)
class DeploymentIdentityConfig:
    deployment_id: str = "local-paper"
    deployment_kind: str = "local"
    environment: str = "paper"
    owner: str = "local-operator"
    version: str = "0.1.1"
    public_key: str | None = None
    signature: str | None = None
    signer: str | None = None

    def __post_init__(self) -> None:
        for field_name in ("deployment_id", "deployment_kind", "environment", "owner", "version"):
            value = getattr(self, field_name)
            if not _safe_text(value, default=""):
                raise ValueError(f"deployment {field_name} must not be empty")


def deployment_claim(
    *,
    config: DeploymentIdentityConfig | None = None,
    generated_at: str,
    operator_context: Mapping[str, Any] | None = None,
    runtime: Mapping[str, Any] | None = None,
    evidence: Mapping[str, Any] | None = None,
) -> dict[str, Any]:
    cfg = config or DeploymentIdentityConfig()
    body = {
        "schema_version": DEPLOYMENT_CLAIM_SCHEMA_VERSION,
        "generated_at": generated_at,
        "deployment": {
            "deployment_id": _safe_text(cfg.deployment_id, default="local-paper"),
            "kind": _safe_text(cfg.deployment_kind, default="local"),
            "environment": _safe_text(cfg.environment, default="paper"),
            "owner": _safe_text(cfg.owner, default="local-operator"),
            "version": _safe_text(cfg.version, default="0.1.1"),
        },
        "operator": _operator_claim(operator_context or {}),
        "runtime": _safe_mapping(runtime or {}),
        "evidence": _safe_mapping(evidence or {}),
        "privacy": {
            "default": "public-safe-aggregate",
            "contains_exchange_credentials": False,
            "contains_wallet_material": False,
            "contains_raw_decisions": False,
            "contains_trace_tokens": False,
            "contains_idempotency_tokens": False,
        },
    }
    claim_hash = sha256_json(body)
    signature = _signature_claim(cfg, claim_hash)
    claim = {
        **body,
        "claim_hash": claim_hash,
        "signature": signature,
    }
    assert_deployment_claim_safe(claim)
    return claim


def assert_deployment_claim_safe(payload: dict[str, Any]) -> None:
    assert_public_profile_safe(payload)
    body = json.dumps(payload, sort_keys=True).lower()
    forbidden = [
        "exchange credential",
        "wallet material",
        "raw decision",
        "idempotency key",
        "trace id",
    ]
    for token in forbidden:
        if token in body:
            raise ValueError(f"deployment claim contains forbidden token: {token}")


def _operator_claim(operator_context: Mapping[str, Any]) -> dict[str, Any]:
    return {
        "handle": _safe_text(operator_context.get("handle"), default="local-operator"),
        "role": _safe_text(operator_context.get("role"), default="owner", max_len=32),
        "scope": _safe_text(operator_context.get("scope"), default="local-private", max_len=40),
        "source": _safe_text(operator_context.get("source"), default="runtime-default", max_len=40),
    }


def _safe_mapping(values: Mapping[str, Any]) -> dict[str, Any]:
    safe: dict[str, Any] = {}
    for key, value in values.items():
        safe_key = _safe_text(key, default="field", max_len=64)
        if isinstance(value, bool) or value is None:
            safe[safe_key] = value
        elif isinstance(value, int | float):
            safe[safe_key] = value
        else:
            safe[safe_key] = _safe_text(value, default="unknown", max_len=120)
    return safe


def _signature_claim(cfg: DeploymentIdentityConfig, claim_hash: str) -> dict[str, Any]:
    if cfg.public_key and cfg.signature:
        return {
            "status": "signed_external",
            "algorithm": "external",
            "public_key": _safe_text(cfg.public_key, default="", max_len=512),
            "signature": _safe_text(cfg.signature, default="", max_len=512),
            "signer": _safe_text(cfg.signer, default="external", max_len=80),
            "signed_claim_hash": claim_hash,
        }
    return {
        "status": "unsigned_local",
        "algorithm": None,
        "public_key": None,
        "signature": None,
        "signer": _safe_text(cfg.signer, default="local-runtime", max_len=80),
        "signed_claim_hash": claim_hash,
    }


def _safe_text(value: Any, *, default: str, max_len: int = 96) -> str:
    raw = str(value or "").strip()
    if not raw:
        return default
    safe = SAFE_TEXT_RE.sub("-", raw)
    safe = safe.strip("-")
    return (safe or default)[:max_len]
