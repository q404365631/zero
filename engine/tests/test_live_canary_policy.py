from __future__ import annotations

from zero_engine.live_canary_policy import build_live_canary_policy, inputs_from_rehearsal


def manifest(
    *,
    mode: str = "refusal",
    risk_ready: bool = False,
    attempted: bool = True,
    accepted: bool = False,
    receipts_accepted: int = 0,
    steps: list[str] | None = None,
) -> dict[str, object]:
    return {
        "schema_version": "zero.live_canary_rehearsal.v1",
        "generated_at": "2026-05-01T00:00:00Z",
        "collector": {"mode": mode},
        "operator": {"handle": "ops", "scope": "local-private"},
        "request": {
            "symbol": "BTC",
            "side": "buy",
            "size": 0.001,
            "idempotency_key": "IDEMPOTENCY_KEY_REDACTED",
        },
        "summary": {
            "mode": mode,
            "risk_ready": risk_ready,
            "preflight_ready": risk_ready,
            "controls_ready": risk_ready,
            "cockpit_risk_increasing_allowed": risk_ready,
            "certification_passed": True,
            "live_start_certified": True,
            "live_order_attempted": attempted,
            "live_order_accepted": accepted,
            "live_order_reason": "submitted" if accepted else "live executor not configured",
            "receipts_total": receipts_accepted,
            "receipts_accepted": receipts_accepted,
            "evidence_hash": "sha256:" + "a" * 64,
        },
        "steps": [{"name": name, "file": f"{name}.json", "status": 200} for name in (steps or [])],
    }


def operator_report(
    *,
    ok: bool = True,
    accepted: bool = False,
    exchange_attached: bool = True,
) -> dict[str, object]:
    return {
        "schema_version": "zero.live_canary_operator.v1",
        "ok": ok,
        "summary": {
            "live_order_accepted": accepted,
            "receipts_accepted": 1 if accepted else 0,
            "exchange_evidence_attached": exchange_attached,
            "exchange_evidence_required": accepted,
            "publishable_canary_evidence": accepted and exchange_attached and ok,
        },
        "failures": [] if ok else ["verification failed"],
    }


def test_refusal_policy_qualifies_fail_closed_evidence_without_live_claim() -> None:
    policy = build_live_canary_policy(
        inputs_from_rehearsal(manifest(), operator_report=operator_report())
    )

    assert policy["schema_version"] == "zero.live_canary_policy.v1"
    assert policy["summary"]["qualified"] is True
    assert policy["summary"]["refusal_evidence_qualified"] is True
    assert policy["summary"]["publishable_canary_evidence"] is False
    assert policy["recommendation"]["action"] == "keep_public_claim_at_refusal_proof"
    assert policy["phases"][0]["status"] == "blocked"


def test_accepted_canary_requires_exchange_evidence_and_follow_through() -> None:
    base_manifest = manifest(
        mode="canary",
        risk_ready=True,
        attempted=True,
        accepted=True,
        receipts_accepted=1,
        steps=["06_live_execute_canary"],
    )

    policy = build_live_canary_policy(
        inputs_from_rehearsal(
            base_manifest,
            operator_report=operator_report(accepted=True, exchange_attached=False),
        )
    )

    assert policy["summary"]["qualified"] is False
    assert policy["recommendation"]["action"] == "attach_exchange_evidence_before_public_claims"
    follow_through = {phase["name"]: phase for phase in policy["phases"]}["follow_through_review"]
    assert follow_through["status"] == "blocked"


def test_accepted_canary_with_exchange_and_follow_through_is_publishable() -> None:
    policy = build_live_canary_policy(
        inputs_from_rehearsal(
            manifest(
                mode="canary",
                risk_ready=True,
                attempted=True,
                accepted=True,
                receipts_accepted=1,
                steps=["07_live_pause", "08_live_flatten", "09_live_kill"],
            ),
            operator_report=operator_report(accepted=True, exchange_attached=True),
        )
    )

    assert policy["summary"]["qualified"] is True
    assert policy["summary"]["publishable_canary_evidence"] is True
    assert policy["recommendation"]["action"] == "publish_live_canary_evidence_for_review"
    phases = {phase["name"]: phase for phase in policy["phases"]}
    assert phases["policy_arm"]["status"] == "armed"
    assert phases["bounded_launch_window"]["status"] == "pass"
    assert phases["follow_through_review"]["status"] == "pass"
