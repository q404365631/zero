from __future__ import annotations

import json
import shutil
from datetime import UTC, datetime
from pathlib import Path

from zero_engine.evolve import (
    apply_promotion,
    candidate_file_hash,
    evolve_policy,
    load_guardian_decisions,
    paths_allowed,
    red_team_review,
    repo_file_hash,
    rollback_promotion,
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


def checkout_with_evolve_targets(tmp_path: Path) -> Path:
    checkout = tmp_path / "checkout"
    for target in ("docs/strategy-plugins.md", "examples/strategy-runner/README.md"):
        source = ROOT / target
        destination = checkout / target
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
    return checkout


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


def test_evolve_apply_and_rollback_are_local_hash_guarded(tmp_path) -> None:
    run_dir = tmp_path / "run"
    payload = run_evolve(decisions=planned_decisions(), output=run_dir, repo_root=ROOT, now=FIXED)
    checkout = checkout_with_evolve_targets(tmp_path)
    target = checkout / "docs" / "strategy-plugins.md"
    original_text = target.read_text(encoding="utf-8")
    candidate_text = Path(payload["promotion_plan"]["mutations"][0]["candidate_path"]).read_text(
        encoding="utf-8"
    )

    apply_receipt = apply_promotion(
        run_path=run_dir / "evolve-run.json",
        repo_root=checkout,
        output=tmp_path / "apply",
        approval_phrase="I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION",
        now=FIXED,
    )

    assert apply_receipt["schema_version"] == "zero.evolve.apply_receipt.v1"
    assert apply_receipt["ok"] is True
    assert apply_receipt["applies_to_checkout"] is True
    assert apply_receipt["pushes_to_remote"] is False
    assert apply_receipt["places_orders"] is False
    assert target.read_text(encoding="utf-8") == candidate_text

    rollback_receipt = rollback_promotion(
        apply_receipt_path=tmp_path / "apply" / "apply-receipt.json",
        repo_root=checkout,
        output=tmp_path / "rollback",
        approval_phrase="I_APPROVE_ZERO_EVOLVE_LOCAL_ROLLBACK",
        now=FIXED,
    )

    assert rollback_receipt["schema_version"] == "zero.evolve.rollback_receipt.v1"
    assert rollback_receipt["ok"] is True
    assert rollback_receipt["applies_to_checkout"] is True
    assert rollback_receipt["pushes_to_remote"] is False
    assert target.read_text(encoding="utf-8") == original_text


def test_evolve_rollback_deletes_files_created_by_local_apply(tmp_path) -> None:
    run_dir = tmp_path / "run"
    payload = run_evolve(decisions=planned_decisions(), output=run_dir, repo_root=ROOT, now=FIXED)
    checkout = checkout_with_evolve_targets(tmp_path)
    target = "docs/evolve-created.md"
    candidate_path = run_dir / "worktree" / "candidate-tree" / target
    candidate_path.parent.mkdir(parents=True, exist_ok=True)
    candidate_path.write_text("# Created by evolve\n", encoding="utf-8")
    payload["promotion_plan"]["mutations"].append(
        {
            "target_path": target,
            "candidate_path": str(candidate_path),
            "original_hash": repo_file_hash(checkout, target),
            "candidate_hash": candidate_file_hash(candidate_path, target),
        }
    )
    (run_dir / "evolve-run.json").write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    apply_receipt = apply_promotion(
        run_path=run_dir / "evolve-run.json",
        repo_root=checkout,
        output=tmp_path / "apply",
        approval_phrase="I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION",
        now=FIXED,
    )

    created = checkout / target
    assert apply_receipt["ok"] is True
    assert apply_receipt["applied"][-1]["existed_before"] is False
    assert created.is_file()

    rollback_receipt = rollback_promotion(
        apply_receipt_path=tmp_path / "apply" / "apply-receipt.json",
        repo_root=checkout,
        output=tmp_path / "rollback",
        approval_phrase="I_APPROVE_ZERO_EVOLVE_LOCAL_ROLLBACK",
        now=FIXED,
    )

    assert rollback_receipt["ok"] is True
    assert not created.exists()


def test_evolve_apply_refuses_wrong_phrase_without_mutating_checkout(tmp_path) -> None:
    run_dir = tmp_path / "run"
    run_evolve(decisions=planned_decisions(), output=run_dir, repo_root=ROOT, now=FIXED)
    checkout = checkout_with_evolve_targets(tmp_path)
    target = checkout / "docs" / "strategy-plugins.md"
    original_text = target.read_text(encoding="utf-8")

    receipt = apply_promotion(
        run_path=run_dir / "evolve-run.json",
        repo_root=checkout,
        output=tmp_path / "apply",
        approval_phrase="wrong",
        now=FIXED,
    )

    assert receipt["ok"] is False
    assert receipt["applies_to_checkout"] is False
    assert receipt["approval_phrase_matched"] is False
    assert target.read_text(encoding="utf-8") == original_text


def test_evolve_apply_prevalidates_all_candidates_before_mutating(tmp_path) -> None:
    run_dir = tmp_path / "run"
    payload = run_evolve(decisions=planned_decisions(), output=run_dir, repo_root=ROOT, now=FIXED)
    checkout = checkout_with_evolve_targets(tmp_path)
    first_target = checkout / "docs" / "strategy-plugins.md"
    original_first = first_target.read_text(encoding="utf-8")
    second_candidate = Path(payload["promotion_plan"]["mutations"][1]["candidate_path"])
    second_candidate.write_text("tampered\n", encoding="utf-8")

    receipt = apply_promotion(
        run_path=run_dir / "evolve-run.json",
        repo_root=checkout,
        output=tmp_path / "apply",
        approval_phrase="I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION",
        now=FIXED,
    )

    assert receipt["ok"] is False
    assert receipt["applied"] == []
    assert "candidate_hash_matches" in ",".join(receipt["failures"])
    assert first_target.read_text(encoding="utf-8") == original_first


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
    assert policy["requires_apply_receipt"] is True
    assert policy["requires_rollback_receipt"] is True
    assert policy["local_apply_allowed_after_human_approval"] is True
