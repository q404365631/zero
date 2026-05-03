from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

JsonMap = dict[str, Any]

MEMORY_SCHEMA_VERSION = "zero.memory.entry.v1"
KNOWLEDGE_SCHEMA_VERSION = "zero.memory.knowledge.v1"
ALLOWED_KINDS = {"signal", "regime", "operator", "strategy_reference"}
DEFAULT_TTL_DAYS = {
    "signal": 7,
    "regime": 3,
    "operator": 30,
    "strategy_reference": 90,
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
MONEY_OR_PRICE_RE = re.compile(r"(?:\$|(?:price|notional|quantity|size)\s*[=:])", re.IGNORECASE)


def utc_now() -> datetime:
    return datetime.now(UTC)


def parse_datetime(value: str) -> datetime:
    normalized = value.replace("Z", "+00:00")
    parsed = datetime.fromisoformat(normalized)
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=UTC)
    return parsed.astimezone(UTC)


def isoformat(value: datetime) -> str:
    return value.astimezone(UTC).isoformat().replace("+00:00", "Z")


def stable_hash(payload: Mapping[str, Any]) -> str:
    body = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(body).hexdigest()


def safe_source_class(source: str) -> str:
    if "hyperliquid" in source.lower():
        return "live-readonly-market-data"
    if source.startswith("scenario:"):
        return "fixture-scenario"
    if source.startswith("api:"):
        return "paper-api"
    return "local-runtime"


def memory_id(
    *,
    kind: str,
    scope: str,
    subject: str,
    summary: str,
    evidence_hash: str,
) -> str:
    return stable_hash(
        {
            "kind": kind,
            "scope": scope,
            "subject": subject,
            "summary": summary,
            "evidence_hash": evidence_hash,
        }
    )


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


def assert_public_safe_memory_payload(payload: Mapping[str, Any]) -> None:
    forbidden = _walk_forbidden_keys(payload.get("metadata", {}))
    if forbidden:
        raise ValueError("memory metadata contains derivable or secret fields: " + ", ".join(forbidden))

    text_fields = [
        str(payload.get("subject", "")),
        str(payload.get("summary", "")),
        " ".join(str(tag) for tag in payload.get("tags", [])),
    ]
    text = "\n".join(text_fields)
    if SENSITIVE_TEXT_RE.search(text):
        raise ValueError("memory text contains secret-like or wallet-like material")
    if MONEY_OR_PRICE_RE.search(text):
        raise ValueError("memory text contains derivable price, size, or notional material")


@dataclass(frozen=True)
class MemoryEntry:
    kind: str
    scope: str
    subject: str
    summary: str
    evidence_hash: str
    source: str
    created_at: datetime
    expires_at: datetime
    confidence: float = 1.0
    tags: tuple[str, ...] = field(default_factory=tuple)
    metadata: Mapping[str, Any] = field(default_factory=dict)
    schema_version: str = MEMORY_SCHEMA_VERSION
    entry_id: str | None = None

    def __post_init__(self) -> None:
        if self.kind not in ALLOWED_KINDS:
            raise ValueError(f"unsupported memory kind: {self.kind}")
        if not 0 <= self.confidence <= 1:
            raise ValueError("confidence must be between 0 and 1")
        if self.expires_at <= self.created_at:
            raise ValueError("expires_at must be after created_at")
        assert_public_safe_memory_payload(self.to_dict(include_id=False))

    @property
    def id(self) -> str:
        return self.entry_id or memory_id(
            kind=self.kind,
            scope=self.scope,
            subject=self.subject,
            summary=self.summary,
            evidence_hash=self.evidence_hash,
        )

    def is_expired(self, now: datetime | None = None) -> bool:
        now = now or utc_now()
        return self.expires_at <= now

    def to_dict(self, *, include_id: bool = True) -> JsonMap:
        payload: JsonMap = {
            "schema_version": self.schema_version,
            "kind": self.kind,
            "scope": self.scope,
            "subject": self.subject,
            "summary": self.summary,
            "evidence_hash": self.evidence_hash,
            "source": self.source,
            "created_at": isoformat(self.created_at),
            "expires_at": isoformat(self.expires_at),
            "confidence": round(float(self.confidence), 4),
            "tags": list(self.tags),
            "metadata": dict(self.metadata),
        }
        if include_id:
            payload["id"] = self.id
        return payload

    @classmethod
    def from_dict(cls, payload: Mapping[str, Any]) -> "MemoryEntry":
        if payload.get("schema_version") != MEMORY_SCHEMA_VERSION:
            raise ValueError("unsupported memory entry schema_version")
        tags = tuple(str(tag) for tag in payload.get("tags", []))
        return cls(
            entry_id=str(payload["id"]),
            kind=str(payload["kind"]),
            scope=str(payload["scope"]),
            subject=str(payload["subject"]),
            summary=str(payload["summary"]),
            evidence_hash=str(payload["evidence_hash"]),
            source=str(payload["source"]),
            created_at=parse_datetime(str(payload["created_at"])),
            expires_at=parse_datetime(str(payload["expires_at"])),
            confidence=float(payload["confidence"]),
            tags=tags,
            metadata=payload.get("metadata", {}),
        )


class MemoryStore:
    """Append-only JSONL store for public-safe local memory."""

    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)

    def append(self, entry: MemoryEntry) -> bool:
        seen = {existing.id for existing in self.read_all()}
        if entry.id in seen:
            return False
        self.path.parent.mkdir(parents=True, exist_ok=True)
        line = json.dumps(entry.to_dict(), sort_keys=True, separators=(",", ":")) + "\n"
        fd = os.open(self.path, os.O_APPEND | os.O_CREAT | os.O_WRONLY, 0o600)
        with os.fdopen(fd, "a", encoding="utf-8") as handle:
            handle.write(line)
            handle.flush()
            os.fsync(handle.fileno())
        return True

    def append_many(self, entries: Iterable[MemoryEntry]) -> int:
        return sum(1 for entry in entries if self.append(entry))

    def read_all(self) -> list[MemoryEntry]:
        if not self.path.exists():
            return []
        lines = self.path.read_text(encoding="utf-8").splitlines()
        return [MemoryEntry.from_dict(json.loads(line)) for line in lines if line.strip()]

    def active(self, now: datetime | None = None) -> list[MemoryEntry]:
        now = now or utc_now()
        return [entry for entry in self.read_all() if not entry.is_expired(now)]

    def stats(self, now: datetime | None = None) -> JsonMap:
        now = now or utc_now()
        entries = self.read_all()
        active = [entry for entry in entries if not entry.is_expired(now)]
        expired = len(entries) - len(active)
        by_kind = {kind: 0 for kind in sorted(ALLOWED_KINDS)}
        for entry in active:
            by_kind[entry.kind] += 1
        return {
            "schema_version": "zero.memory.stats.v1",
            "generated_at": isoformat(now),
            "path": str(self.path),
            "total_entries": len(entries),
            "active_entries": len(active),
            "expired_entries": expired,
            "by_kind": by_kind,
            "deduplication": "idempotent-by-content-hash",
            "privacy": {
                "contains_live_prices": False,
                "contains_wallet_material": False,
                "contains_exchange_order_ids": False,
                "contains_private_keys": False,
            },
        }


def entry_ttl(kind: str, created_at: datetime) -> datetime:
    return created_at + timedelta(days=DEFAULT_TTL_DAYS[kind])


def memory_from_decision(payload: Mapping[str, Any], *, now: datetime) -> MemoryEntry:
    allowed = bool(payload.get("allowed"))
    symbol = str(payload.get("symbol") or payload.get("coin") or "UNKNOWN").upper()
    reason = str(payload.get("reason") or "no reason recorded").strip()
    source = safe_source_class(str(payload.get("source") or "local-runtime"))
    side = str(payload.get("side") or "unknown").lower()
    reduce_only = bool(payload.get("reduce_only", False))
    kind = "signal" if allowed else "strategy_reference"
    summary = (
        f"Accepted paper decision for {symbol}; risk gate reported {reason}."
        if allowed
        else f"Rejected {symbol} by risk gate: {reason}."
    )
    evidence = {
        "symbol": symbol,
        "side": side,
        "allowed": allowed,
        "reason": reason,
        "reduce_only": reduce_only,
        "source": source,
    }
    evidence_hash = stable_hash(evidence)
    return MemoryEntry(
        kind=kind,
        scope="local-private",
        subject=symbol,
        summary=summary,
        evidence_hash=evidence_hash,
        source=source,
        created_at=now,
        expires_at=entry_ttl(kind, now),
        confidence=0.8 if allowed else 0.9,
        tags=("paper", "risk", "accepted" if allowed else "rejected"),
        metadata={
            "allowed": allowed,
            "reason_class": reason,
            "source_class": source,
            "reduce_only": reduce_only,
        },
    )


def regime_memory(
    *,
    symbol: str,
    regime: str,
    source: str,
    confidence: float,
    now: datetime,
) -> MemoryEntry:
    kind = "regime"
    subject = symbol.upper()
    summary = f"{subject} regime classified as {regime}."
    evidence_hash = stable_hash(
        {
            "symbol": subject,
            "regime": regime,
            "source": safe_source_class(source),
        }
    )
    return MemoryEntry(
        kind=kind,
        scope="local-private",
        subject=subject,
        summary=summary,
        evidence_hash=evidence_hash,
        source=safe_source_class(source),
        created_at=now,
        expires_at=entry_ttl(kind, now),
        confidence=confidence,
        tags=("regime",),
        metadata={"regime": regime, "source_class": safe_source_class(source)},
    )


def operator_memory(
    *,
    action: str,
    risk_direction: str,
    ok: bool,
    source: str,
    now: datetime,
    reason: str | None = None,
) -> MemoryEntry:
    kind = "operator"
    outcome = "accepted" if ok else "refused"
    reason_text = f": {reason}" if reason else "."
    summary = f"Operator action {action} was {outcome}; risk direction {risk_direction}{reason_text}"
    evidence_hash = stable_hash(
        {
            "action": action,
            "risk_direction": risk_direction,
            "ok": ok,
            "reason": reason,
            "source": safe_source_class(source),
        }
    )
    return MemoryEntry(
        kind=kind,
        scope="local-private",
        subject=action,
        summary=summary,
        evidence_hash=evidence_hash,
        source=safe_source_class(source),
        created_at=now,
        expires_at=entry_ttl(kind, now),
        confidence=0.85,
        tags=("operator", "risk-direction"),
        metadata={"risk_direction": risk_direction, "ok": ok, "source_class": safe_source_class(source)},
    )


def extract_from_decisions(decisions: Iterable[Mapping[str, Any]], *, now: datetime) -> list[MemoryEntry]:
    return [memory_from_decision(decision, now=now) for decision in decisions]


def knowledge_markdown(entries: Iterable[MemoryEntry], *, generated_at: datetime) -> str:
    active = sorted(entries, key=lambda entry: (entry.kind, entry.subject, entry.summary))
    counts = {kind: 0 for kind in sorted(ALLOWED_KINDS)}
    for entry in active:
        counts[entry.kind] += 1
    lines = [
        "# ZERO Local Knowledge",
        "",
        f"schema_version: {KNOWLEDGE_SCHEMA_VERSION}",
        f"generated_at: {isoformat(generated_at)}",
        "",
        "This file is generated from public-safe local memory entries. It must not",
        "contain live prices, raw exchange payloads, wallet material, private keys,",
        "or exchange order identifiers.",
        "",
        "## Summary",
        "",
    ]
    for kind in sorted(ALLOWED_KINDS):
        lines.append(f"- {kind}: {counts[kind]}")
    lines.extend(["", "## Active Memory", ""])
    for entry in active:
        lines.extend(
            [
                f"### {entry.kind}: {entry.subject}",
                "",
                f"- summary: {entry.summary}",
                f"- confidence: {entry.confidence:.2f}",
                f"- source: {entry.source}",
                f"- expires_at: {isoformat(entry.expires_at)}",
                f"- evidence_hash: {entry.evidence_hash}",
                "",
            ]
        )
    return "\n".join(lines).rstrip() + "\n"


def write_knowledge(store: MemoryStore, output: str | Path, *, now: datetime | None = None) -> Path:
    now = now or utc_now()
    path = Path(output)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(knowledge_markdown(store.active(now), generated_at=now), encoding="utf-8")
    return path


def load_jsonl(path: str | Path) -> list[JsonMap]:
    lines = Path(path).read_text(encoding="utf-8").splitlines()
    return [json.loads(line) for line in lines if line.strip()]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="ZERO local memory core")
    subcommands = parser.add_subparsers(dest="command", required=True)

    extract = subcommands.add_parser("extract", help="extract memory from decision JSONL")
    extract.add_argument("--decisions", required=True, help="decision JSONL input")
    extract.add_argument("--store", required=True, help="append-only memory JSONL store")
    extract.add_argument("--knowledge", help="optional generated knowledge.md output")
    extract.add_argument("--now", help="UTC timestamp override for deterministic runs")

    status = subcommands.add_parser("status", help="print memory store stats")
    status.add_argument("--store", required=True, help="append-only memory JSONL store")
    status.add_argument("--now", help="UTC timestamp override for deterministic runs")

    knowledge = subcommands.add_parser("knowledge", help="generate knowledge.md from memory")
    knowledge.add_argument("--store", required=True, help="append-only memory JSONL store")
    knowledge.add_argument("--output", required=True, help="knowledge.md output")
    knowledge.add_argument("--now", help="UTC timestamp override for deterministic runs")

    args = parser.parse_args(argv)
    now = parse_datetime(args.now) if getattr(args, "now", None) else utc_now()
    store = MemoryStore(args.store)

    if args.command == "extract":
        decisions = load_jsonl(args.decisions)
        entries = extract_from_decisions(decisions, now=now)
        appended = store.append_many(entries)
        output: JsonMap = {
            "schema_version": "zero.memory.extract.v1",
            "input_decisions": len(decisions),
            "candidate_entries": len(entries),
            "appended_entries": appended,
            "store": store.stats(now),
        }
        if args.knowledge:
            output["knowledge_path"] = str(write_knowledge(store, args.knowledge, now=now))
        print(json.dumps(output, indent=2, sort_keys=True))
        return 0

    if args.command == "status":
        print(json.dumps(store.stats(now), indent=2, sort_keys=True))
        return 0

    if args.command == "knowledge":
        path = write_knowledge(store, args.output, now=now)
        print(json.dumps({"schema_version": KNOWLEDGE_SCHEMA_VERSION, "path": str(path)}, sort_keys=True))
        return 0

    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
