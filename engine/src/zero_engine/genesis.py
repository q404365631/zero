from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from zero_engine.memory import isoformat, load_jsonl, parse_datetime, stable_hash

JsonMap = dict[str, Any]

PROPOSAL_SCHEMA_VERSION = "zero.genesis.proposal.v1"
DECISION_SCHEMA_VERSION = "zero.genesis.guardian.v1"
PLAN_SCHEMA_VERSION = "zero.genesis.plan.v1"
STATUS_SCHEMA_VERSION = "zero.genesis.status.v1"
SNAPSHOT_SCHEMA_VERSION = "zero.genesis.snapshot.v1"

RISK_TIERS = {"low", "medium", "high", "protected"}
DECISIONS = {"accepted", "escalated", "rejected"}
MIN_SAMPLE_SIZE = {
    "low": 5,
    "medium": 30,
    "high": 100,
    "protected": 100,
}
PROTECTED_PATH_CLASSES: dict[str, tuple[str, ...]] = {
    "execution": (
        "engine/src/zero_engine/live.py",
        "cli/crates/zero-commands/src/dispatch.rs",
    ),
    "sizing": (
        "engine/src/zero_engine/safety.py",
        "engine/src/zero_engine/models.py",
    ),
    "stops": ("stop", "trailing", "drawdown"),
    "circuit_breakers": (
        "engine/src/zero_engine/immune.py",
        "circuit",
        "breaker",
    ),
    "live_adapters": (
        "engine/src/zero_engine/hyperliquid.py",
        "engine/src/zero_engine/live.py",
        "hyperliquid",
    ),
    "immune_core": ("engine/src/zero_engine/immune.py", "immune"),
}
FORBIDDEN_KEYS = {
    "api_key",
    "exchange_order_id",
    "idempotency_key",
    "notional_usd",
    "order_id",
    "price",
    "private_key",
    "quantity",
    "raw",
    "raw_payload",
    "secret",
    "size",
    "wallet",
    "wallet_address",
}
SENSITIVE_TEXT_RE = re.compile(
    r"(?:0x[a-fA-F0-9]{32,}|[A-Za-z0-9_=-]{40,}|sk-[A-Za-z0-9_-]{20,})"
)


def utc_now() -> datetime:
    return datetime.now(UTC)


def _walk_forbidden_keys(payload: Any, path: str = "") -> list[str]:
    findings: list[str] = []
    if isinstance(payload, Mapping):
        for key, value in payload.items():
            key_text = str(key)
            key_path = f"{path}.{key_text}" if path else key_text
            if key_text.lower() in FORBIDDEN_KEYS:
                findings.append(key_path)
            findings.extend(_walk_forbidden_keys(value, key_path))
    elif isinstance(payload, list):
        for idx, value in enumerate(payload):
            findings.extend(_walk_forbidden_keys(value, f"{path}[{idx}]"))
    return findings


def assert_public_safe_proposal(payload: Mapping[str, Any]) -> None:
    forbidden = _walk_forbidden_keys(payload.get("metadata", {}))
    if forbidden:
        raise ValueError("genesis metadata contains derivable or secret fields: " + ", ".join(forbidden))

    text = "\n".join(
        [
            str(payload.get("title", "")),
            str(payload.get("summary", "")),
            str(payload.get("revert_plan", "")),
            " ".join(str(item) for item in payload.get("evidence_refs", [])),
            " ".join(str(item) for item in payload.get("target_paths", [])),
        ]
    )
    if SENSITIVE_TEXT_RE.search(text):
        raise ValueError("genesis proposal contains secret-like or wallet-like material")


def protected_classes_for_paths(paths: Iterable[str]) -> tuple[str, ...]:
    classes: set[str] = set()
    normalized_paths = [path.lower() for path in paths]
    for class_name, markers in PROTECTED_PATH_CLASSES.items():
        lower_markers = [marker.lower() for marker in markers]
        if any(marker in path for marker in lower_markers for path in normalized_paths):
            classes.add(class_name)
    return tuple(sorted(classes))


def proposal_id(payload: Mapping[str, Any]) -> str:
    return stable_hash(
        {
            "title": payload.get("title"),
            "summary": payload.get("summary"),
            "target_paths": payload.get("target_paths", []),
            "evidence_refs": payload.get("evidence_refs", []),
        }
    )


@dataclass(frozen=True)
class Proposal:
    title: str
    summary: str
    target_paths: tuple[str, ...]
    evidence_refs: tuple[str, ...]
    sample_size: int
    risk_tier: str
    revert_plan: str
    created_at: datetime
    metadata: Mapping[str, Any] = field(default_factory=dict)
    schema_version: str = PROPOSAL_SCHEMA_VERSION
    proposal_id: str | None = None

    def __post_init__(self) -> None:
        if self.risk_tier not in RISK_TIERS:
            raise ValueError(f"unsupported genesis risk tier: {self.risk_tier}")
        if self.sample_size < 0:
            raise ValueError("sample_size must be non-negative")
        if not self.title.strip():
            raise ValueError("proposal title is required")
        if not self.summary.strip():
            raise ValueError("proposal summary is required")
        if not self.target_paths:
            raise ValueError("target_paths are required")
        assert_public_safe_proposal(self.to_dict(include_id=False))

    @property
    def id(self) -> str:
        return self.proposal_id or proposal_id(self.to_dict(include_id=False))

    @property
    def protected_classes(self) -> tuple[str, ...]:
        return protected_classes_for_paths(self.target_paths)

    @property
    def effective_risk_tier(self) -> str:
        return "protected" if self.protected_classes else self.risk_tier

    def to_dict(self, *, include_id: bool = True) -> JsonMap:
        payload: JsonMap = {
            "schema_version": self.schema_version,
            "title": self.title,
            "summary": self.summary,
            "target_paths": list(self.target_paths),
            "evidence_refs": list(self.evidence_refs),
            "sample_size": self.sample_size,
            "risk_tier": self.risk_tier,
            "effective_risk_tier": self.effective_risk_tier,
            "protected_classes": list(self.protected_classes),
            "revert_plan": self.revert_plan,
            "created_at": isoformat(self.created_at),
            "metadata": dict(self.metadata),
        }
        if include_id:
            payload["id"] = self.id
        return payload

    @classmethod
    def from_dict(cls, payload: Mapping[str, Any]) -> "Proposal":
        if payload.get("schema_version") != PROPOSAL_SCHEMA_VERSION:
            raise ValueError("unsupported genesis proposal schema_version")
        return cls(
            proposal_id=str(payload["id"]) if payload.get("id") else None,
            title=str(payload["title"]),
            summary=str(payload["summary"]),
            target_paths=tuple(str(path) for path in payload.get("target_paths", [])),
            evidence_refs=tuple(str(ref) for ref in payload.get("evidence_refs", [])),
            sample_size=int(payload.get("sample_size", 0)),
            risk_tier=str(payload.get("risk_tier", "low")),
            revert_plan=str(payload.get("revert_plan", "")),
            created_at=parse_datetime(str(payload["created_at"])),
            metadata=payload.get("metadata", {}),
        )


@dataclass(frozen=True)
class GuardianDecision:
    proposal: Proposal
    decision: str
    reason: str
    decided_at: datetime
    required_human_review: bool
    min_sample_size: int
    policy_version: str = "zero.genesis.guardian_policy.v1"
    schema_version: str = DECISION_SCHEMA_VERSION

    def __post_init__(self) -> None:
        if self.decision not in DECISIONS:
            raise ValueError(f"unsupported guardian decision: {self.decision}")

    @property
    def id(self) -> str:
        return stable_hash(
            {
                "proposal_id": self.proposal.id,
                "decision": self.decision,
                "reason": self.reason,
                "policy_version": self.policy_version,
            }
        )

    def to_dict(self) -> JsonMap:
        return {
            "schema_version": self.schema_version,
            "id": self.id,
            "proposal_id": self.proposal.id,
            "decision": self.decision,
            "reason": self.reason,
            "decided_at": isoformat(self.decided_at),
            "required_human_review": self.required_human_review,
            "min_sample_size": self.min_sample_size,
            "policy_version": self.policy_version,
            "proposal": self.proposal.to_dict(),
        }

    @classmethod
    def from_dict(cls, payload: Mapping[str, Any]) -> "GuardianDecision":
        if payload.get("schema_version") != DECISION_SCHEMA_VERSION:
            raise ValueError("unsupported genesis guardian schema_version")
        return cls(
            proposal=Proposal.from_dict(payload["proposal"]),
            decision=str(payload["decision"]),
            reason=str(payload["reason"]),
            decided_at=parse_datetime(str(payload["decided_at"])),
            required_human_review=bool(payload["required_human_review"]),
            min_sample_size=int(payload["min_sample_size"]),
            policy_version=str(payload.get("policy_version", "zero.genesis.guardian_policy.v1")),
        )


class GenesisJournal:
    """Append-only JSONL store for genesis proposal decisions."""

    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)

    def append(self, decision: GuardianDecision) -> bool:
        seen = {existing.proposal.id for existing in self.read_all()}
        if decision.proposal.id in seen:
            return False
        self.path.parent.mkdir(parents=True, exist_ok=True)
        line = json.dumps(decision.to_dict(), sort_keys=True, separators=(",", ":")) + "\n"
        fd = os.open(self.path, os.O_APPEND | os.O_CREAT | os.O_WRONLY, 0o600)
        with os.fdopen(fd, "a", encoding="utf-8") as handle:
            handle.write(line)
            handle.flush()
            os.fsync(handle.fileno())
        return True

    def append_many(self, decisions: Iterable[GuardianDecision]) -> int:
        return sum(1 for decision in decisions if self.append(decision))

    def read_all(self) -> list[GuardianDecision]:
        if not self.path.exists():
            return []
        lines = self.path.read_text(encoding="utf-8").splitlines()
        return [GuardianDecision.from_dict(json.loads(line)) for line in lines if line.strip()]

    def stats(self, now: datetime | None = None) -> JsonMap:
        now = now or utc_now()
        decisions = self.read_all()
        by_decision = {decision: 0 for decision in sorted(DECISIONS)}
        by_risk = {risk: 0 for risk in sorted(RISK_TIERS)}
        protected_classes: set[str] = set()
        for item in decisions:
            by_decision[item.decision] += 1
            by_risk[item.proposal.effective_risk_tier] += 1
            protected_classes.update(item.proposal.protected_classes)
        return {
            "schema_version": "zero.genesis.stats.v1",
            "generated_at": isoformat(now),
            "path": str(self.path),
            "total_decisions": len(decisions),
            "by_decision": by_decision,
            "by_effective_risk_tier": by_risk,
            "protected_classes": sorted(protected_classes),
            "deduplication": "idempotent-by-proposal-id",
            "privacy": {
                "contains_live_prices": False,
                "contains_wallet_material": False,
                "contains_exchange_order_ids": False,
                "contains_private_keys": False,
            },
        }


def decide_proposal(proposal: Proposal, *, now: datetime | None = None) -> GuardianDecision:
    now = now or utc_now()
    effective = proposal.effective_risk_tier
    min_sample = MIN_SAMPLE_SIZE[effective]
    missing: list[str] = []
    if not proposal.evidence_refs:
        missing.append("evidence_refs")
    if not proposal.revert_plan.strip():
        missing.append("revert_plan")
    if proposal.sample_size < min_sample:
        missing.append(f"sample_size>={min_sample}")

    if missing:
        return GuardianDecision(
            proposal=proposal,
            decision="rejected",
            reason="missing guardian requirements: " + ", ".join(missing),
            decided_at=now,
            required_human_review=False,
            min_sample_size=min_sample,
        )

    if proposal.protected_classes:
        return GuardianDecision(
            proposal=proposal,
            decision="escalated",
            reason="protected path classes require human review: "
            + ", ".join(proposal.protected_classes),
            decided_at=now,
            required_human_review=True,
            min_sample_size=min_sample,
        )

    if proposal.risk_tier == "high":
        return GuardianDecision(
            proposal=proposal,
            decision="escalated",
            reason="high-risk proposal requires human review",
            decided_at=now,
            required_human_review=True,
            min_sample_size=min_sample,
        )

    return GuardianDecision(
        proposal=proposal,
        decision="accepted",
        reason="guardian requirements satisfied for non-protected proposal",
        decided_at=now,
        required_human_review=False,
        min_sample_size=min_sample,
    )


def plan_proposals(
    proposals: Iterable[Proposal],
    *,
    journal: GenesisJournal | None = None,
    now: datetime | None = None,
) -> JsonMap:
    now = now or utc_now()
    decisions = [decide_proposal(proposal, now=now) for proposal in proposals]
    appended = journal.append_many(decisions) if journal is not None else 0
    return {
        "schema_version": PLAN_SCHEMA_VERSION,
        "generated_at": isoformat(now),
        "mode": "plan-only",
        "applies_code_changes": False,
        "append_only_journal": journal is not None,
        "appended_decisions": appended,
        "decisions": [decision.to_dict() for decision in decisions],
        "stats": summarize_decisions(decisions, generated_at=now),
    }


def summarize_decisions(decisions: Iterable[GuardianDecision], *, generated_at: datetime) -> JsonMap:
    items = list(decisions)
    by_decision = {decision: 0 for decision in sorted(DECISIONS)}
    protected_classes: set[str] = set()
    for item in items:
        by_decision[item.decision] += 1
        protected_classes.update(item.proposal.protected_classes)
    return {
        "schema_version": "zero.genesis.summary.v1",
        "generated_at": isoformat(generated_at),
        "total_decisions": len(items),
        "by_decision": by_decision,
        "protected_classes": sorted(protected_classes),
        "human_review_required": by_decision["escalated"],
    }


def status_snapshot(
    *,
    journal: GenesisJournal | None = None,
    proposals: Iterable[Proposal] | None = None,
    now: datetime | None = None,
) -> JsonMap:
    now = now or utc_now()
    if journal is not None:
        decisions = journal.read_all()
        source = "genesis-journal"
        stats = journal.stats(now)
    else:
        decisions = [decide_proposal(proposal, now=now) for proposal in (proposals or [])]
        source = "ephemeral-proposals"
        stats = summarize_decisions(decisions, generated_at=now)
    return {
        "schema_version": STATUS_SCHEMA_VERSION,
        "generated_at": isoformat(now),
        "source": source,
        "mode": "plan-only",
        "applies_code_changes": False,
        "stats": stats,
        "decisions": [decision.to_dict() for decision in decisions],
    }


def snapshot_from_proposals(
    proposals: Iterable[Proposal],
    *,
    now: datetime | None = None,
) -> JsonMap:
    now = now or utc_now()
    decisions = [decide_proposal(proposal, now=now) for proposal in proposals]
    return {
        "schema_version": SNAPSHOT_SCHEMA_VERSION,
        "generated_at": isoformat(now),
        "source": "fixture-proposals",
        "mode": "plan-only",
        "applies_code_changes": False,
        "guardian_policy": {
            "schema_version": "zero.genesis.guardian_policy.v1",
            "min_sample_size": dict(MIN_SAMPLE_SIZE),
            "protected_path_classes": {
                key: list(value) for key, value in sorted(PROTECTED_PATH_CLASSES.items())
            },
            "requires_revert_plan": True,
            "requires_evidence_refs": True,
            "protected_paths_require_human_review": True,
            "high_risk_requires_human_review": True,
        },
        "stats": summarize_decisions(decisions, generated_at=now),
        "decisions": [decision.to_dict() for decision in decisions],
        "privacy": {
            "contains_live_prices": False,
            "contains_wallet_material": False,
            "contains_exchange_order_ids": False,
            "contains_private_keys": False,
        },
    }


def load_proposals(path: str | Path) -> list[Proposal]:
    return [Proposal.from_dict(payload) for payload in load_jsonl(path)]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="ZERO genesis proposal planner")
    subcommands = parser.add_subparsers(dest="command", required=True)

    plan = subcommands.add_parser("plan", help="classify proposals and append guardian decisions")
    plan.add_argument("--proposals", required=True, help="proposal JSONL input")
    plan.add_argument("--journal", required=True, help="append-only genesis JSONL journal")
    plan.add_argument("--now", help="UTC timestamp override for deterministic runs")

    status = subcommands.add_parser("status", help="print genesis journal status")
    status.add_argument("--journal", required=True, help="append-only genesis JSONL journal")
    status.add_argument("--now", help="UTC timestamp override for deterministic runs")

    args = parser.parse_args(argv)
    now = parse_datetime(args.now) if getattr(args, "now", None) else utc_now()
    journal = GenesisJournal(args.journal)

    if args.command == "plan":
        proposals = load_proposals(args.proposals)
        print(json.dumps(plan_proposals(proposals, journal=journal, now=now), indent=2, sort_keys=True))
        return 0

    if args.command == "status":
        print(json.dumps(status_snapshot(journal=journal, now=now), indent=2, sort_keys=True))
        return 0

    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
