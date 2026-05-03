from __future__ import annotations

import json
from datetime import UTC, datetime, timedelta

import pytest

from zero_engine.memory import (
    MemoryEntry,
    MemoryStore,
    extract_from_decisions,
    knowledge_markdown,
    main,
    operator_memory,
    regime_memory,
)

FIXED = datetime(2026, 5, 1, tzinfo=UTC)


def decision(symbol: str = "BTC", *, allowed: bool = True) -> dict:
    return {
        "as_of": 1777646400.0,
        "source": "api:/execute",
        "symbol": symbol,
        "side": "buy",
        "quantity": 0.01,
        "price": 40500.0,
        "notional_usd": 405.0,
        "confidence": 0.9,
        "reduce_only": False,
        "allowed": allowed,
        "reason": "allowed" if allowed else "order notional exceeds limit",
        "trace_id": "trace-fixture",
    }


def test_extract_from_decisions_redacts_derivable_market_state() -> None:
    entries = extract_from_decisions([decision()], now=FIXED)

    assert len(entries) == 1
    entry = entries[0].to_dict()
    serialized = json.dumps(entry, sort_keys=True).lower()
    assert entry["kind"] == "signal"
    assert entry["subject"] == "BTC"
    assert "40500" not in serialized
    assert "0.01" not in serialized
    assert "notional_usd" not in serialized
    assert "idempotency_key" not in serialized
    assert entry["metadata"]["source_class"] == "paper-api"


def test_memory_store_deduplicates_and_expires(tmp_path) -> None:
    store = MemoryStore(tmp_path / "memory.jsonl")
    entries = extract_from_decisions([decision()], now=FIXED)

    assert store.append_many(entries) == 1
    assert store.append_many(entries) == 0
    assert store.stats(FIXED)["active_entries"] == 1
    assert store.stats(FIXED + timedelta(days=8))["expired_entries"] == 1
    assert store.active(FIXED + timedelta(days=8)) == []


def test_memory_rejects_secret_like_and_derivable_payloads() -> None:
    with pytest.raises(ValueError, match="wallet-like"):
        MemoryEntry(
            kind="operator",
            scope="local-private",
            subject="0x1234567890abcdef1234567890abcdef12345678",
            summary="operator note",
            evidence_hash="sha256:test",
            source="test",
            created_at=FIXED,
            expires_at=FIXED + timedelta(days=1),
        )

    with pytest.raises(ValueError, match="derivable"):
        MemoryEntry(
            kind="operator",
            scope="local-private",
            subject="operator",
            summary="price=40500",
            evidence_hash="sha256:test",
            source="test",
            created_at=FIXED,
            expires_at=FIXED + timedelta(days=1),
        )

    with pytest.raises(ValueError, match="derivable or secret fields"):
        MemoryEntry(
            kind="signal",
            scope="local-private",
            subject="BTC",
            summary="Accepted paper decision.",
            evidence_hash="sha256:test",
            source="test",
            created_at=FIXED,
            expires_at=FIXED + timedelta(days=1),
            metadata={"price": 40500.0},
        )


def test_memory_supports_required_entry_kinds() -> None:
    entries = [
        *extract_from_decisions([decision(allowed=True), decision("ETH", allowed=False)], now=FIXED),
        regime_memory(symbol="SOL", regime="PAPER", source="scenario:test", confidence=1.0, now=FIXED),
        operator_memory(
            action="/pause-entries",
            risk_direction="reduces",
            ok=True,
            source="api:/operator/events",
            now=FIXED,
        ),
    ]

    assert {entry.kind for entry in entries} == {
        "signal",
        "strategy_reference",
        "regime",
        "operator",
    }


def test_knowledge_markdown_is_generated_from_active_memory() -> None:
    entries = extract_from_decisions([decision()], now=FIXED)
    markdown = knowledge_markdown(entries, generated_at=FIXED)

    assert markdown.startswith("# ZERO Local Knowledge")
    assert "schema_version: zero.memory.knowledge.v1" in markdown
    assert "signal: 1" in markdown
    assert "40500" not in markdown


def test_memory_cli_extract_status_and_knowledge(tmp_path, capsys) -> None:
    decisions = tmp_path / "decisions.jsonl"
    store = tmp_path / "memory.jsonl"
    knowledge = tmp_path / "knowledge.md"
    decisions.write_text(json.dumps(decision()) + "\n", encoding="utf-8")

    extract_code = main(
        [
            "extract",
            "--decisions",
            str(decisions),
            "--store",
            str(store),
            "--knowledge",
            str(knowledge),
            "--now",
            "2026-05-01T00:00:00Z",
        ]
    )
    extract_payload = json.loads(capsys.readouterr().out)
    status_code = main(["status", "--store", str(store), "--now", "2026-05-01T00:00:00Z"])
    status_payload = json.loads(capsys.readouterr().out)

    assert extract_code == 0
    assert status_code == 0
    assert extract_payload["appended_entries"] == 1
    assert status_payload["active_entries"] == 1
    assert knowledge.exists()
    assert "40500" not in knowledge.read_text(encoding="utf-8")
