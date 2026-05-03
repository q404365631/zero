from __future__ import annotations

import argparse
import json
import os
import re
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
ALLOWED_PATCH_ROOTS = ("docs/", "examples/")
FORBIDDEN_PATCH_ROOTS = (
    "engine/src/zero_engine/live.py",
    "engine/src/zero_engine/hyperliquid.py",
    "engine/src/zero_engine/immune.py",
    "engine/src/zero_engine/safety.py",
    "cli/crates/zero-commands/src/dispatch.rs",
)
SENSITIVE_TEXT_RE = re.compile(
    r"(?:0x[a-fA-F0-9]{32,}|[A-Za-z0-9_=-]{40,}|sk-[A-Za-z0-9_-]{20,})"
)


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
        manifest = {
            "schema_version": "zero.evolve.build.v1",
            "generated_at": isoformat(self.generated_at),
            "mode": "paper-only",
            "branch_name": self.branch_name,
            "sandbox_dir": str(self.sandbox_dir),
            "applies_to_checkout": False,
            "pushes_to_remote": False,
            "proposal_id": self.proposal.id,
            "target_paths": list(self.proposal.target_paths),
            "patch_path": str(patch_path),
            "patch_hash": stable_hash({"patch": self.patch_text}),
            "checks": {
                "target_paths_allowed": paths_allowed(self.proposal.target_paths),
                "proposal_accepted": True,
                "protected_classes_empty": not self.proposal.protected_classes,
            },
        }
        (self.sandbox_dir / "build.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        return manifest


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
        "open_positions": len([position for position in engine.positions.values() if position.quantity != 0]),
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
        "promotion_is_local_only": True,
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
        if (
            (candidate / "examples" / "genesis" / "proposals.jsonl").is_file()
            and (candidate / "examples" / "paper-trading" / "scenario.json").is_file()
        ):
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
    run.add_argument("--proposals", help="proposal JSONL input; planned ephemerally if journal is omitted")
    run.add_argument("--output", required=True, help="evolve artifact directory")
    run.add_argument("--repo-root", default=".", help="ZERO source checkout")
    run.add_argument("--now", help="UTC timestamp override for deterministic runs")

    status = subcommands.add_parser("status", help="print evolve run status")
    status.add_argument("--run", required=True, help="evolve-run.json path")
    status.add_argument("--now", help="UTC timestamp override for deterministic runs")

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

    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
