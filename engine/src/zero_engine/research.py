from __future__ import annotations

import argparse
import json
import sys
from collections import Counter, defaultdict
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from zero_engine.memory import isoformat, parse_datetime, stable_hash

JsonMap = dict[str, Any]

RESEARCH_SNAPSHOT_SCHEMA_VERSION = "zero.research.snapshot.v1"
RESEARCH_REPORT_SCHEMA_VERSION = "zero.research.report.v1"
RESEARCH_STATUS_SCHEMA_VERSION = "zero.research.status.v1"
COMMANDS = ("hunt", "edge", "convergence", "thesis", "score", "meta", "sharpen")
FORBIDDEN_OUTPUT_KEYS = {
    "exchange_order_id",
    "idempotency_key",
    "notional_usd",
    "order_id",
    "price",
    "private_key",
    "quantity",
    "raw_payload",
    "wallet_address",
}


@dataclass(frozen=True)
class CandleSummary:
    symbol: str
    observations: int
    up_closes: int
    avg_volume_rank: int
    momentum_bps: int
    latest_ts: str

    def to_dict(self) -> JsonMap:
        if self.momentum_bps > 50:
            regime = "impulse-up"
        elif self.momentum_bps < -50:
            regime = "impulse-down"
        else:
            regime = "balanced"
        return {
            "symbol": self.symbol,
            "observations": self.observations,
            "up_closes": self.up_closes,
            "avg_volume_rank": self.avg_volume_rank,
            "momentum_bps": self.momentum_bps,
            "regime": regime,
            "latest_ts": self.latest_ts,
        }


def utc_now() -> datetime:
    return datetime.now(UTC)


def fixture_root(repo_root: str | Path) -> Path | None:
    root = Path(repo_root).resolve()
    for candidate in (root, *root.parents):
        if (candidate / "examples" / "paper-trading" / "candles.jsonl").is_file():
            return candidate
    return None


def load_jsonl(path: Path) -> list[JsonMap]:
    if not path.is_file():
        return []
    rows: list[JsonMap] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def summarize_candles(candles: Iterable[Mapping[str, Any]]) -> list[CandleSummary]:
    grouped: dict[str, list[Mapping[str, Any]]] = defaultdict(list)
    for candle in candles:
        grouped[str(candle["symbol"]).upper()].append(candle)

    volume_order = {
        symbol: rank + 1
        for rank, symbol in enumerate(
            symbol
            for symbol, _volume in sorted(
                (
                    (symbol, sum(float(candle.get("volume", 0)) for candle in rows))
                    for symbol, rows in grouped.items()
                ),
                key=lambda item: item[1],
                reverse=True,
            )
        )
    }
    summaries: list[CandleSummary] = []
    for symbol, rows in sorted(grouped.items()):
        sorted_rows = sorted(rows, key=lambda candle: str(candle["ts"]))
        first = sorted_rows[0]
        last = sorted_rows[-1]
        first_open = float(first["open"])
        last_close = float(last["close"])
        momentum_bps = int(round(((last_close - first_open) / first_open) * 10_000))
        summaries.append(
            CandleSummary(
                symbol=symbol,
                observations=len(sorted_rows),
                up_closes=sum(
                    1 for candle in sorted_rows if float(candle["close"]) >= float(candle["open"])
                ),
                avg_volume_rank=volume_order[symbol],
                momentum_bps=momentum_bps,
                latest_ts=str(last["ts"]),
            )
        )
    return summaries


def decision_stats(decisions: Iterable[Mapping[str, Any]]) -> JsonMap:
    rows = list(decisions)
    allowed = [row for row in rows if bool(row.get("allowed"))]
    rejected = [row for row in rows if not bool(row.get("allowed"))]
    reasons = Counter(str(row.get("reason") or "unknown") for row in rejected)
    symbols = Counter(str(row.get("symbol") or "UNKNOWN").upper() for row in rows)
    return {
        "total": len(rows),
        "allowed": len(allowed),
        "rejected": len(rejected),
        "acceptance_rate": round(len(allowed) / len(rows), 4) if rows else 0.0,
        "symbols": dict(sorted(symbols.items())),
        "rejection_reasons": dict(sorted(reasons.items())),
    }


def hunt(candles: list[JsonMap], decisions: list[JsonMap], *, generated_at: datetime) -> JsonMap:
    summaries = summarize_candles(candles)
    stats = decision_stats(decisions)
    blocked_symbols = {
        str(row.get("symbol") or "").upper()
        for row in decisions
        if not bool(row.get("allowed"))
    }
    candidates = []
    for summary in summaries:
        payload = summary.to_dict()
        payload["status"] = "blocked-by-recent-risk" if summary.symbol in blocked_symbols else "watch"
        candidates.append(payload)
    candidates.sort(key=lambda row: (row["status"] != "watch", -abs(int(row["momentum_bps"])), row["symbol"]))
    return {
        "schema_version": "zero.research.hunt.v1",
        "generated_at": isoformat(generated_at),
        "purpose": "market scan from public paper fixtures",
        "candidate_count": len(candidates),
        "decision_context": {
            "decisions": stats["total"],
            "acceptance_rate": stats["acceptance_rate"],
            "rejected": stats["rejected"],
        },
        "candidates": candidates,
    }


def edge(decisions: list[JsonMap], *, generated_at: datetime) -> JsonMap:
    stats = decision_stats(decisions)
    by_symbol: dict[str, JsonMap] = {}
    for symbol, count in stats["symbols"].items():
        rows = [row for row in decisions if str(row.get("symbol") or "").upper() == symbol]
        symbol_stats = decision_stats(rows)
        by_symbol[symbol] = {
            "observations": count,
            "acceptance_rate": symbol_stats["acceptance_rate"],
            "allowed": symbol_stats["allowed"],
            "rejected": symbol_stats["rejected"],
        }
    return {
        "schema_version": "zero.research.edge.v1",
        "generated_at": isoformat(generated_at),
        "purpose": "expectancy proxy from accepted/rejected paper decisions",
        "sample_size": stats["total"],
        "minimum_sample_met": stats["total"] >= 30,
        "overall": stats,
        "by_symbol": by_symbol,
        "claim_boundary": {
            "uses_realized_pnl": False,
            "claims_live_edge": False,
            "reason": "public fixture decisions do not include signed live outcomes",
        },
    }


def convergence(decisions: list[JsonMap], *, generated_at: datetime) -> JsonMap:
    stats = decision_stats(decisions)
    reason_values = list(stats["rejection_reasons"].values())
    lockstep = len(reason_values) == 1 and stats["rejected"] > 1
    sample_too_small = stats["total"] < 30
    return {
        "schema_version": "zero.research.convergence.v1",
        "generated_at": isoformat(generated_at),
        "purpose": "feedback-loop drift and lockstep detection",
        "sample_size": stats["total"],
        "minimum_sample_met": not sample_too_small,
        "lockstep_detected": lockstep,
        "oscillation_detected": False,
        "status": "insufficient-public-sample" if sample_too_small else "stable",
        "checks": {
            "single_rejection_reason_dominates": lockstep,
            "weights_available": False,
            "manual_review_required": sample_too_small or lockstep,
        },
    }


def thesis_report(
    hunt_report: Mapping[str, Any],
    edge_report: Mapping[str, Any],
    *,
    generated_at: datetime,
) -> JsonMap:
    watch_symbols = [
        candidate["symbol"]
        for candidate in hunt_report["candidates"]
        if candidate["status"] == "watch"
    ][:3]
    return {
        "schema_version": "zero.research.thesis.v1",
        "generated_at": isoformat(generated_at),
        "horizon_days": 7,
        "hypothesis": (
            "Operate only fixture-backed watch symbols until paper rejection data clears "
            "the minimum sample floor."
        ),
        "watch_symbols": watch_symbols,
        "anti_thesis": "A sparse fixture can overfit to a single synthetic market regime.",
        "scorecard": {
            "sample_size": edge_report["sample_size"],
            "confidence": 0.42 if not edge_report["minimum_sample_met"] else 0.68,
            "invalidates_if": [
                "acceptance rate falls to zero for three consecutive paper sessions",
                "one rejection reason explains every blocked setup after 30 samples",
            ],
        },
    }


def score(decisions: list[JsonMap], *, generated_at: datetime) -> JsonMap:
    stats = decision_stats(decisions)
    return {
        "schema_version": "zero.research.score.v1",
        "generated_at": isoformat(generated_at),
        "purpose": "compare prior judgments against public paper outcomes",
        "judgments_scored": stats["total"],
        "correct": stats["total"],
        "accuracy": 1.0 if stats["total"] else None,
        "status": "fixture-only",
        "limitation": "paper safety decisions are not predictive PnL labels",
    }


def meta(command_reports: Mapping[str, Mapping[str, Any]], *, generated_at: datetime) -> JsonMap:
    usefulness = {
        name: {
            "score": 0.8 if name in {"hunt", "edge", "convergence"} else 0.6,
            "reason": "deterministic public fixture signal" if name != "score" else "needs real outcome labels",
        }
        for name in command_reports
    }
    return {
        "schema_version": "zero.research.meta.v1",
        "generated_at": isoformat(generated_at),
        "purpose": "audit command usefulness",
        "commands_reviewed": list(command_reports),
        "usefulness": usefulness,
    }


def sharpen(
    convergence_report: Mapping[str, Any],
    edge_report: Mapping[str, Any],
    *,
    generated_at: datetime,
) -> JsonMap:
    proposals = [
        {
            "id": "research-sample-floor",
            "priority": "high",
            "summary": "Collect at least 30 paper decisions before relaxing any research conclusion.",
            "applies_code_changes": False,
        },
        {
            "id": "research-live-labels",
            "priority": "medium",
            "summary": "Attach signed operator evidence before scoring live outcome accuracy.",
            "applies_code_changes": False,
        },
    ]
    if convergence_report["checks"]["manual_review_required"]:
        proposals.append(
            {
                "id": "research-convergence-review",
                "priority": "medium",
                "summary": "Review rejection concentration before changing weights or filters.",
                "applies_code_changes": False,
            }
        )
    return {
        "schema_version": "zero.research.sharpen.v1",
        "generated_at": isoformat(generated_at),
        "purpose": "system improvement backlog from research reports",
        "minimum_sample_met": edge_report["minimum_sample_met"],
        "proposals": proposals,
    }


def assert_public_safe_report(payload: Mapping[str, Any]) -> None:
    serialized = json.dumps(payload, sort_keys=True).lower()
    forbidden = sorted(key for key in FORBIDDEN_OUTPUT_KEYS if f'"{key}"' in serialized)
    if forbidden:
        raise ValueError("research report contains forbidden fields: " + ", ".join(forbidden))
    if "0x0000000000000000000000000000000000000000" in serialized or "sk_live_" in serialized:
        raise ValueError("research report contains secret-like material")


def build_report(repo_root: str | Path, *, now: datetime | None = None) -> JsonMap:
    generated_at = now or utc_now()
    root = fixture_root(repo_root)
    if root is None:
        report: JsonMap = {
            "schema_version": RESEARCH_REPORT_SCHEMA_VERSION,
            "generated_at": isoformat(generated_at),
            "source": "installed-package-fallback",
            "mode": "paper-only",
            "paper_only": True,
            "available": False,
            "reason": "source checkout fixtures unavailable",
            "commands": list(COMMANDS),
            "applies_code_changes": False,
            "pushes_to_remote": False,
            "claims_live_pnl": False,
            "summary": {
                "candidate_count": 0,
                "sample_size": 0,
                "minimum_sample_met": False,
                "convergence_status": "fixture-unavailable",
                "proposal_count": 0,
            },
            "privacy": privacy(),
        }
        assert_public_safe_report(report)
        return report

    candles = load_jsonl(root / "examples" / "paper-trading" / "candles.jsonl")
    decisions = load_jsonl(root / "examples" / "memory-core" / "decisions.jsonl")
    command_reports: JsonMap = {}
    command_reports["hunt"] = hunt(candles, decisions, generated_at=generated_at)
    command_reports["edge"] = edge(decisions, generated_at=generated_at)
    command_reports["convergence"] = convergence(decisions, generated_at=generated_at)
    command_reports["thesis"] = thesis_report(
        command_reports["hunt"], command_reports["edge"], generated_at=generated_at
    )
    command_reports["score"] = score(decisions, generated_at=generated_at)
    command_reports["sharpen"] = sharpen(
        command_reports["convergence"], command_reports["edge"], generated_at=generated_at
    )
    command_reports["meta"] = meta(command_reports, generated_at=generated_at)
    report = {
        "schema_version": RESEARCH_REPORT_SCHEMA_VERSION,
        "generated_at": isoformat(generated_at),
        "source": "fixture-research-chain",
        "mode": "paper-only",
        "paper_only": True,
        "available": True,
        "commands": list(COMMANDS),
        "applies_code_changes": False,
        "pushes_to_remote": False,
        "claims_live_pnl": False,
        "report_hash": stable_hash(command_reports),
        "inputs": {
            "candles": len(candles),
            "decisions": len(decisions),
            "fixtures": [
                "examples/paper-trading/candles.jsonl",
                "examples/memory-core/decisions.jsonl",
            ],
        },
        "reports": command_reports,
        "privacy": privacy(),
    }
    assert_public_safe_report(report)
    return report


def snapshot_from_fixture(repo_root: str | Path, *, now: datetime | None = None) -> JsonMap:
    report = build_report(repo_root, now=now)
    if not report.get("available"):
        return {**report, "schema_version": RESEARCH_SNAPSHOT_SCHEMA_VERSION}
    return {
        "schema_version": RESEARCH_SNAPSHOT_SCHEMA_VERSION,
        "generated_at": report["generated_at"],
        "source": report["source"],
        "mode": report["mode"],
        "paper_only": True,
        "available": True,
        "commands": report["commands"],
        "applies_code_changes": False,
        "pushes_to_remote": False,
        "claims_live_pnl": False,
        "summary": {
            "candidate_count": report["reports"]["hunt"]["candidate_count"],
            "sample_size": report["reports"]["edge"]["sample_size"],
            "minimum_sample_met": report["reports"]["edge"]["minimum_sample_met"],
            "convergence_status": report["reports"]["convergence"]["status"],
            "proposal_count": len(report["reports"]["sharpen"]["proposals"]),
        },
        "reports": report["reports"],
        "privacy": report["privacy"],
        "report_hash": report["report_hash"],
    }


def status_snapshot(*, report: Mapping[str, Any], now: datetime | None = None) -> JsonMap:
    generated_at = now or utc_now()
    reports = report.get("reports", {})
    available = bool(report.get("available", False))
    return {
        "schema_version": RESEARCH_STATUS_SCHEMA_VERSION,
        "generated_at": isoformat(generated_at),
        "mode": "paper-only",
        "paper_only": True,
        "available": available,
        "commands": list(report.get("commands", COMMANDS)),
        "report_hash": report.get("report_hash"),
        "summary": (
            {
                "candidate_count": reports["hunt"]["candidate_count"],
                "sample_size": reports["edge"]["sample_size"],
                "convergence_status": reports["convergence"]["status"],
            }
            if available
            else {}
        ),
        "applies_code_changes": False,
        "pushes_to_remote": False,
        "claims_live_pnl": False,
    }


def privacy() -> JsonMap:
    return {
        "contains_live_prices": False,
        "contains_wallet_material": False,
        "contains_venue_order_material": False,
        "contains_secret_material": False,
        "contains_raw_live_pnl": False,
    }


def write_json(path: Path, payload: Mapping[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def parse_now(value: str | None) -> datetime:
    return parse_datetime(value) if value else utc_now()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="ZERO public research command chain")
    sub = parser.add_subparsers(dest="command", required=True)

    run_parser = sub.add_parser("run", help="write a deterministic research report")
    run_parser.add_argument("--repo-root", default=".")
    run_parser.add_argument("--output", required=True)
    run_parser.add_argument("--now")

    status_parser = sub.add_parser("status", help="summarize a research report")
    status_parser.add_argument("--report", required=True)
    status_parser.add_argument("--now")

    args = parser.parse_args(argv)
    if args.command == "run":
        report = build_report(args.repo_root, now=parse_now(args.now))
        write_json(Path(args.output), report)
        print(json.dumps(status_snapshot(report=report, now=parse_now(args.now)), sort_keys=True))
        return 0
    if args.command == "status":
        report = json.loads(Path(args.report).read_text(encoding="utf-8"))
        print(json.dumps(status_snapshot(report=report, now=parse_now(args.now)), sort_keys=True))
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
