from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from zero_engine import PaperEngine, load_scenario
from zero_engine.genesis import GuardianDecision, Proposal, load_proposals, plan_proposals
from zero_engine.memory import isoformat, parse_datetime, stable_hash

JsonMap = dict[str, Any]

EVOLVE_RUN_SCHEMA_VERSION = "zero.evolve.run.v1"
EVOLVE_STATUS_SCHEMA_VERSION = "zero.evolve.status.v1"
EVOLVE_SNAPSHOT_SCHEMA_VERSION = "zero.evolve.snapshot.v1"
EVOLVE_POLICY_VERSION = "zero.evolve.policy.v1"
EVOLVE_PROMOTION_PLAN_SCHEMA_VERSION = "zero.evolve.promotion_plan.v1"
EVOLVE_ROLLBACK_PLAN_SCHEMA_VERSION = "zero.evolve.rollback_plan.v1"
EVOLVE_PROMOTION_VERIFICATION_SCHEMA_VERSION = "zero.evolve.promotion_verification.v1"
EVOLVE_APPLY_RECEIPT_SCHEMA_VERSION = "zero.evolve.apply_receipt.v1"
EVOLVE_ROLLBACK_RECEIPT_SCHEMA_VERSION = "zero.evolve.rollback_receipt.v1"
PROMOTION_APPROVAL_PHRASE = "I_APPROVE_ZERO_EVOLVE_LOCAL_PROMOTION"
ROLLBACK_APPROVAL_PHRASE = "I_APPROVE_ZERO_EVOLVE_LOCAL_ROLLBACK"
ALLOWED_PATCH_ROOTS = ("docs/", "examples/")
FORBIDDEN_PATCH_ROOTS = (
    "engine/src/zero_engine/live.py",
    "engine/src/zero_engine/hyperliquid.py",
    "engine/src/zero_engine/immune.py",
    "engine/src/zero_engine/safety.py",
    "cli/crates/zero-commands/src/dispatch.rs",
)
SENSITIVE_TEXT_RE = re.compile(r"(?:0x[a-fA-F0-9]{32,}|[A-Za-z0-9_=-]{40,}|sk-[A-Za-z0-9_-]{20,})")


def utc_now() -> datetime:
    return datetime.now(UTC)


def load_guardian_decisions(path: str | Path) -> list[GuardianDecision]:
    target = Path(path)
    if not target.exists():
        return []
    lines = target.read_text(encoding="utf-8").splitlines()
    return [GuardianDecision.from_dict(json.loads(line)) for line in lines if line.strip()]


def proposal_slug(proposal: Proposal) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", proposal.title.lower()).strip("-")
    return slug[:48] or "proposal"


def paths_allowed(paths: Iterable[str]) -> bool:
    for path in paths:
        normalized = path.lstrip("/")
        if any(normalized.startswith(root) for root in FORBIDDEN_PATCH_ROOTS):
            return False
        if not any(normalized.startswith(root) for root in ALLOWED_PATCH_ROOTS):
            return False
    return True


@dataclass(frozen=True)
class BuildArtifact:
    proposal: Proposal
    sandbox_dir: Path
    repo_root: Path
    generated_at: datetime

    @property
    def branch_name(self) -> str:
        short_hash = self.proposal.id.replace("sha256:", "")[:12]
        return f"codex/evolve/{proposal_slug(self.proposal)}-{short_hash}"

    @property
    def patch_text(self) -> str:
        target = self.proposal.target_paths[0]
        return "\n".join(
            [
                f"diff --git a/{target} b/{target}",
                f"--- a/{target}",
                f"+++ b/{target}",
                "@@",
                f"+<!-- ZERO evolve proposal: {self.proposal.id} -->",
                "",
            ]
        )

    def write(self) -> JsonMap:
        self.sandbox_dir.mkdir(parents=True, exist_ok=True)
        patch_path = self.sandbox_dir / "candidate.patch"
        patch_path.write_text(self.patch_text, encoding="utf-8")
        candidate_tree = self.sandbox_dir / "candidate-tree"
        mutations = materialize_candidate_tree(
            repo_root=self.repo_root,
            target_paths=self.proposal.target_paths,
            candidate_tree=candidate_tree,
            marker=f"<!-- ZERO evolve proposal: {self.proposal.id} -->",
        )
        manifest = {
            "schema_version": "zero.evolve.build.v1",
            "generated_at": isoformat(self.generated_at),
            "mode": "paper-only",
            "branch_name": self.branch_name,
            "sandbox_dir": str(self.sandbox_dir),
            "candidate_tree": str(candidate_tree),
            "applies_to_checkout": False,
            "mutates_sandbox": True,
            "pushes_to_remote": False,
            "proposal_id": self.proposal.id,
            "target_paths": list(self.proposal.target_paths),
            "patch_path": str(patch_path),
            "patch_hash": stable_hash({"patch": self.patch_text}),
            "mutations": mutations,
            "checks": {
                "target_paths_allowed": paths_allowed(self.proposal.target_paths),
                "proposal_accepted": True,
                "protected_classes_empty": not self.proposal.protected_classes,
                "candidate_tree_materialized": bool(mutations),
            },
        }
        (self.sandbox_dir / "build.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        return manifest


def materialize_candidate_tree(
    *,
    repo_root: Path,
    target_paths: Iterable[str],
    candidate_tree: Path,
    marker: str,
) -> list[JsonMap]:
    mutations: list[JsonMap] = []
    candidate_tree.mkdir(parents=True, exist_ok=True)
    for raw_target in target_paths:
        target = raw_target.lstrip("/")
        if not paths_allowed([target]):
            continue
        source_path = repo_root / target
        original_text = source_path.read_text(encoding="utf-8") if source_path.is_file() else ""
        candidate_text = original_text.rstrip() + "\n\n" + marker + "\n"
        candidate_path = candidate_tree / target
        candidate_path.parent.mkdir(parents=True, exist_ok=True)
        candidate_path.write_text(candidate_text, encoding="utf-8")
        mutations.append(
            {
                "target_path": target,
                "source_path": target,
                "candidate_path": str(candidate_path),
                "original_hash": stable_hash({"path": target, "content": original_text}),
                "candidate_hash": stable_hash({"path": target, "content": candidate_text}),
                "operation": "append_public_evolve_marker",
                "applies_to_checkout": False,
            }
        )
    return mutations


def red_team_review(build: JsonMap) -> JsonMap:
    patch_path = Path(str(build["patch_path"]))
    patch_text = patch_path.read_text(encoding="utf-8")
    findings: list[JsonMap] = []
    if not build["checks"]["target_paths_allowed"]:
        findings.append({"severity": "blocker", "reason": "target path outside allowed roots"})
    if SENSITIVE_TEXT_RE.search(patch_text):
        findings.append({"severity": "blocker", "reason": "patch contains secret-like material"})
    if any(token in patch_text.lower() for token in ("private_key", "wallet_address", "order_id")):
        findings.append({"severity": "blocker", "reason": "patch contains forbidden private field"})
    verdict = "pass" if not findings else "fail"
    return {
        "schema_version": "zero.evolve.red_team.v1",
        "generated_at": build["generated_at"],
        "mode": "paper-only",
        "proposal_id": build["proposal_id"],
        "verdict": verdict,
        "findings": findings,
        "policy": {
            "forbidden_private_material": True,
            "protected_paths_blocked": True,
            "remote_push_blocked": True,
        },
    }


def run_paper_canary(repo_root: Path, *, generated_at: datetime) -> JsonMap:
    scenario = load_scenario(repo_root / "examples" / "paper-trading" / "scenario.json")
    engine = PaperEngine(limits=scenario.limits, clock=lambda: generated_at.timestamp())
    for order in scenario.orders:
        engine.submit(order, source=f"evolve-canary:{scenario.name}")
    return {
        "schema_version": "zero.evolve.paper_canary.v1",
        "generated_at": isoformat(generated_at),
        "mode": "paper",
        "scenario": scenario.name,
        "decisions": len(engine.decisions),
        "fills": len(engine.fills),
        "rejections": len(engine.rejections),
        "open_positions": len(
            [position for position in engine.positions.values() if position.quantity != 0]
        ),
        "baseline": {
            "decisions": 4,
            "fills": 2,
            "rejections": 2,
            "open_positions": 1,
        },
    }


def calibrate(canary: Mapping[str, Any], *, generated_at: datetime) -> JsonMap:
    baseline = canary["baseline"]
    drift = {
        key: int(canary[key]) - int(baseline[key])
        for key in ("decisions", "fills", "rejections", "open_positions")
    }
    passed = all(value == 0 for value in drift.values())
    return {
        "schema_version": "zero.evolve.calibration.v1",
        "generated_at": isoformat(generated_at),
        "mode": "paper",
        "passed": passed,
        "drift": drift,
        "gate": "zero-drift-against-deterministic-paper-baseline",
    }


def promotion_decision(
    *,
    build: Mapping[str, Any] | None,
    red_team: Mapping[str, Any] | None,
    canary: Mapping[str, Any],
    calibration: Mapping[str, Any],
    generated_at: datetime,
) -> JsonMap:
    gates = {
        "build": bool(build and build["checks"]["target_paths_allowed"]),
        "red_team": bool(red_team and red_team["verdict"] == "pass"),
        "paper_canary": bool(canary["fills"] == canary["baseline"]["fills"]),
        "calibration": bool(calibration["passed"]),
        "human_approval": False,
    }
    promotable = all(value for key, value in gates.items() if key != "human_approval")
    return {
        "schema_version": "zero.evolve.promotion.v1",
        "generated_at": isoformat(generated_at),
        "mode": "local-only",
        "promotable_after_human_review": promotable,
        "promoted": False,
        "pushes_to_remote": False,
        "requires_human_approval": True,
        "gates": gates,
        "reason": (
            "all automated paper gates passed; human approval still required"
            if promotable
            else "one or more automated paper gates failed"
        ),
    }


def build_promotion_plan(
    *,
    build: Mapping[str, Any] | None,
    red_team: Mapping[str, Any] | None,
    canary: Mapping[str, Any],
    calibration: Mapping[str, Any],
    promotion: Mapping[str, Any],
    generated_at: datetime,
) -> JsonMap:
    eligible = bool(promotion.get("promotable_after_human_review"))
    return {
        "schema_version": EVOLVE_PROMOTION_PLAN_SCHEMA_VERSION,
        "generated_at": isoformat(generated_at),
        "mode": "local-sandbox",
        "plan_only": True,
        "eligible_for_local_apply": eligible,
        "applies_to_checkout": False,
        "pushes_to_remote": False,
        "places_orders": False,
        "requires_human_approval": True,
        "required_approval_phrase": PROMOTION_APPROVAL_PHRASE,
        "branch_name": None if build is None else build.get("branch_name"),
        "patch_hash": None if build is None else build.get("patch_hash"),
        "candidate_tree": None if build is None else build.get("candidate_tree"),
        "mutations": [] if build is None else list(build.get("mutations", [])),
        "gates": {
            "build": bool(build and build.get("checks", {}).get("candidate_tree_materialized")),
            "red_team": bool(red_team and red_team.get("verdict") == "pass"),
            "paper_canary": bool(canary.get("fills") == canary.get("baseline", {}).get("fills")),
            "calibration": bool(calibration.get("passed")),
            "rollback_plan_required": True,
            "human_approval": False,
        },
        "safety": {
            "checkout_mutation_default": "forbidden",
            "remote_push_default": "forbidden",
            "live_code_mutation_default": "forbidden",
            "candidate_scope": list(ALLOWED_PATCH_ROOTS),
        },
    }


def build_rollback_plan(
    *,
    build: Mapping[str, Any] | None,
    promotion_plan: Mapping[str, Any],
    generated_at: datetime,
) -> JsonMap:
    mutations = [] if build is None else list(build.get("mutations", []))
    restores = [
        {
            "target_path": mutation["target_path"],
            "restore_hash": mutation["original_hash"],
            "candidate_hash": mutation["candidate_hash"],
            "action": "discard_candidate_and_restore_original_before_checkout_apply",
        }
        for mutation in mutations
        if isinstance(mutation, dict)
    ]
    rollback_hash = stable_hash(
        {"restores": restores, "patch_hash": promotion_plan.get("patch_hash")}
    )
    return {
        "schema_version": EVOLVE_ROLLBACK_PLAN_SCHEMA_VERSION,
        "generated_at": isoformat(generated_at),
        "mode": "local-sandbox",
        "plan_only": True,
        "rollback_ready": bool(restores),
        "applies_to_checkout": False,
        "pushes_to_remote": False,
        "restores": restores,
        "rollback_hash": rollback_hash,
        "instructions": [
            "do not promote if rollback_ready is false",
            "verify original hashes before any local apply",
            "discard candidate tree to abandon the proposal",
            "rerun paper canary and calibration after any approved local apply",
        ],
    }


def verify_promotion_artifacts(
    *,
    promotion_plan: Mapping[str, Any],
    rollback_plan: Mapping[str, Any],
    generated_at: datetime,
) -> JsonMap:
    checks = {
        "promotion_schema": promotion_plan.get("schema_version")
        == EVOLVE_PROMOTION_PLAN_SCHEMA_VERSION,
        "rollback_schema": rollback_plan.get("schema_version")
        == EVOLVE_ROLLBACK_PLAN_SCHEMA_VERSION,
        "promotion_does_not_apply_checkout": promotion_plan.get("applies_to_checkout") is False,
        "rollback_does_not_apply_checkout": rollback_plan.get("applies_to_checkout") is False,
        "promotion_does_not_push": promotion_plan.get("pushes_to_remote") is False,
        "rollback_does_not_push": rollback_plan.get("pushes_to_remote") is False,
        "approval_phrase_required": promotion_plan.get("required_approval_phrase")
        == PROMOTION_APPROVAL_PHRASE,
        "rollback_ready": rollback_plan.get("rollback_ready") is True,
    }
    ok = all(checks.values())
    return {
        "schema_version": EVOLVE_PROMOTION_VERIFICATION_SCHEMA_VERSION,
        "generated_at": isoformat(generated_at),
        "ok": ok,
        "checks": checks,
        "failures": [name for name, passed in checks.items() if not passed],
    }


def load_run(path: str | Path) -> JsonMap:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def repo_file_hash(repo_root: Path, target: str) -> str:
    path = repo_root / target
    content = path.read_text(encoding="utf-8") if path.is_file() else ""
    return stable_hash({"path": target, "content": content})


def candidate_file_hash(candidate_path: Path, target: str) -> str:
    content = candidate_path.read_text(encoding="utf-8")
    return stable_hash({"path": target, "content": content})


def apply_promotion(
    *,
    run_path: str | Path,
    repo_root: str | Path,
    output: str | Path,
    approval_phrase: str,
    now: datetime | None = None,
) -> JsonMap:
    now = now or utc_now()
    run = load_run(run_path)
    repo = Path(repo_root)
    output_path = Path(output)
    output_path.mkdir(parents=True, exist_ok=True)
    backup_dir = output_path / "backup"
    backup_dir.mkdir(parents=True, exist_ok=True)

    promotion_plan = run.get("promotion_plan") or {}
    rollback_plan = run.get("rollback_plan") or {}
    verification = run.get("promotion_verification") or {}
    mutations = list(promotion_plan.get("mutations", []))
    checks: JsonMap = {
        "run_schema": run.get("schema_version") == EVOLVE_RUN_SCHEMA_VERSION,
        "approval_phrase": approval_phrase == PROMOTION_APPROVAL_PHRASE,
        "promotion_verification_ok": verification.get("ok") is True,
        "eligible_for_local_apply": promotion_plan.get("eligible_for_local_apply") is True,
        "rollback_ready": rollback_plan.get("rollback_ready") is True,
        "does_not_push_remote": run.get("pushes_to_remote") is False
        and promotion_plan.get("pushes_to_remote") is False,
        "does_not_place_orders": promotion_plan.get("places_orders") is False,
        "paths_allowed": all(
            isinstance(mutation, dict) and paths_allowed([str(mutation.get("target_path", ""))])
            for mutation in mutations
        ),
    }
    failures = [name for name, passed in checks.items() if not passed]
    applied: list[JsonMap] = []
    prepared: list[JsonMap] = []

    if not failures:
        for mutation in mutations:
            target = str(mutation["target_path"])
            candidate_path = Path(str(mutation["candidate_path"]))
            current_hash = repo_file_hash(repo, target)
            expected_original_hash = str(mutation["original_hash"])
            candidate_exists = candidate_path.is_file()
            observed_candidate_hash = (
                candidate_file_hash(candidate_path, target) if candidate_exists else None
            )
            expected_candidate_hash = str(mutation["candidate_hash"])
            mutation_checks = {
                "current_hash_matches_original": current_hash == expected_original_hash,
                "candidate_hash_matches": observed_candidate_hash == expected_candidate_hash,
                "candidate_exists": candidate_exists,
                "target_path_allowed": paths_allowed([target]),
            }
            failed_mutation_checks = [
                name for name, passed in mutation_checks.items() if not passed
            ]
            if failed_mutation_checks:
                failures.append(f"{target}: {','.join(failed_mutation_checks)}")
                break
            prepared.append(
                {
                    "target_path": target,
                    "candidate_path": candidate_path,
                    "candidate_hash": expected_candidate_hash,
                    "original_hash": expected_original_hash,
                    "existed_before": (repo / target).is_file(),
                    "checks": mutation_checks,
                }
            )

    if not failures:
        for item in prepared:
            target = str(item["target_path"])
            candidate_path = Path(item["candidate_path"])
            target_path = repo / target
            backup_path = backup_dir / target
            backup_path.parent.mkdir(parents=True, exist_ok=True)
            if item["existed_before"]:
                shutil.copyfile(target_path, backup_path)
            target_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(candidate_path, target_path)
            applied.append(
                {
                    "target_path": target,
                    "backup_path": str(backup_path),
                    "applied_hash": item["candidate_hash"],
                    "original_hash": item["original_hash"],
                    "candidate_hash": item["candidate_hash"],
                    "existed_before": item["existed_before"],
                    "checks": item["checks"],
                }
            )

    ok = not failures
    receipt = {
        "schema_version": EVOLVE_APPLY_RECEIPT_SCHEMA_VERSION,
        "generated_at": isoformat(now),
        "mode": "local-checkout",
        "ok": ok,
        "applies_to_checkout": ok,
        "pushes_to_remote": False,
        "places_orders": False,
        "run_path": str(run_path),
        "repo_root": str(repo),
        "approval_phrase_matched": approval_phrase == PROMOTION_APPROVAL_PHRASE,
        "checks": checks,
        "failures": failures,
        "applied": applied if ok else [],
        "rollback_receipt_required": ok,
        "rollback_approval_phrase": ROLLBACK_APPROVAL_PHRASE,
    }
    (output_path / "apply-receipt.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return receipt


def rollback_promotion(
    *,
    apply_receipt_path: str | Path,
    repo_root: str | Path,
    output: str | Path,
    approval_phrase: str,
    now: datetime | None = None,
) -> JsonMap:
    now = now or utc_now()
    apply_receipt = load_run(apply_receipt_path)
    repo = Path(repo_root)
    output_path = Path(output)
    output_path.mkdir(parents=True, exist_ok=True)
    applied = list(apply_receipt.get("applied", []))
    checks: JsonMap = {
        "apply_receipt_schema": apply_receipt.get("schema_version")
        == EVOLVE_APPLY_RECEIPT_SCHEMA_VERSION,
        "apply_receipt_ok": apply_receipt.get("ok") is True,
        "approval_phrase": approval_phrase == ROLLBACK_APPROVAL_PHRASE,
        "does_not_push_remote": apply_receipt.get("pushes_to_remote") is False,
        "paths_allowed": all(
            isinstance(item, dict) and paths_allowed([str(item.get("target_path", ""))])
            for item in applied
        ),
    }
    failures = [name for name, passed in checks.items() if not passed]
    restored: list[JsonMap] = []

    if not failures:
        for item in applied:
            target = str(item["target_path"])
            target_path = repo / target
            backup_path = Path(str(item["backup_path"]))
            current_hash = repo_file_hash(repo, target)
            if current_hash != item["candidate_hash"]:
                failures.append(f"{target}: current_hash_does_not_match_applied_candidate")
                break
            existed_before = item.get("existed_before", True)
            if existed_before and not backup_path.is_file():
                failures.append(f"{target}: backup_missing")
                break
            if existed_before:
                target_path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(backup_path, target_path)
            elif target_path.exists():
                target_path.unlink()
            restored_hash = repo_file_hash(repo, target)
            if restored_hash != item["original_hash"]:
                failures.append(f"{target}: restored_hash_does_not_match_original")
                break
            restored.append(
                {
                    "target_path": target,
                    "restored_hash": restored_hash,
                    "expected_original_hash": item["original_hash"],
                }
            )

    ok = not failures
    receipt = {
        "schema_version": EVOLVE_ROLLBACK_RECEIPT_SCHEMA_VERSION,
        "generated_at": isoformat(now),
        "mode": "local-checkout",
        "ok": ok,
        "applies_to_checkout": ok,
        "pushes_to_remote": False,
        "places_orders": False,
        "apply_receipt_path": str(apply_receipt_path),
        "repo_root": str(repo),
        "approval_phrase_matched": approval_phrase == ROLLBACK_APPROVAL_PHRASE,
        "checks": checks,
        "failures": failures,
        "restored": restored if ok else [],
    }
    (output_path / "rollback-receipt.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return receipt


def run_evolve(
    *,
    decisions: Iterable[GuardianDecision],
    output: str | Path,
    repo_root: str | Path = ".",
    now: datetime | None = None,
) -> JsonMap:
    now = now or utc_now()
    root = Path(repo_root)
    output_path = Path(output)
    output_path.mkdir(parents=True, exist_ok=True)
    decision_items = list(decisions)
    accepted = [
        decision.proposal
        for decision in decision_items
        if decision.decision == "accepted"
        and not decision.required_human_review
        and paths_allowed(decision.proposal.target_paths)
    ]
    selected = accepted[0] if accepted else None
    build = None
    red_team = None
    if selected is not None:
        artifact = BuildArtifact(
            proposal=selected,
            sandbox_dir=output_path / "worktree",
            repo_root=root,
            generated_at=now,
        )
        build = artifact.write()
        red_team = red_team_review(build)
        (output_path / "red_team.json").write_text(
            json.dumps(red_team, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    canary = run_paper_canary(root, generated_at=now)
    calibration = calibrate(canary, generated_at=now)
    promotion = promotion_decision(
        build=build,
        red_team=red_team,
        canary=canary,
        calibration=calibration,
        generated_at=now,
    )
    promotion_plan = build_promotion_plan(
        build=build,
        red_team=red_team,
        canary=canary,
        calibration=calibration,
        promotion=promotion,
        generated_at=now,
    )
    rollback_plan = build_rollback_plan(
        build=build,
        promotion_plan=promotion_plan,
        generated_at=now,
    )
    promotion_verification = verify_promotion_artifacts(
        promotion_plan=promotion_plan,
        rollback_plan=rollback_plan,
        generated_at=now,
    )
    payload = {
        "schema_version": EVOLVE_RUN_SCHEMA_VERSION,
        "generated_at": isoformat(now),
        "mode": "paper-only",
        "applies_to_checkout": False,
        "pushes_to_remote": False,
        "input_decisions": len(decision_items),
        "selected_proposal_id": selected.id if selected else None,
        "accepted_candidates": len(accepted),
        "build": build,
        "red_team": red_team,
        "paper_canary": canary,
        "calibration": calibration,
        "promotion": promotion,
        "promotion_plan": promotion_plan,
        "rollback_plan": rollback_plan,
        "promotion_verification": promotion_verification,
        "policy": evolve_policy(),
    }
    (output_path / "evolve-run.json").write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return payload


def evolve_policy() -> JsonMap:
    return {
        "schema_version": EVOLVE_POLICY_VERSION,
        "allowed_patch_roots": list(ALLOWED_PATCH_ROOTS),
        "forbidden_patch_roots": list(FORBIDDEN_PATCH_ROOTS),
        "requires_accepted_genesis_decision": True,
        "requires_red_team_pass": True,
        "requires_paper_canary": True,
        "requires_calibration_pass": True,
        "requires_rollback_plan": True,
        "requires_promotion_artifact_verification": True,
        "requires_apply_receipt": True,
        "requires_rollback_receipt": True,
        "promotion_is_local_only": True,
        "local_apply_allowed_after_human_approval": True,
        "remote_push_allowed": False,
    }


def status_snapshot(run_path: str | Path, *, now: datetime | None = None) -> JsonMap:
    now = now or utc_now()
    path = Path(run_path)
    run = json.loads(path.read_text(encoding="utf-8")) if path.exists() else None
    return {
        "schema_version": EVOLVE_STATUS_SCHEMA_VERSION,
        "generated_at": isoformat(now),
        "source": str(path),
        "run_present": run is not None,
        "mode": run.get("mode") if run else "missing",
        "promotion": run.get("promotion") if run else None,
        "promotion_plan": run.get("promotion_plan") if run else None,
        "rollback_plan": run.get("rollback_plan") if run else None,
        "promotion_verification": run.get("promotion_verification") if run else None,
        "policy": evolve_policy(),
    }


def fixture_root(repo_root: str | Path) -> Path | None:
    candidates: list[Path] = []
    env_root = os.environ.get("ZERO_REPO_ROOT")
    if env_root:
        candidates.append(Path(env_root))
    for candidate in (Path(repo_root), Path.cwd(), Path("/app")):
        candidates.append(candidate)
        candidates.append(candidate.parent)
    for candidate in candidates:
        if (candidate / "examples" / "genesis" / "proposals.jsonl").is_file() and (
            candidate / "examples" / "paper-trading" / "scenario.json"
        ).is_file():
            return candidate
    return None


def snapshot_from_fixture(repo_root: str | Path, *, now: datetime | None = None) -> JsonMap:
    now = now or utc_now()
    root = fixture_root(repo_root)
    if root is None:
        canary = {
            "schema_version": "zero.evolve.paper_canary.v1",
            "generated_at": isoformat(now),
            "mode": "paper",
            "scenario": None,
            "decisions": 0,
            "fills": 0,
            "rejections": 0,
            "open_positions": 0,
            "baseline": {
                "decisions": 4,
                "fills": 2,
                "rejections": 2,
                "open_positions": 1,
            },
            "status": "fixture_unavailable",
        }
        calibration = calibrate(canary, generated_at=now)
        promotion = promotion_decision(
            build=None,
            red_team=None,
            canary=canary,
            calibration=calibration,
            generated_at=now,
        )
        promotion_plan = build_promotion_plan(
            build=None,
            red_team=None,
            canary=canary,
            calibration=calibration,
            promotion=promotion,
            generated_at=now,
        )
        rollback_plan = build_rollback_plan(
            build=None,
            promotion_plan=promotion_plan,
            generated_at=now,
        )
        promotion_verification = verify_promotion_artifacts(
            promotion_plan=promotion_plan,
            rollback_plan=rollback_plan,
            generated_at=now,
        )
        return {
            "schema_version": EVOLVE_SNAPSHOT_SCHEMA_VERSION,
            "generated_at": isoformat(now),
            "mode": "paper-only",
            "applies_to_checkout": False,
            "pushes_to_remote": False,
            "input_decisions": 0,
            "selected_proposal_id": None,
            "accepted_candidates": 0,
            "build": None,
            "red_team": None,
            "paper_canary": canary,
            "calibration": calibration,
            "promotion": promotion,
            "promotion_plan": promotion_plan,
            "rollback_plan": rollback_plan,
            "promotion_verification": promotion_verification,
            "policy": evolve_policy(),
            "source": "fixture-unavailable",
        }
    proposals = load_proposals(root / "examples" / "genesis" / "proposals.jsonl")
    planned = plan_proposals(proposals, now=now)
    decisions = [GuardianDecision.from_dict(item) for item in planned["decisions"]]
    sandbox = Path(os.environ.get("ZERO_EVOLVE_SNAPSHOT_DIR", "/tmp/zero-evolve-snapshot"))
    payload = run_evolve(decisions=decisions, output=sandbox, repo_root=root, now=now)
    return {
        **payload,
        "schema_version": EVOLVE_SNAPSHOT_SCHEMA_VERSION,
        "source": "fixture-genesis-proposals",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="ZERO paper-only evolve harness")
    subcommands = parser.add_subparsers(dest="command", required=True)

    run = subcommands.add_parser("run", help="run build/red-team/canary/calibration gates")
    run.add_argument("--genesis-journal", help="append-only genesis journal")
    run.add_argument(
        "--proposals", help="proposal JSONL input; planned ephemerally if journal is omitted"
    )
    run.add_argument("--output", required=True, help="evolve artifact directory")
    run.add_argument("--repo-root", default=".", help="ZERO source checkout")
    run.add_argument("--now", help="UTC timestamp override for deterministic runs")

    status = subcommands.add_parser("status", help="print evolve run status")
    status.add_argument("--run", required=True, help="evolve-run.json path")
    status.add_argument("--now", help="UTC timestamp override for deterministic runs")

    apply_cmd = subcommands.add_parser(
        "apply", help="apply a verified evolve candidate to the local checkout"
    )
    apply_cmd.add_argument("--run", required=True, help="evolve-run.json path")
    apply_cmd.add_argument("--repo-root", default=".", help="ZERO source checkout")
    apply_cmd.add_argument("--output", required=True, help="apply receipt directory")
    apply_cmd.add_argument("--approval-phrase", required=True, help="exact local apply phrase")
    apply_cmd.add_argument("--now", help="UTC timestamp override for deterministic runs")

    rollback_cmd = subcommands.add_parser(
        "rollback", help="restore files from a local evolve apply receipt"
    )
    rollback_cmd.add_argument("--apply-receipt", required=True, help="apply-receipt.json path")
    rollback_cmd.add_argument("--repo-root", default=".", help="ZERO source checkout")
    rollback_cmd.add_argument("--output", required=True, help="rollback receipt directory")
    rollback_cmd.add_argument(
        "--approval-phrase", required=True, help="exact local rollback phrase"
    )
    rollback_cmd.add_argument("--now", help="UTC timestamp override for deterministic runs")

    args = parser.parse_args(argv)
    now = parse_datetime(args.now) if getattr(args, "now", None) else utc_now()

    if args.command == "run":
        if args.genesis_journal:
            decisions = load_guardian_decisions(args.genesis_journal)
        elif args.proposals:
            planned = plan_proposals(load_proposals(args.proposals), now=now)
            decisions = [GuardianDecision.from_dict(item) for item in planned["decisions"]]
        else:
            print("zero-evolve run requires --genesis-journal or --proposals", file=sys.stderr)
            return 2
        print(
            json.dumps(
                run_evolve(
                    decisions=decisions,
                    output=args.output,
                    repo_root=args.repo_root,
                    now=now,
                ),
                indent=2,
                sort_keys=True,
            )
        )
        return 0

    if args.command == "status":
        print(json.dumps(status_snapshot(args.run, now=now), indent=2, sort_keys=True))
        return 0

    if args.command == "apply":
        receipt = apply_promotion(
            run_path=args.run,
            repo_root=args.repo_root,
            output=args.output,
            approval_phrase=args.approval_phrase,
            now=now,
        )
        print(json.dumps(receipt, indent=2, sort_keys=True))
        return 0 if receipt["ok"] else 1

    if args.command == "rollback":
        receipt = rollback_promotion(
            apply_receipt_path=args.apply_receipt,
            repo_root=args.repo_root,
            output=args.output,
            approval_phrase=args.approval_phrase,
            now=now,
        )
        print(json.dumps(receipt, indent=2, sort_keys=True))
        return 0 if receipt["ok"] else 1

    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
