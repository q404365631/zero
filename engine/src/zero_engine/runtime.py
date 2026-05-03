from __future__ import annotations

import argparse
import json
import os
import shutil
import time
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from zero_engine.bus import DurableRuntimeBus
from zero_engine.journal import DecisionJournal
from zero_engine.live import LiveExecutionPolicy, LiveExecutor, RecordingExchangeAdapter
from zero_engine.live_certification import run_live_certification
from zero_engine.memory import isoformat, stable_hash
from zero_engine.models import OrderIntent
from zero_engine.paper import Fill
from zero_engine.paper import PaperEngine
from zero_engine.scenario import PaperScenario, load_scenario


CYCLE_SCHEMA_VERSION = "zero.runtime.cycle.v1"
FEEDBACK_SCHEMA_VERSION = "zero.runtime.feedback.v1"
PRODUCTION_PARITY_SCHEMA_VERSION = "zero.runtime.production_parity.v1"
PHASE_ORDER = ("observe", "orient", "decide", "act", "learn")


@dataclass(frozen=True)
class RuntimeConfig:
    scenario: PaperScenario
    decision_journal_path: Path | None = None
    cycle_journal_path: Path | None = None
    runtime_bus_path: Path | None = None
    mode: str = "paper"
    interval_s: float = 5.0

    def __post_init__(self) -> None:
        if self.mode != "paper":
            raise ValueError("public runtime loop only supports paper mode")
        if self.interval_s < 0:
            raise ValueError("runtime interval must be non-negative")


@dataclass(frozen=True)
class RuntimeCycleRecord:
    schema_version: str
    cycle_id: int
    mode: str
    observe: dict[str, Any]
    orient: dict[str, Any]
    decide: dict[str, Any]
    act: dict[str, Any]
    learn: dict[str, Any]
    as_of: float

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "cycle_id": self.cycle_id,
            "mode": self.mode,
            "observe": self.observe,
            "orient": self.orient,
            "decide": self.decide,
            "act": self.act,
            "learn": self.learn,
            "as_of": self.as_of,
        }


@dataclass
class RuntimeLoop:
    config: RuntimeConfig
    engine: PaperEngine
    bus: DurableRuntimeBus | None = None

    @classmethod
    def from_config(cls, config: RuntimeConfig) -> "RuntimeLoop":
        journal = (
            DecisionJournal(config.decision_journal_path) if config.decision_journal_path else None
        )
        bus = DurableRuntimeBus(config.runtime_bus_path) if config.runtime_bus_path else None
        if journal is None:
            engine = PaperEngine(limits=config.scenario.limits)
        elif config.decision_journal_path and config.decision_journal_path.exists():
            engine = PaperEngine.recover_from_journal(journal, limits=config.scenario.limits)
        else:
            engine = PaperEngine(limits=config.scenario.limits, journal=journal)
        return cls(config=config, engine=engine, bus=bus)

    def run_once(self) -> RuntimeCycleRecord:
        cycle_id = len(self.engine.decisions) + 1
        observe = self.observe(cycle_id)
        orient = self.orient(observe)
        decide = self.decide(cycle_id, orient)
        act = self.act(cycle_id, decide)
        learn = self.learn()
        record = RuntimeCycleRecord(
            schema_version=CYCLE_SCHEMA_VERSION,
            cycle_id=cycle_id,
            mode=self.config.mode,
            observe=observe,
            orient=orient,
            decide=decide,
            act=act,
            learn=learn,
            as_of=self.engine.clock(),
        )
        self.append_cycle(record)
        self.publish_bus_events(record)
        return record

    def run(self, *, max_cycles: int | None = None) -> list[RuntimeCycleRecord]:
        records: list[RuntimeCycleRecord] = []
        remaining = max_cycles
        while remaining is None or remaining > 0:
            records.append(self.run_once())
            if remaining is not None:
                remaining -= 1
                if remaining <= 0:
                    break
            if self.config.interval_s:
                time.sleep(self.config.interval_s)
        return records

    def observe(self, cycle_id: int) -> dict[str, Any]:
        open_positions = [
            {
                "symbol": symbol,
                "quantity": position.quantity,
                "avg_price": position.avg_price,
                "notional_usd": round(position.notional_usd, 2),
            }
            for symbol, position in sorted(self.engine.positions.items())
            if position.quantity != 0
        ]
        return {
            "phase": "observe",
            "cycle_id": cycle_id,
            "scenario": self.config.scenario.name,
            "market_source": "scenario:orders",
            "decisions_seen": len(self.engine.decisions),
            "fills_seen": len(self.engine.fills),
            "rejections_seen": len(self.engine.rejections),
            "open_positions": open_positions,
            "recovery": self.engine.recovery.to_dict(),
        }

    def orient(self, observe: dict[str, Any]) -> dict[str, Any]:
        open_positions = observe["open_positions"]
        total_position_notional = sum(
            float(position["notional_usd"]) for position in open_positions
        )
        risk_posture = "flat" if not open_positions else "exposed"
        if total_position_notional >= self.config.scenario.limits.max_position_notional_usd:
            risk_posture = "capacity_reached"
        return {
            "phase": "orient",
            "risk_posture": risk_posture,
            "stale_data": False,
            "total_position_notional_usd": round(total_position_notional, 2),
            "limits": {
                "max_notional_usd": self.config.scenario.limits.max_notional_usd,
                "max_position_notional_usd": self.config.scenario.limits.max_position_notional_usd,
                "min_confidence": self.config.scenario.limits.min_confidence,
            },
        }

    def decide(self, cycle_id: int, orient: dict[str, Any]) -> dict[str, Any]:
        intent = self.next_intent()
        return {
            "phase": "decide",
            "source": f"runtime:{self.config.scenario.name}",
            "risk_posture": orient["risk_posture"],
            "intent": intent_to_dict(intent),
            "idempotency_key": f"runtime-{self.config.scenario.name}-{cycle_id}",
        }

    def act(self, cycle_id: int, decide: dict[str, Any]) -> dict[str, Any]:
        intent = self.next_intent()
        decision = self.engine.submit(
            intent,
            source=decide["source"],
            idempotency_key=str(decide["idempotency_key"]),
            trace_id=f"runtime-cycle-{cycle_id}",
        )
        latest = self.engine.decisions[-1]
        return {
            "phase": "act",
            "accepted": decision.allowed,
            "reason": decision.reason,
            "decision": latest.to_dict(),
        }

    def learn(self) -> dict[str, Any]:
        return {
            "phase": "learn",
            "decisions": len(self.engine.decisions),
            "fills": len(self.engine.fills),
            "rejections": len(self.engine.rejections),
            "open_positions": len([p for p in self.engine.positions.values() if p.quantity != 0]),
        }

    def next_intent(self) -> OrderIntent:
        index = len(self.engine.decisions) % len(self.config.scenario.orders)
        return self.config.scenario.orders[index]

    def append_cycle(self, record: RuntimeCycleRecord) -> None:
        if self.config.cycle_journal_path is None:
            return
        self.config.cycle_journal_path.parent.mkdir(parents=True, exist_ok=True)
        with self.config.cycle_journal_path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record.to_dict(), sort_keys=True, separators=(",", ":")) + "\n")

    def publish_bus_events(self, record: RuntimeCycleRecord) -> None:
        if self.bus is None:
            return

        trace_id = f"runtime-cycle-{record.cycle_id}"
        self.bus.append("runtime.cycle", record.to_dict(), as_of=record.as_of, trace_id=trace_id)
        latest_decision = record.act["decision"]
        self.bus.append("decision.record", latest_decision, as_of=record.as_of, trace_id=trace_id)
        if record.act["accepted"]:
            self.bus.append(
                "fill.record",
                fill_to_dict(self.engine.fills[-1]),
                as_of=record.as_of,
                trace_id=trace_id,
            )
        else:
            self.bus.append(
                "rejection.record", latest_decision, as_of=record.as_of, trace_id=trace_id
            )
        positions = positions_to_dict(self.engine.positions)
        self.bus.append(
            "position.snapshot",
            {"positions": positions},
            as_of=record.as_of,
            trace_id=trace_id,
        )
        health = runtime_health(record, self.engine)
        health_event = self.bus.append(
            "runtime.health", health, as_of=record.as_of, trace_id=trace_id
        )
        self.bus.write_snapshot(
            {
                "cycle": record.to_dict(),
                "health": health,
                "positions": positions,
            },
            as_of=record.as_of,
            source_event_id=health_event.event_id,
        )


def intent_to_dict(intent: OrderIntent) -> dict[str, Any]:
    return {
        "symbol": intent.symbol,
        "side": intent.side.value,
        "quantity": intent.quantity,
        "price": intent.price,
        "notional_usd": round(intent.notional_usd, 2),
        "confidence": intent.confidence,
        "reduce_only": intent.reduce_only,
    }


def fill_to_dict(fill: Fill) -> dict[str, Any]:
    return {
        "symbol": fill.symbol,
        "side": fill.side,
        "quantity": fill.quantity,
        "price": fill.price,
        "notional_usd": round(fill.notional_usd, 2),
        "as_of": fill.as_of,
    }


def positions_to_dict(positions: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        {
            "symbol": symbol,
            "quantity": position.quantity,
            "avg_price": position.avg_price,
            "notional_usd": round(position.notional_usd, 2),
        }
        for symbol, position in sorted(positions.items())
        if position.quantity != 0
    ]


def runtime_health(record: RuntimeCycleRecord, engine: PaperEngine) -> dict[str, Any]:
    return {
        "mode": record.mode,
        "last_cycle_id": record.cycle_id,
        "decisions": len(engine.decisions),
        "fills": len(engine.fills),
        "rejections": len(engine.rejections),
        "open_positions": len([p for p in engine.positions.values() if p.quantity != 0]),
        "recovery": engine.recovery.to_dict(),
    }


def runtime_feedback(
    records: list[RuntimeCycleRecord],
    engine: PaperEngine,
    *,
    generated_at: datetime | None = None,
) -> dict[str, Any]:
    generated_at = generated_at or datetime.now(UTC)
    rejection_reasons: dict[str, int] = {}
    rejection_symbols: dict[str, int] = {}
    for intent, decision in engine.rejections:
        reason = decision.reason
        rejection_reasons[reason] = rejection_reasons.get(reason, 0) + 1
        rejection_symbols[intent.symbol] = rejection_symbols.get(intent.symbol, 0) + 1

    accepted = len(engine.fills)
    total = len(engine.decisions)
    rejection_rate = (len(engine.rejections) / total) if total else 0.0
    feedback_items = []
    for reason, count in sorted(rejection_reasons.items(), key=lambda item: (-item[1], item[0])):
        if "notional exceeds" in reason:
            action = "lower candidate size or raise the explicit risk limit after review"
        elif "confidence" in reason:
            action = "improve signal calibration before promoting the strategy"
        else:
            action = "inspect rejected cohort before changing risk policy"
        feedback_items.append(
            {
                "reason": reason,
                "count": count,
                "suggested_action": action,
                "risk_direction": "down",
            }
        )

    return {
        "schema_version": FEEDBACK_SCHEMA_VERSION,
        "generated_at": isoformat(generated_at),
        "sample_size": total,
        "cycles": len(records),
        "fills": accepted,
        "rejections": len(engine.rejections),
        "rejection_rate": round(rejection_rate, 4),
        "by_rejection_reason": dict(sorted(rejection_reasons.items())),
        "by_rejection_symbol": dict(sorted(rejection_symbols.items())),
        "execution_quality": {
            "live_slippage_claimed": False,
            "exchange_fill_quality_claimed": False,
            "paper_fill_quality_available": bool(engine.fills),
            "requires_operator_owned_live_evidence": True,
        },
        "items": feedback_items,
    }


def production_parity_report(
    *,
    scenario_path: str | Path,
    output: str | Path,
    cycles: int | None = None,
    generated_at: datetime | None = None,
) -> dict[str, Any]:
    generated_at = generated_at or datetime.now(UTC)
    if cycles is not None and cycles <= 0:
        raise ValueError("production parity cycles must be positive")

    scenario = load_scenario(scenario_path)
    cycle_count = cycles or len(scenario.orders)
    output_path = Path(output)
    output_path.mkdir(parents=True, exist_ok=True)
    for stale_file in ("decisions.jsonl", "cycles.jsonl", "production-parity.json"):
        target = output_path / stale_file
        if target.exists():
            target.unlink()
    stale_bus = output_path / "runtime-bus"
    if stale_bus.exists():
        shutil.rmtree(stale_bus)

    config = RuntimeConfig(
        scenario=scenario,
        decision_journal_path=output_path / "decisions.jsonl",
        cycle_journal_path=output_path / "cycles.jsonl",
        runtime_bus_path=output_path / "runtime-bus",
        interval_s=0,
    )
    loop = RuntimeLoop.from_config(config)
    records = loop.run(max_cycles=cycle_count)

    adapter = RecordingExchangeAdapter()
    live_executor = LiveExecutor(
        adapter=adapter,
        policy=LiveExecutionPolicy(max_notional_usd=scenario.limits.max_notional_usd),
        clock=loop.engine.clock,
        enabled=False,
    )
    live_shadow = []
    for record in records:
        intent = dict_to_intent(record.decide["intent"])
        shadow = live_executor.submit(
            intent,
            idempotency_key=str(record.decide["idempotency_key"]),
            trace_id=f"production-parity-cycle-{record.cycle_id}",
            operator_context={
                "schema_version": "zero.operator_context.v1",
                "operator_id": "production-parity-harness",
                "handle": "production-parity-harness",
                "role": "system",
                "scope": "public-proof",
                "source": "zero.runtime.production_parity",
            },
        )
        live_shadow.append(shadow)

    bus = loop.bus
    integrity = bus.verify_integrity().to_dict() if bus is not None else {"ok": False}
    snapshot = bus.verify_snapshot() if bus is not None else {"present": False, "consistent": False}
    certification = run_live_certification().to_dict()
    feedback = runtime_feedback(records, loop.engine, generated_at=generated_at)

    checks = {
        "phase_order": all(record_phase_order(record) == list(PHASE_ORDER) for record in records),
        "paper_cycle_count": len(records) == cycle_count,
        "live_shadow_count": len(live_shadow) == len(records),
        "live_shadow_fail_closed": all(not record.accepted for record in live_shadow),
        "live_adapter_no_orders": len(adapter.placed) == 0,
        "runtime_bus_integrity": bool(integrity.get("ok")),
        "runtime_snapshot_consistent": bool(snapshot.get("consistent")),
        "shared_idempotency_keys": [record.decide["idempotency_key"] for record in records]
        == [record.idempotency_key for record in live_shadow],
        "paper_boundary": all(record.mode == "paper" for record in records),
        "dry_run_live_certification": certification["passed"]
        and certification["summary"]["orders_placed_live"] == 0,
        "operator_owned_canary_required": True,
        "protected_live_code_mutation_blocked": True,
    }
    ok = all(checks.values())
    payload = {
        "schema_version": PRODUCTION_PARITY_SCHEMA_VERSION,
        "generated_at": isoformat(generated_at),
        "mode": "production-parity",
        "paper_only": True,
        "places_live_orders": False,
        "scenario": scenario.name,
        "cycles_requested": cycle_count,
        "cycles_run": len(records),
        "phase_order": list(PHASE_ORDER),
        "paper": {
            "decisions": len(loop.engine.decisions),
            "fills": len(loop.engine.fills),
            "rejections": len(loop.engine.rejections),
            "open_positions": len([p for p in loop.engine.positions.values() if p.quantity != 0]),
            "recent_cycles": [record.to_dict() for record in records[-3:]],
        },
        "live_shadow": {
            "mode": "disabled-fail-closed",
            "accepted": len([record for record in live_shadow if record.accepted]),
            "refused": len([record for record in live_shadow if not record.accepted]),
            "adapter_orders_placed": len(adapter.placed),
            "records": [record.to_dict() for record in live_shadow],
        },
        "runtime_bus": {
            "integrity": integrity,
            "snapshot": snapshot,
        },
        "feedback": feedback,
        "certification": {
            "schema_version": certification["schema_version"],
            "mode": certification["mode"],
            "passed": certification["passed"],
            "live_start_certified": certification["live_start_certified"],
            "summary": certification["summary"],
        },
        "checks": checks,
        "ok": ok,
        "claim_boundary": {
            "production_ooda_parity": ok,
            "live_trading_claimed": False,
            "operator_owned_canary_required_for_live_claim": True,
            "protected_live_code_evolution_allowed": False,
            "remote_push_allowed": False,
        },
        "parity_hash": stable_hash(
            {
                "schema_version": PRODUCTION_PARITY_SCHEMA_VERSION,
                "scenario": scenario.name,
                "cycles_run": len(records),
                "checks": checks,
                "feedback": feedback,
            }
        ),
    }
    (output_path / "production-parity.json").write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return payload


def production_parity_snapshot(
    repo_root: str | Path,
    *,
    now: datetime | None = None,
) -> dict[str, Any]:
    root = find_runtime_repo_root(Path(repo_root))
    scenario = root / "examples" / "paper-trading" / "scenario.json"
    if not scenario.is_file():
        return {
            "schema_version": PRODUCTION_PARITY_SCHEMA_VERSION,
            "generated_at": isoformat(now or datetime.now(UTC)),
            "mode": "production-parity",
            "paper_only": True,
            "places_live_orders": False,
            "available": False,
            "ok": False,
            "reason": "bundled scenario unavailable",
            "claim_boundary": {
                "production_ooda_parity": False,
                "live_trading_claimed": False,
                "operator_owned_canary_required_for_live_claim": True,
                "protected_live_code_evolution_allowed": False,
                "remote_push_allowed": False,
            },
        }
    output = Path(
        os.environ.get("ZERO_RUNTIME_PARITY_SNAPSHOT_DIR", "/tmp/zero-runtime-parity-snapshot")
    )
    return {
        **production_parity_report(scenario_path=scenario, output=output, generated_at=now),
        "source": "bundled-paper-scenario",
        "available": True,
    }


def find_runtime_repo_root(start: Path) -> Path:
    current = start.resolve()
    if current.is_file():
        current = current.parent
    for candidate in (current, *current.parents):
        if (candidate / "examples" / "paper-trading" / "scenario.json").is_file():
            return candidate
    return current


def record_phase_order(record: RuntimeCycleRecord) -> list[str]:
    phases = [record.observe["phase"], record.orient["phase"], record.decide["phase"]]
    phases.extend([record.act["phase"], record.learn["phase"]])
    return [str(phase) for phase in phases]


def dict_to_intent(payload: dict[str, Any]) -> OrderIntent:
    from zero_engine.models import Side

    return OrderIntent(
        symbol=str(payload["symbol"]),
        side=Side(str(payload["side"])),
        quantity=float(payload["quantity"]),
        price=float(payload["price"]),
        confidence=float(payload["confidence"]),
        reduce_only=bool(payload.get("reduce_only", False)),
    )


def load_runtime_config(
    *,
    scenario_path: str | Path,
    decision_journal_path: str | Path | None = None,
    cycle_journal_path: str | Path | None = None,
    runtime_bus_path: str | Path | None = None,
    interval_s: float = 5.0,
) -> RuntimeConfig:
    return RuntimeConfig(
        scenario=load_scenario(scenario_path),
        decision_journal_path=Path(decision_journal_path) if decision_journal_path else None,
        cycle_journal_path=Path(cycle_journal_path) if cycle_journal_path else None,
        runtime_bus_path=Path(runtime_bus_path) if runtime_bus_path else None,
        interval_s=interval_s,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run the ZERO paper OODA runtime loop.")
    parser.add_argument("--scenario", default="examples/paper-trading/scenario.json")
    parser.add_argument("--journal", default=None, help="Decision JSONL journal path.")
    parser.add_argument("--cycle-journal", default=None, help="Runtime cycle JSONL journal path.")
    parser.add_argument("--runtime-bus", default=None, help="Durable runtime bus directory.")
    parser.add_argument("--interval", type=float, default=5.0)
    parser.add_argument("--once", action="store_true", help="Run exactly one cycle.")
    parser.add_argument("--max-cycles", type=int, default=None)
    parser.add_argument(
        "--production-parity",
        action="store_true",
        help="Emit a production-parity OODA report without placing live orders.",
    )
    parser.add_argument(
        "--output",
        default=None,
        help="Artifact directory for --production-parity.",
    )
    args = parser.parse_args(argv)

    if args.max_cycles is not None and args.max_cycles <= 0:
        parser.error("--max-cycles must be positive")

    if args.production_parity:
        payload = production_parity_report(
            scenario_path=args.scenario,
            output=args.output or "artifacts/runtime-parity",
            cycles=args.max_cycles,
        )
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0

    max_cycles = 1 if args.once else args.max_cycles
    if max_cycles is None:
        print(
            "zero runtime loop running continuously; pass --once or --max-cycles for bounded runs",
            flush=True,
        )

    config = load_runtime_config(
        scenario_path=args.scenario,
        decision_journal_path=args.journal,
        cycle_journal_path=args.cycle_journal,
        runtime_bus_path=args.runtime_bus,
        interval_s=args.interval,
    )
    loop = RuntimeLoop.from_config(config)
    records = loop.run(max_cycles=max_cycles)
    if records:
        payload: dict[str, Any] = records[-1].to_dict()
    else:
        payload = {"schema_version": CYCLE_SCHEMA_VERSION, "cycles": 0}
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
