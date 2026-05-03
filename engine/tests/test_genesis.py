from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path

import pytest

from zero_engine.genesis import (
    GenesisJournal,
    Proposal,
    decide_proposal,
    load_proposals,
    main,
    plan_proposals,
    snapshot_from_proposals,
    status_snapshot,
)

FIXED = datetime(2026, 5, 1, tzinfo=UTC)
FIXTURE_PATH = Path(__file__).resolve().parents[2] / "examples" / "genesis" / "proposals.jsonl"


def proposal(**overrides) -> Proposal:
    payload = {
        "title": "Document accepted fixture behavior",
        "summary": "Promote fixture-backed learning into public docs.",
        "target_paths": ("docs/strategy-plugins.md",),
        "evidence_refs": ("docs/proof/demo/proof-pack.json",),
        "sample_size": 42,
        "risk_tier": "medium",
        "revert_plan": "Revert the docs update.",
        "created_at": FIXED,
    }
    payload.update(overrides)
    return Proposal(**payload)


def test_guardian_accepts_fixture_backed_non_protected_proposal() -> None:
    decision = decide_proposal(proposal(), now=FIXED)

    assert decision.decision == "accepted"
    assert decision.required_human_review is False
    assert decision.proposal.effective_risk_tier == "medium"
    assert decision.min_sample_size == 30


def test_guardian_rejects_missing_sample_or_revert_plan() -> None:
    small_sample = decide_proposal(proposal(sample_size=2, risk_tier="low"), now=FIXED)
    no_revert = decide_proposal(proposal(revert_plan=""), now=FIXED)

    assert small_sample.decision == "rejected"
    assert "sample_size>=5" in small_sample.reason
    assert no_revert.decision == "rejected"
    assert "revert_plan" in no_revert.reason


def test_guardian_escalates_protected_paths_and_high_risk() -> None:
    live = decide_proposal(
        proposal(target_paths=("engine/src/zero_engine/live.py",), sample_size=140),
        now=FIXED,
    )
    immune = decide_proposal(
        proposal(target_paths=("engine/src/zero_engine/immune.py",), sample_size=140),
        now=FIXED,
    )
    high = decide_proposal(proposal(risk_tier="high", sample_size=120), now=FIXED)

    assert live.decision == "escalated"
    assert live.required_human_review is True
    assert live.proposal.effective_risk_tier == "protected"
    assert "execution" in live.proposal.protected_classes
    assert "live_adapters" in live.proposal.protected_classes
    assert immune.decision == "escalated"
    assert "immune_core" in immune.proposal.protected_classes
    assert high.decision == "escalated"
    assert high.required_human_review is True


def test_genesis_journal_is_append_only_and_deduplicated(tmp_path) -> None:
    journal = GenesisJournal(tmp_path / "genesis.jsonl")
    proposals = [proposal(), proposal(title="Adjust docs after low-risk evidence", risk_tier="low")]

    first = plan_proposals(proposals, journal=journal, now=FIXED)
    second = plan_proposals(proposals, journal=journal, now=FIXED)
    status = status_snapshot(journal=journal, now=FIXED)

    assert first["appended_decisions"] == 2
    assert second["appended_decisions"] == 0
    assert status["source"] == "genesis-journal"
    assert status["stats"]["total_decisions"] == 2


def test_snapshot_is_plan_only_and_public_safe() -> None:
    snapshot = snapshot_from_proposals([proposal()], now=FIXED)
    serialized = json.dumps(snapshot, sort_keys=True).lower()

    assert snapshot["schema_version"] == "zero.genesis.snapshot.v1"
    assert snapshot["applies_code_changes"] is False
    assert snapshot["guardian_policy"]["protected_paths_require_human_review"] is True
    assert "0x1234567890" not in serialized
    assert "sk_live_" not in serialized


def test_genesis_rejects_secret_like_material() -> None:
    with pytest.raises(ValueError, match="wallet-like"):
        proposal(summary="send to 0x1234567890abcdef1234567890abcdef12345678")

    with pytest.raises(ValueError, match="derivable or secret fields"):
        proposal(metadata={"private_key": "redacted"})


def test_loads_fixture_proposals_deterministically() -> None:
    proposals = load_proposals(FIXTURE_PATH)
    snapshot = snapshot_from_proposals(proposals, now=FIXED)

    assert [decision["decision"] for decision in snapshot["decisions"]] == [
        "accepted",
        "rejected",
        "escalated",
    ]
    assert snapshot["stats"]["by_decision"] == {
        "accepted": 1,
        "escalated": 1,
        "rejected": 1,
    }


def test_genesis_cli_plan_and_status(tmp_path, capsys) -> None:
    journal = tmp_path / "genesis.jsonl"

    plan_code = main(
        [
            "plan",
            "--proposals",
            str(FIXTURE_PATH),
            "--journal",
            str(journal),
            "--now",
            "2026-05-01T00:00:00Z",
        ]
    )
    plan_payload = json.loads(capsys.readouterr().out)
    status_code = main(["status", "--journal", str(journal), "--now", "2026-05-01T00:00:00Z"])
    status_payload = json.loads(capsys.readouterr().out)

    assert plan_code == 0
    assert status_code == 0
    assert plan_payload["schema_version"] == "zero.genesis.plan.v1"
    assert plan_payload["appended_decisions"] == 3
    assert status_payload["schema_version"] == "zero.genesis.status.v1"
    assert status_payload["stats"]["total_decisions"] == 3
