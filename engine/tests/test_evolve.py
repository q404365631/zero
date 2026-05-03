from __future__ import annotations

import json
from datetime import UTC, datetime
from pathlib import Path

from zero_engine.evolve import (
    evolve_policy,
    load_guardian_decisions,
    paths_allowed,
    red_team_review,
    run_evolve,
    snapshot_from_fixture,
    status_snapshot,
)
from zero_engine.genesis import load_proposals, plan_proposals

FIXED = datetime(2026, 5, 1, tzinfo=UTC)
ROOT = Path(__file__).resolve().parents[2]
PROPOSALS = ROOT / "examples" / "genesis" / "proposals.jsonl"


def planned_decisions() -> list:
    planned = plan_proposals(load_proposals(PROPOSALS), now=FIXED)
    from zero_engine.genesis import GuardianDecision

    return [GuardianDecision.from_dict(item) for item in planned["decisions"]]


def test_paths_allowed_blocks_protected_and_non_public_roots() -> None:
    assert paths_allowed(["docs/strategy-plugins.md"])
    assert paths_allowed(["examples/strategy-runner/README.md"])
    assert not paths_allowed(["engine/src/zero_engine/live.py"])
    assert not paths_allowed(["contracts/paper-api/genesis.json"])


def test_evolve_runs_build_red_team_canary_calibration_and_local_promotion(tmp_path) -> None:
    payload = run_evolve(
        decisions=planned_decisions(),
        output=tmp_path,
        repo_root=ROOT,
        now=FIXED,
    )

    assert payload["schema_version"] == "zero.evolve.run.v1"
    assert payload["mode"] == "paper-only"
    assert payload["applies_to_checkout"] is False
    assert payload["pushes_to_remote"] is False
    assert payload["selected_proposal_id"] == "sha256:genesis-accepted-docs"
    assert payload["build"]["checks"]["target_paths_allowed"] is True
    assert payload["red_team"]["verdict"] == "pass"
    assert payload["paper_canary"]["fills"] == 2
    assert payload["calibration"]["passed"] is True
    assert payload["promotion"]["promotable_after_human_review"] is True
    assert payload["promotion"]["promoted"] is False
    assert payload["promotion"]["requires_human_approval"] is True
    assert payload["promotion_plan"]["schema_version"] == "zero.evolve.promotion_plan.v1"
    assert payload["promotion_plan"]["eligible_for_local_apply"] is True
    assert payload["promotion_plan"]["applies_to_checkout"] is False
    assert payload["promotion_plan"]["pushes_to_remote"] is False
    assert (
        payload["promotion_plan"]["required_approval_phrase"]
        == "I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION"
    )
    assert payload["rollback_plan"]["schema_version"] == "zero.evolve.rollback_plan.v1"
    assert payload["rollback_plan"]["rollback_ready"] is True
    assert payload["rollback_plan"]["pushes_to_remote"] is False
    assert (
        payload["promotion_verification"]["schema_version"]
        == "zero.evolve.promotion_verification.v1"
    )
    assert payload["promotion_verification"]["ok"] is True
    assert (tmp_path / "evolve-run.json").is_file()
    assert (tmp_path / "worktree" / "candidate.patch").is_file()
    candidate = tmp_path / "worktree" / "candidate-tree" / "docs" / "strategy-plugins.md"
    assert candidate.is_file()
    assert "ZERO evolve proposal: sha256:genesis-accepted-docs" in candidate.read_text(
        encoding="utf-8"
    )


def test_evolve_status_reads_run_artifact(tmp_path) -> None:
    run_evolve(decisions=planned_decisions(), output=tmp_path, repo_root=ROOT, now=FIXED)
    status = status_snapshot(tmp_path / "evolve-run.json", now=FIXED)

    assert status["schema_version"] == "zero.evolve.status.v1"
    assert status["run_present"] is True
    assert status["promotion"]["pushes_to_remote"] is False
    assert status["promotion_plan"]["pushes_to_remote"] is False
    assert status["rollback_plan"]["rollback_ready"] is True
    assert status["promotion_verification"]["ok"] is True


def test_evolve_red_team_blocks_secret_like_patch(tmp_path) -> None:
    patch = tmp_path / "candidate.patch"
    patch.write_text("+private_key = 'redacted'\n", encoding="utf-8")
    build = {
        "generated_at": "2026-05-01T00:00:00Z",
        "proposal_id": "sha256:test",
        "patch_path": str(patch),
        "checks": {"target_paths_allowed": True},
    }

    review = red_team_review(build)

    assert review["verdict"] == "fail"
    assert review["findings"][0]["severity"] == "blocker"


def test_snapshot_from_fixture_is_public_safe(tmp_path, monkeypatch) -> None:
    monkeypatch.setenv("ZERO_EVOLVE_SNAPSHOT_DIR", str(tmp_path))
    snapshot = snapshot_from_fixture(ROOT, now=FIXED)
    body = json.dumps(snapshot, sort_keys=True).lower()

    assert snapshot["schema_version"] == "zero.evolve.snapshot.v1"
    assert snapshot["promotion"]["pushes_to_remote"] is False
    assert snapshot["promotion_plan"]["applies_to_checkout"] is False
    assert snapshot["rollback_plan"]["rollback_ready"] is True
    assert snapshot["promotion_verification"]["ok"] is True
    assert "0x1234567890" not in body
    assert "sk_live_" not in body


def test_snapshot_from_fixture_fails_closed_when_examples_are_not_packaged(
    tmp_path, monkeypatch
) -> None:
    monkeypatch.setenv("ZERO_REPO_ROOT", str(tmp_path / "missing"))
    monkeypatch.chdir(tmp_path)

    snapshot = snapshot_from_fixture(tmp_path / "also-missing", now=FIXED)

    assert snapshot["schema_version"] == "zero.evolve.snapshot.v1"
    assert snapshot["source"] == "fixture-unavailable"
    assert snapshot["promotion"]["pushes_to_remote"] is False
    assert snapshot["promotion"]["promotable_after_human_review"] is False
    assert snapshot["promotion_plan"]["eligible_for_local_apply"] is False
    assert snapshot["rollback_plan"]["rollback_ready"] is False
    assert snapshot["promotion_verification"]["ok"] is False
    assert snapshot["paper_canary"]["status"] == "fixture_unavailable"


def test_load_guardian_decisions_round_trips(tmp_path) -> None:
    payload = run_evolve(decisions=planned_decisions(), output=tmp_path, repo_root=ROOT, now=FIXED)
    assert payload["input_decisions"] == 3

    planned = plan_proposals(load_proposals(PROPOSALS), now=FIXED)
    journal = tmp_path / "genesis.jsonl"
    journal.write_text(
        "".join(json.dumps(item, sort_keys=True) + "\n" for item in planned["decisions"]),
        encoding="utf-8",
    )

    decisions = load_guardian_decisions(journal)

    assert len(decisions) == 3
    assert decisions[0].decision == "accepted"


def test_evolve_policy_forbids_remote_promotion() -> None:
    policy = evolve_policy()

    assert policy["remote_push_allowed"] is False
    assert policy["promotion_is_local_only"] is True
    assert policy["requires_rollback_plan"] is True
    assert policy["requires_promotion_artifact_verification"] is True
