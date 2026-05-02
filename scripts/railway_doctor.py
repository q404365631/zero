#!/usr/bin/env python3
"""Remote deployment doctor for ZERO Railway-style paper services."""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any


SCHEMA_VERSION = "zero.railway_doctor.v1"
DEFAULT_TIMEOUT_SECONDS = 8.0


@dataclass(frozen=True)
class HttpResult:
    status: int
    headers: dict[str, str]
    payload: dict[str, Any] | list[Any] | None
    raw: str
    error: str | None = None


@dataclass(frozen=True)
class Check:
    name: str
    status: str
    message: str
    evidence: dict[str, Any]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Check a remote ZERO Railway paper deployment for health, recovery, "
            "public packet privacy, live-mode refusal, and Intelligence API gates."
        )
    )
    parser.add_argument(
        "url",
        nargs="?",
        default=os.environ.get("ZERO_RAILWAY_URL", ""),
        help="Base URL for the deployment. Defaults to ZERO_RAILWAY_URL.",
    )
    parser.add_argument(
        "--token",
        default=os.environ.get("ZERO_INTELLIGENCE_API_TOKEN", ""),
        help="Optional ZERO Intelligence API bearer token for paid-scope checks.",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_SECONDS,
        help="HTTP timeout per request in seconds.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON instead of text.",
    )
    parser.add_argument(
        "--expect-paper",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Require public deployment safety posture: no live executor and risk-up refused.",
    )
    parser.add_argument(
        "--fail-on-warn",
        action="store_true",
        help="Exit nonzero when warning checks are present.",
    )
    return parser.parse_args()


def normalize_base_url(raw_url: str) -> str:
    if not raw_url:
        raise ValueError("missing deployment URL; pass a URL or set ZERO_RAILWAY_URL")
    parsed = urllib.parse.urlparse(raw_url)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise ValueError(f"invalid deployment URL: {raw_url!r}")
    return raw_url.rstrip("/")


def request_json(
    base_url: str,
    path: str,
    *,
    timeout: float,
    token: str = "",
) -> HttpResult:
    headers = {"accept": "application/json", "user-agent": "zero-railway-doctor/1"}
    if token:
        headers["authorization"] = f"Bearer {token}"
    request = urllib.request.Request(f"{base_url}{path}", headers=headers, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            raw = response.read().decode("utf-8", errors="replace")
            return HttpResult(
                status=response.status,
                headers={k.lower(): v for k, v in response.headers.items()},
                payload=parse_payload(raw),
                raw=raw,
            )
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        return HttpResult(
            status=exc.code,
            headers={k.lower(): v for k, v in exc.headers.items()},
            payload=parse_payload(raw),
            raw=raw,
            error=str(exc),
        )
    except (OSError, TimeoutError, urllib.error.URLError) as exc:
        return HttpResult(status=0, headers={}, payload=None, raw="", error=str(exc))


def parse_payload(raw: str) -> dict[str, Any] | list[Any] | None:
    if not raw:
        return None
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        return None
    if isinstance(payload, (dict, list)):
        return payload
    return None


def as_dict(result: HttpResult) -> dict[str, Any]:
    if isinstance(result.payload, dict):
        return result.payload
    return {}


def raw_contains(result: HttpResult, needles: tuple[str, ...]) -> bool:
    raw = result.raw.lower()
    return any(needle.lower() in raw for needle in needles)


def check_schema(packet: dict[str, Any], schema: str) -> bool:
    return packet.get("schema_version") == schema


def ok(name: str, message: str, **evidence: Any) -> Check:
    return Check(name=name, status="ok", message=message, evidence=evidence)


def warn(name: str, message: str, **evidence: Any) -> Check:
    return Check(name=name, status="warn", message=message, evidence=evidence)


def fail(name: str, message: str, **evidence: Any) -> Check:
    return Check(name=name, status="fail", message=message, evidence=evidence)


def status_check(name: str, result: HttpResult, schema: str | None = None) -> Check:
    packet = as_dict(result)
    if result.status != 200:
        return fail(name, f"expected HTTP 200, got {result.status}", error=result.error)
    if schema and not check_schema(packet, schema):
        return fail(
            name,
            f"expected schema {schema}",
            schema_version=packet.get("schema_version"),
        )
    return ok(name, "reachable", status=result.status, schema_version=packet.get("schema_version"))


def run_checks(base_url: str, *, token: str, timeout: float, expect_paper: bool) -> list[Check]:
    checks: list[Check] = []

    health = request_json(base_url, "/health", timeout=timeout)
    health_packet = as_dict(health)
    if health.status == 200 and health_packet.get("status") == "ok":
        checks.append(
            ok(
                "health",
                "service is healthy",
                recovery=health_packet.get("recovery", {}).get("status"),
                market_data=health_packet.get("market_data", {}).get("source"),
            )
        )
    else:
        checks.append(
            fail(
                "health",
                "service health is not ok",
                status=health.status,
                api_status=health_packet.get("status"),
                error=health.error,
            )
        )

    recovery = health_packet.get("recovery", {})
    if recovery.get("durable") is True:
        checks.append(ok("durable_journal", "deployment reports durable recovery", path=recovery.get("path")))
    elif recovery:
        checks.append(
            warn(
                "durable_journal",
                "journal is ephemeral; attach a Railway volume before public demos",
                status=recovery.get("status"),
                path=recovery.get("path"),
            )
        )
    else:
        checks.append(warn("durable_journal", "health response did not include recovery metadata"))

    v2_status = request_json(base_url, "/v2/status", timeout=timeout)
    v2_packet = as_dict(v2_status)
    if v2_status.status == 200 and isinstance(v2_packet.get("recovery"), dict):
        checks.append(
            ok(
                "v2_status",
                "operator status exposes recovery metadata",
                recovery=v2_packet.get("recovery", {}).get("status"),
                open_positions=v2_packet.get("positions", {}).get("open"),
            )
        )
    else:
        checks.append(
            fail(
                "v2_status",
                "operator status is unavailable or missing recovery metadata",
                status=v2_status.status,
                error=v2_status.error,
            )
        )

    quote = request_json(base_url, "/market/quote?symbol=BTC", timeout=timeout)
    quote_packet = as_dict(quote)
    if quote.status == 200 and quote_packet.get("source") in {"paper:static", "hyperliquid:allMids"}:
        checks.append(
            ok(
                "market_quote",
                "BTC quote source is explicit",
                source=quote_packet.get("source"),
                live=quote_packet.get("live"),
            )
        )
    else:
        checks.append(
            fail(
                "market_quote",
                "BTC quote failed or source is unknown",
                status=quote.status,
                source=quote_packet.get("source"),
                error=quote.error,
            )
        )

    metrics = request_json(base_url, "/metrics", timeout=timeout)
    metrics_packet = as_dict(metrics)
    if metrics.status == 200 and check_schema(metrics_packet, "zero.metrics.v1"):
        checks.append(
            ok(
                "metrics",
                "metrics packet is versioned",
                execute_count=metrics_packet.get("api", {}).get("execute_count"),
                event_count=metrics_packet.get("runtime_bus", {}).get("event_count"),
            )
        )
    else:
        checks.append(status_check("metrics", metrics, "zero.metrics.v1"))

    immune = request_json(base_url, "/immune", timeout=timeout)
    immune_packet = as_dict(immune)
    if immune.status == 200 and check_schema(immune_packet, "zero.immune.v1"):
        risk_allowed = immune_packet.get("risk_increasing_allowed")
        if expect_paper and risk_allowed is not False:
            checks.append(
                fail(
                    "immune",
                    "paper deployment must not allow risk-increasing actions",
                    risk_increasing_allowed=risk_allowed,
                )
            )
        else:
            checks.append(
                ok(
                    "immune",
                    "immune packet is fail-closed for paper",
                    risk_increasing_allowed=risk_allowed,
                    risk_blocking=immune_packet.get("summary", {}).get("risk_blocking"),
                )
            )
    else:
        checks.append(status_check("immune", immune, "zero.immune.v1"))

    preflight = request_json(base_url, "/live/preflight", timeout=timeout)
    preflight_packet = as_dict(preflight)
    if preflight.status == 200 and check_schema(preflight_packet, "zero.live_preflight.v1"):
        if expect_paper and (
            preflight_packet.get("ready") is not False
            or preflight_packet.get("live_mode") != "refused"
        ):
            checks.append(
                fail(
                    "live_preflight",
                    "paper deployment must refuse live mode",
                    ready=preflight_packet.get("ready"),
                    live_mode=preflight_packet.get("live_mode"),
                )
            )
        else:
            checks.append(
                ok(
                    "live_preflight",
                    "live mode is refused",
                    ready=preflight_packet.get("ready"),
                    live_mode=preflight_packet.get("live_mode"),
                )
            )
    else:
        checks.append(status_check("live_preflight", preflight, "zero.live_preflight.v1"))

    cockpit = request_json(base_url, "/live/cockpit", timeout=timeout)
    cockpit_packet = as_dict(cockpit)
    if cockpit.status == 200 and check_schema(cockpit_packet, "zero.live_cockpit.v1"):
        if expect_paper and cockpit_packet.get("ready") is not False:
            checks.append(fail("live_cockpit", "paper cockpit unexpectedly reports ready"))
        else:
            checks.append(
                ok(
                    "live_cockpit",
                    "cockpit exposes refusal and next action",
                    ready=cockpit_packet.get("ready"),
                    next_action=cockpit_packet.get("next_action"),
                )
            )
    else:
        checks.append(status_check("live_cockpit", cockpit, "zero.live_cockpit.v1"))

    profile = request_json(base_url, "/network/profile", timeout=timeout)
    profile_packet = as_dict(profile)
    if profile.status == 200 and check_schema(profile_packet, "zero.network.profile.v1"):
        if raw_contains(profile, ("trace-", "idempotency_key", "private_key")):
            checks.append(fail("network_profile_privacy", "public profile leaked private runtime data"))
        else:
            checks.append(
                ok(
                    "network_profile_privacy",
                    "public profile is redacted",
                    publish_enabled=profile_packet.get("profile", {}).get("publish_enabled"),
                    proof_hash=profile_packet.get("proof", {}).get("proof_hash"),
                )
            )
    else:
        checks.append(status_check("network_profile", profile, "zero.network.profile.v1"))

    snapshot = request_json(base_url, "/intelligence/snapshot", timeout=timeout)
    snapshot_packet = as_dict(snapshot)
    if snapshot.status == 200 and check_schema(snapshot_packet, "zero.intelligence.snapshot.v1"):
        if raw_contains(snapshot, ("trace-", "idempotency_key", "private_key")):
            checks.append(fail("intelligence_snapshot_privacy", "delayed snapshot leaked private runtime data"))
        elif snapshot_packet.get("access", {}).get("class") != "public_delayed":
            checks.append(
                fail(
                    "intelligence_snapshot_privacy",
                    "public intelligence snapshot must remain delayed",
                    access_class=snapshot_packet.get("access", {}).get("class"),
                )
            )
        else:
            checks.append(
                ok(
                    "intelligence_snapshot_privacy",
                    "delayed public intelligence is redacted",
                    access_class=snapshot_packet.get("access", {}).get("class"),
                )
            )
    else:
        checks.append(status_check("intelligence_snapshot", snapshot, "zero.intelligence.snapshot.v1"))

    for name, path, schema in (
        ("intelligence_catalog", "/intelligence/catalog", "zero.intelligence.catalog.v1"),
        ("intelligence_commercial", "/intelligence/commercial", "zero.intelligence.commercial.v1"),
        ("model_gateway", "/intelligence/model-gateway", "zero.model_gateway.status.v1"),
        ("model_gateway_health", "/intelligence/model-gateway/health", "zero.model_gateway.health.v1"),
        ("model_gateway_audit", "/intelligence/model-gateway/audit", "zero.model_gateway.audit.v1"),
    ):
        checks.append(status_check(name, request_json(base_url, path, timeout=timeout), schema))

    hosted_snapshot = request_json(base_url, "/v1/intelligence/snapshots", timeout=timeout)
    hosted_packet = as_dict(hosted_snapshot)
    rate_policy = hosted_snapshot.headers.get("x-zero-ratelimit-policy")
    hosted_leak_needles = ("trace-", "private_key", token) if token else ("trace-", "private_key")
    if hosted_snapshot.status == 200 and check_schema(
        hosted_packet, "zero.intelligence.hosted.snapshots.v1"
    ):
        if not rate_policy:
            checks.append(fail("hosted_snapshot_headers", "hosted snapshot omitted rate-limit policy header"))
        elif raw_contains(hosted_snapshot, hosted_leak_needles):
            checks.append(fail("hosted_snapshot_headers", "hosted snapshot leaked private data"))
        else:
            checks.append(
                ok(
                    "hosted_snapshot_headers",
                    "hosted-compatible delayed snapshot is public and rate-limited",
                    rate_policy=rate_policy,
                    freshness=hosted_packet.get("access", {}).get("freshness"),
                )
            )
    else:
        checks.append(
            status_check(
                "hosted_snapshot_headers",
                hosted_snapshot,
                "zero.intelligence.hosted.snapshots.v1",
            )
        )

    unauth_history = request_json(base_url, "/v1/intelligence/history?limit=1", timeout=timeout)
    unauth_packet = as_dict(unauth_history)
    if unauth_history.status in {401, 403} and check_schema(
        unauth_packet, "zero.intelligence.hosted_error.v1"
    ):
        checks.append(
            ok(
                "paid_scopes_fail_closed",
                "history scope refuses unauthenticated access",
                status=unauth_history.status,
                error=unauth_packet.get("error"),
            )
        )
    else:
        checks.append(
            fail(
                "paid_scopes_fail_closed",
                "history scope must fail closed without a bearer token",
                status=unauth_history.status,
                schema_version=unauth_packet.get("schema_version"),
            )
        )

    if token:
        paid_history = request_json(
            base_url,
            "/v1/intelligence/history?limit=1",
            timeout=timeout,
            token=token,
        )
        paid_packet = as_dict(paid_history)
        if paid_history.status == 200 and check_schema(
            paid_packet, "zero.intelligence.hosted.history.v1"
        ):
            if raw_contains(paid_history, ("trace-", "private_key", token)):
                checks.append(fail("paid_history_auth", "paid history response leaked private data"))
            else:
                checks.append(
                    ok(
                        "paid_history_auth",
                        "paid history scope accepts configured token",
                        account=paid_packet.get("account", {}).get("id"),
                        plan=paid_packet.get("account", {}).get("plan"),
                    )
                )
        else:
            checks.append(
                fail(
                    "paid_history_auth",
                    "paid history scope rejected the supplied token",
                    status=paid_history.status,
                    schema_version=paid_packet.get("schema_version"),
                    error=paid_history.error,
                )
            )
    else:
        checks.append(
            warn(
                "paid_history_auth",
                "skipped tokened paid-scope check; pass --token or ZERO_INTELLIGENCE_API_TOKEN",
            )
        )

    return checks


def build_report(base_url: str, checks: list[Check]) -> dict[str, Any]:
    counts = {
        "ok": sum(1 for check in checks if check.status == "ok"),
        "warn": sum(1 for check in checks if check.status == "warn"),
        "fail": sum(1 for check in checks if check.status == "fail"),
    }
    if counts["fail"]:
        status = "fail"
    elif counts["warn"]:
        status = "warn"
    else:
        status = "ok"
    return {
        "schema_version": SCHEMA_VERSION,
        "target": base_url,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "summary": {"status": status, **counts},
        "checks": [
            {
                "name": check.name,
                "status": check.status,
                "message": check.message,
                "evidence": check.evidence,
            }
            for check in checks
        ],
    }


def emit_text(report: dict[str, Any]) -> None:
    summary = report["summary"]
    print(
        f"zero railway doctor: {summary['status']} "
        f"({summary['ok']} ok, {summary['warn']} warn, {summary['fail']} fail)"
    )
    print(f"target: {report['target']}")
    for check in report["checks"]:
        print(f"{check['status']:>4} {check['name']}: {check['message']}")


def main() -> int:
    args = parse_args()
    try:
        base_url = normalize_base_url(args.url)
    except ValueError as exc:
        print(f"zero railway doctor: {exc}", file=sys.stderr)
        return 2

    checks = run_checks(
        base_url,
        token=args.token,
        timeout=args.timeout,
        expect_paper=args.expect_paper,
    )
    report = build_report(base_url, checks)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        emit_text(report)

    summary = report["summary"]
    if summary["fail"] or (args.fail_on_warn and summary["warn"]):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
