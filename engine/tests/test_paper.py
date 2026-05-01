from zero_engine.models import OrderIntent, RiskLimits, Side
from zero_engine.journal import DecisionJournal
from zero_engine.paper import PaperEngine


def test_paper_engine_records_fill() -> None:
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=1_000))
    decision = engine.submit(OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9))

    assert decision.allowed
    assert len(engine.fills) == 1
    assert engine.positions["BTC"].quantity == 0.01
    assert len(engine.decisions) == 1
    assert engine.decisions[0].decision.allowed


def test_paper_engine_records_rejection() -> None:
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=100))
    decision = engine.submit(OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9))

    assert not decision.allowed
    assert len(engine.rejections) == 1
    assert not engine.fills
    assert engine.decisions[0].to_dict()["reason"] == "order notional exceeds limit"


def test_paper_engine_records_source_in_decision_log() -> None:
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=1_000), clock=lambda: 123.0)
    engine.submit(
        OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9),
        source="strategy:test",
        trace_id="trace-paper-test",
    )

    record = engine.decisions[0].to_dict()

    assert record["source"] == "strategy:test"
    assert record["trace_id"] == "trace-paper-test"
    assert record["as_of"] == 123.0
    assert record["symbol"] == "BTC"
    assert record["allowed"] is True


def test_paper_engine_appends_decision_journal(tmp_path) -> None:
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    engine = PaperEngine(limits=RiskLimits(max_notional_usd=100), clock=lambda: 456.0, journal=journal)

    engine.submit(
        OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9),
        source="strategy:test",
    )

    records = journal.tail()
    assert len(records) == 1
    assert records[0]["as_of"] == 456.0
    assert records[0]["source"] == "strategy:test"
    assert records[0]["allowed"] is False
    assert records[0]["reason"] == "order notional exceeds limit"


def test_paper_engine_recovers_positions_and_counts_from_journal(tmp_path) -> None:
    journal = DecisionJournal(tmp_path / "decisions.jsonl")
    first = PaperEngine(limits=RiskLimits(max_notional_usd=1_000), clock=lambda: 456.0, journal=journal)
    first.submit(
        OrderIntent("BTC", Side.BUY, quantity=0.01, price=40_000, confidence=0.9),
        source="api:/execute",
        idempotency_key="recover-fill",
    )
    first.submit(
        OrderIntent("ETH", Side.BUY, quantity=1.0, price=2_000, confidence=0.9),
        source="api:/execute",
        idempotency_key="recover-reject",
    )

    recovered = PaperEngine.recover_from_journal(
        journal,
        limits=RiskLimits(max_notional_usd=1_000),
        clock=lambda: 789.0,
    )

    assert recovered.positions["BTC"].quantity == 0.01
    assert recovered.positions["BTC"].avg_price == 40_000
    assert len(recovered.fills) == 1
    assert len(recovered.rejections) == 1
    assert recovered.recovery.status == "recovered"
    assert recovered.recovery.durable is True
    assert recovered.recovery.decisions_recovered == 2
    assert recovered.recovery.fills_recovered == 1
    assert recovered.recovery.rejections_recovered == 1
    assert recovered.recovery.positions_recovered == 1
