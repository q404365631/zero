from __future__ import annotations

from dataclasses import dataclass
from typing import Any


SCHEMA_VERSION = "zero.live_canary_policy.v1"
POLICY_VERSION = "zero.live_canary_policy.public.v1"
DEFAULT_LAUNCH_WINDOW_SECONDS = 600
REQUIRED_RISK_REDUCING_STEPS = (
    "07_live_pause",
    "08_live_flatten",
    "09_live_kill",
)
REQUIRED_CANARY_EVIDENCE = (
    "live_preflight",
    "live_cockpit",
    "live_certification",
    "hl_reconcile",
    "live_execution_receipts",
    "live_evidence",
    "exchange_evidence",
    "operator_report",
    "operator_verification",
)


JsonMap = dict[str, Any]


@dataclass(frozen=True)
class CanaryInputs:
    generated_at: str
    mode: str
    risk_ready: bool
    preflight_ready: bool
    controls_ready: bool
    cockpit_risk_increasing_allowed: bool
    certification_passed: bool
    live_start_certified: bool
    live_order_attempted: bool
    live_order_accepted: bool
    live_order_reason: str
    receipts_total: int | None
    receipts_accepted: int
    evidence_hash: str | None
    exchange_evidence_attached: bool
    exchange_evidence_required: bool
    operator_report_ok: bool | None
    verifier_ok: bool | None
    risk_reducing_steps_captured: tuple[str, ...]
    operator_context: JsonMap | None = None
    request: JsonMap | None = None


def phase(name: str, status: str, detail: str, **evidence: Any) -> JsonMap:
    return {
        "name": name,
        "status": status,
        "detail": detail,
        "evidence": evidence,
    }


def int_or_none(value: Any) -> int | None:
    try:
        if value is None:
            return None
        return int(value)
    except (TypeError, ValueError):
        return None


def bool_value(value: Any) -> bool:
    return bool(value)


def inputs_from_runtime(
    *,
    generated_at: str,
    preflight: JsonMap,
    cockpit: JsonMap,
    certification: JsonMap,
    evidence: JsonMap,
    operator_context: JsonMap | None = None,
) -> CanaryInputs:
    summary = evidence.get("summary") if isinstance(evidence.get("summary"), dict) else {}
    receipts_accepted = int_or_none(summary.get("live_receipts_accepted")) or 0
    return CanaryInputs(
        generated_at=generated_at,
        mode="runtime-readiness",
        risk_ready=bool_value(cockpit.get("risk_increasing_allowed")),
        preflight_ready=bool_value(preflight.get("ready")),
        controls_ready=bool_value(preflight.get("controls_ready")),
        cockpit_risk_increasing_allowed=bool_value(cockpit.get("risk_increasing_allowed")),
        certification_passed=bool_value(certification.get("passed")),
        live_start_certified=bool_value(certification.get("live_start_certified")),
        live_order_attempted=False,
        live_order_accepted=False,
        live_order_reason="not_attempted",
        receipts_total=int_or_none(summary.get("live_receipts_total")),
        receipts_accepted=receipts_accepted,
        evidence_hash=str(evidence.get("evidence_hash") or "") or None,
        exchange_evidence_attached=False,
        exchange_evidence_required=receipts_accepted > 0,
        operator_report_ok=None,
        verifier_ok=None,
        risk_reducing_steps_captured=(),
        operator_context=operator_context,
    )


def inputs_from_rehearsal(
    manifest: JsonMap,
    *,
    operator_report: JsonMap | None = None,
    verifier_report: JsonMap | None = None,
    generated_at: str | None = None,
) -> CanaryInputs:
    collector = manifest.get("collector") if isinstance(manifest.get("collector"), dict) else {}
    summary = manifest.get("summary") if isinstance(manifest.get("summary"), dict) else {}
    steps = manifest.get("steps") if isinstance(manifest.get("steps"), list) else []
    step_names = tuple(str(step.get("name")) for step in steps if isinstance(step, dict))
    report_summary = (
        operator_report.get("summary")
        if operator_report is not None and isinstance(operator_report.get("summary"), dict)
        else {}
    )
    return CanaryInputs(
        generated_at=generated_at or str(manifest.get("generated_at") or ""),
        mode=str(collector.get("mode") or summary.get("mode") or "unknown"),
        risk_ready=bool_value(summary.get("risk_ready")),
        preflight_ready=bool_value(summary.get("preflight_ready")),
        controls_ready=bool_value(summary.get("controls_ready")),
        cockpit_risk_increasing_allowed=bool_value(summary.get("cockpit_risk_increasing_allowed")),
        certification_passed=bool_value(summary.get("certification_passed")),
        live_start_certified=bool_value(summary.get("live_start_certified")),
        live_order_attempted=bool_value(summary.get("live_order_attempted")),
        live_order_accepted=bool_value(summary.get("live_order_accepted")),
        live_order_reason=str(summary.get("live_order_reason") or "unknown"),
        receipts_total=int_or_none(summary.get("receipts_total")),
        receipts_accepted=int_or_none(summary.get("receipts_accepted")) or 0,
        evidence_hash=str(summary.get("evidence_hash") or "") or None,
        exchange_evidence_attached=bool_value(report_summary.get("exchange_evidence_attached")),
        exchange_evidence_required=bool_value(report_summary.get("exchange_evidence_required"))
        or (int_or_none(summary.get("receipts_accepted")) or 0) > 0,
        operator_report_ok=None if operator_report is None else bool_value(operator_report.get("ok")),
        verifier_ok=None if verifier_report is None else bool_value(verifier_report.get("ok")),
        risk_reducing_steps_captured=tuple(
            name for name in REQUIRED_RISK_REDUCING_STEPS if name in step_names
        ),
        operator_context=manifest.get("operator") if isinstance(manifest.get("operator"), dict) else None,
        request=manifest.get("request") if isinstance(manifest.get("request"), dict) else None,
    )


def build_live_canary_policy(inputs: CanaryInputs) -> JsonMap:
    operator_ok = True if inputs.operator_report_ok is None else inputs.operator_report_ok
    verifier_ok = True if inputs.verifier_ok is None else inputs.verifier_ok
    all_risk_reducers = all(step in inputs.risk_reducing_steps_captured for step in REQUIRED_RISK_REDUCING_STEPS)
    publishable = bool(
        inputs.live_order_accepted
        and inputs.exchange_evidence_attached
        and operator_ok
        and verifier_ok
        and all_risk_reducers
    )
    refusal_qualified = bool(
        inputs.mode == "refusal"
        and inputs.live_order_attempted
        and not inputs.live_order_accepted
        and operator_ok
        and verifier_ok
    )
    qualification = publishable or refusal_qualified

    phases = [
        phase(
            "readiness",
            "pass" if inputs.risk_ready else "blocked",
            "live gates allow a tiny-capital canary"
            if inputs.risk_ready
            else "live gates are not ready for risk-increasing canary mode",
            preflight_ready=inputs.preflight_ready,
            controls_ready=inputs.controls_ready,
            cockpit_risk_increasing_allowed=inputs.cockpit_risk_increasing_allowed,
            certification_passed=inputs.certification_passed,
        ),
        phase(
            "policy_arm",
            "armed" if inputs.mode == "canary" and inputs.risk_ready else "disarmed",
            "operator requested canary mode and readiness gates passed"
            if inputs.mode == "canary" and inputs.risk_ready
            else "policy remains disarmed outside ready canary mode",
            mode=inputs.mode,
            requires_explicit_confirmation=True,
        ),
        phase(
            "bounded_launch_window",
            "pass" if inputs.mode == "canary" and inputs.live_order_attempted else "not_open",
            "canary attempt occurred inside the captured operator workflow"
            if inputs.mode == "canary" and inputs.live_order_attempted
            else "no bounded live launch window was opened",
            window_seconds=DEFAULT_LAUNCH_WINDOW_SECONDS,
            live_order_attempted=inputs.live_order_attempted,
        ),
        phase(
            "evidence_export",
            "pass" if inputs.evidence_hash else "fail",
            "hash-only evidence bundle captured"
            if inputs.evidence_hash
            else "live evidence hash missing",
            evidence_hash=inputs.evidence_hash,
            receipts_total=inputs.receipts_total,
            receipts_accepted=inputs.receipts_accepted,
        ),
        phase(
            "shadow_review",
            "pass" if operator_ok and verifier_ok else "blocked",
            "operator report and verifier are clean"
            if operator_ok and verifier_ok
            else "operator report or verifier failed",
            operator_report_ok=inputs.operator_report_ok,
            verifier_ok=inputs.verifier_ok,
        ),
        phase(
            "qualification",
            "pass" if qualification else "blocked",
            qualification_detail(inputs, publishable=publishable, refusal_qualified=refusal_qualified),
            publishable_canary_evidence=publishable,
            refusal_evidence_qualified=refusal_qualified,
            exchange_evidence_attached=inputs.exchange_evidence_attached,
        ),
        phase(
            "follow_through_review",
            "pass" if (not inputs.live_order_accepted or all_risk_reducers) else "blocked",
            "no live risk was accepted; follow-through controls were not required"
            if not inputs.live_order_accepted
            else "pause, flatten, and kill follow-through controls were captured"
            if all_risk_reducers
            else "accepted live risk requires pause, flatten, and kill follow-through evidence",
            required_steps=list(REQUIRED_RISK_REDUCING_STEPS),
            captured_steps=list(inputs.risk_reducing_steps_captured),
        ),
    ]
    recommendation = recommendation_for(inputs, publishable=publishable, refusal_qualified=refusal_qualified)
    return {
        "schema_version": SCHEMA_VERSION,
        "policy_version": POLICY_VERSION,
        "generated_at": inputs.generated_at,
        "mode": inputs.mode,
        "summary": {
            "ready_for_canary": inputs.risk_ready,
            "policy_armed": inputs.mode == "canary" and inputs.risk_ready,
            "live_order_attempted": inputs.live_order_attempted,
            "live_order_accepted": inputs.live_order_accepted,
            "receipts_accepted": inputs.receipts_accepted,
            "exchange_evidence_attached": inputs.exchange_evidence_attached,
            "publishable_canary_evidence": publishable,
            "refusal_evidence_qualified": refusal_qualified,
            "qualified": qualification,
            "next_step": recommendation["action"],
        },
        "policy": {
            "default_state": "disarmed",
            "arm_requires": [
                "ready live preflight",
                "risk-increasing cockpit allowance",
                "passing dry-run live certification",
                "operator-owned custody",
                "exact live-risk confirmation phrase",
            ],
            "disarm_after": [
                "canary attempt completed",
                "pause captured",
                "flatten captured",
                "kill captured",
                "evidence exported",
                "operator report written",
            ],
            "launch_window_seconds": DEFAULT_LAUNCH_WINDOW_SECONDS,
            "tiny_capital_only": True,
            "requires_exchange_evidence_for_accepted_receipts": True,
            "required_evidence": list(REQUIRED_CANARY_EVIDENCE),
        },
        "phases": phases,
        "recommendation": recommendation,
        "operator_context": inputs.operator_context,
        "request": inputs.request,
        "privacy": {
            "contains_exchange_credentials": False,
            "contains_wallet_material": False,
            "contains_raw_exchange_order_ids": False,
            "contains_raw_client_order_ids": False,
            "contains_idempotency_tokens": False,
            "contains_confirmation_phrase": False,
            "contains_private_notes": False,
        },
    }


def qualification_detail(inputs: CanaryInputs, *, publishable: bool, refusal_qualified: bool) -> str:
    if publishable:
        return "accepted live canary has exchange evidence, clean review, and follow-through controls"
    if refusal_qualified:
        return "refusal-mode bundle qualifies as fail-closed public proof, not live trading proof"
    if inputs.live_order_accepted and not inputs.exchange_evidence_attached:
        return "accepted live canary is missing exchange-side evidence"
    if inputs.mode == "canary" and not inputs.risk_ready:
        return "canary mode was requested before readiness gates passed"
    return "bundle is not qualified as publishable live canary evidence"


def recommendation_for(inputs: CanaryInputs, *, publishable: bool, refusal_qualified: bool) -> JsonMap:
    if publishable:
        return {
            "action": "publish_live_canary_evidence_for_review",
            "risk_direction": "none",
            "reason": "operator-owned canary evidence is complete and public-safe",
        }
    if inputs.live_order_accepted and not inputs.exchange_evidence_attached:
        return {
            "action": "attach_exchange_evidence_before_public_claims",
            "risk_direction": "none",
            "reason": "accepted live receipts require matching Hyperliquid records",
        }
    if inputs.mode == "canary" and not inputs.risk_ready:
        return {
            "action": "do_not_retry_canary_until_preflight_ready",
            "risk_direction": "down",
            "reason": "risk gates were not ready when canary mode was requested",
        }
    if refusal_qualified:
        return {
            "action": "keep_public_claim_at_refusal_proof",
            "risk_direction": "none",
            "reason": "fail-closed evidence is valid but does not prove live execution",
        }
    if inputs.mode == "collect-only":
        return {
            "action": "review_readiness_then_decide_whether_to_arm_canary",
            "risk_direction": "none",
            "reason": "collect-only mode captured state without attempting live risk",
        }
    if not inputs.risk_ready:
        return {
            "action": "fix_live_preflight_before_canary",
            "risk_direction": "down",
            "reason": "preflight, immune, reconciliation, journal, or emergency controls are not ready",
        }
    return {
        "action": "arm_tiny_capital_canary_only_with_operator_confirmation",
        "risk_direction": "up",
        "reason": "readiness appears sufficient but live risk still requires explicit local approval",
    }
