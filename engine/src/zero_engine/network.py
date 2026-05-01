from __future__ import annotations

import hashlib
import html
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from zero_engine.paper import PaperEngine

HANDLE_RE = re.compile(r"^[a-zA-Z0-9_-]{3,32}$")
PROFILE_SCHEMA_VERSION = "zero.network.profile.v1"
LEADERBOARD_SCHEMA_VERSION = "zero.network.leaderboard.v1"


@dataclass(frozen=True)
class PublicProfileConfig:
    handle: str = "local-operator"
    display_name: str | None = None
    publish_enabled: bool = False

    def __post_init__(self) -> None:
        if not HANDLE_RE.match(self.handle):
            raise ValueError("network handle must be 3-32 chars: letters, numbers, _ or -")
        if self.display_name is not None and len(self.display_name.strip()) > 80:
            raise ValueError("network display name must be 80 chars or fewer")


def public_profile(
    engine: PaperEngine,
    *,
    config: PublicProfileConfig | None = None,
    generated_at: str,
    mode: str = "paper",
    live_execution_count: int = 0,
) -> dict[str, Any]:
    cfg = config or PublicProfileConfig()
    metrics = public_metrics(engine, live_execution_count=live_execution_count)
    proof_payload = {
        "schema_version": "zero.network.proof.v1",
        "handle": cfg.handle,
        "mode": mode,
        "metrics": metrics,
    }
    proof_hash = sha256_json(proof_payload)
    profile = {
        "schema_version": PROFILE_SCHEMA_VERSION,
        "generated_at": generated_at,
        "mode": mode,
        "profile": {
            "handle": cfg.handle,
            "display_name": cfg.display_name or cfg.handle,
            "publish_enabled": cfg.publish_enabled,
        },
        "verification": {
            "status": "verified" if metrics["decisions"] else "empty",
            "proof_hash": proof_hash,
            "badges": verification_badges(mode, metrics, proof_hash),
        },
        "metrics": metrics,
        "privacy": privacy_policy(),
        "leaderboard_row": leaderboard_row(cfg.handle, mode, metrics, proof_hash),
    }
    assert_public_profile_safe(profile)
    return profile


def public_metrics(engine: PaperEngine, *, live_execution_count: int = 0) -> dict[str, Any]:
    decisions = len(engine.decisions)
    fills = len(engine.fills)
    rejections = len(engine.rejections)
    open_positions = len([p for p in engine.positions.values() if p.quantity != 0])
    accepted = len([record for record in engine.decisions if record.decision.allowed])
    total_notional = sum(record.intent.notional_usd for record in engine.decisions)
    rejection_rate = rejections / decisions if decisions else 0.0
    acceptance_rate = accepted / decisions if decisions else 0.0
    return {
        "decisions": decisions,
        "fills": fills,
        "rejections": rejections,
        "open_positions": open_positions,
        "acceptance_rate": round(acceptance_rate, 4),
        "rejection_rate": round(rejection_rate, 4),
        "total_notional_usd": round(total_notional, 2),
        "live_execution_count": live_execution_count,
        "journal_durable": engine.recovery.durable or engine.journal is not None,
    }


def verification_badges(
    mode: str,
    metrics: dict[str, Any],
    proof_hash: str,
) -> list[dict[str, Any]]:
    badges = [
        {
            "name": "paper_verified",
            "status": "verified" if metrics["decisions"] else "empty",
            "evidence": proof_hash,
        }
    ]
    if mode == "live" or metrics["live_execution_count"] > 0:
        badges.append(
            {
                "name": "live_observed",
                "status": "verified" if metrics["live_execution_count"] > 0 else "not_observed",
                "evidence": proof_hash,
            }
        )
    if metrics["journal_durable"]:
        badges.append({"name": "durable_journal", "status": "verified", "evidence": proof_hash})
    return badges


def leaderboard_row(
    handle: str,
    mode: str,
    metrics: dict[str, Any],
    proof_hash: str,
) -> dict[str, Any]:
    score = min(
        100.0,
        (metrics["decisions"] * 1.0)
        + (metrics["rejections"] * 1.5)
        + (10.0 if metrics["journal_durable"] else 0.0),
    )
    return {
        "handle": handle,
        "mode": mode,
        "decisions": metrics["decisions"],
        "rejection_rate": metrics["rejection_rate"],
        "open_positions": metrics["open_positions"],
        "verification_score": round(score, 2),
        "proof_hash": proof_hash,
    }


def public_leaderboard(
    profiles: list[dict[str, Any]] | tuple[dict[str, Any], ...],
    *,
    generated_at: str,
    limit: int = 100,
) -> dict[str, Any]:
    if limit <= 0:
        raise ValueError("leaderboard limit must be positive")
    rows = [_public_leaderboard_row(profile) for profile in profiles]
    rows.sort(
        key=lambda row: (
            -float(row["verification_score"]),
            -int(row["decisions"]),
            -float(row["rejection_rate"]),
            str(row["handle"]),
        )
    )
    ranked_rows = [
        {
            "rank": index,
            **row,
        }
        for index, row in enumerate(rows[:limit], start=1)
    ]
    payload = {
        "schema_version": LEADERBOARD_SCHEMA_VERSION,
        "generated_at": generated_at,
        "row_count": len(ranked_rows),
        "rows": ranked_rows,
        "rules": {
            "ranking": [
                "verification_score desc",
                "decisions desc",
                "rejection_rate desc",
                "handle asc",
            ],
            "purpose": "proof-of-process, not financial advice",
        },
        "privacy": privacy_policy(),
    }
    assert_public_profile_safe(payload)
    return payload


def public_profile_page(profile: dict[str, Any], *, generated_at: str) -> str:
    row = _public_leaderboard_row(profile)
    assert_public_profile_safe(profile)
    metrics = profile.get("metrics", {})
    verification = profile.get("verification", {})
    badges = verification.get("badges", [])
    if not isinstance(metrics, dict):
        raise ValueError("public profile metrics must be a JSON object")
    if not isinstance(badges, list):
        raise ValueError("public profile verification.badges must be a list")

    badge_items = "\n".join(
        f"""          <li><span>{_escape(str(badge.get("name", "unknown")))}</span><strong>{_escape(str(badge.get("status", "unknown")))}</strong></li>"""
        for badge in badges
        if isinstance(badge, dict)
    )
    metric_items = "\n".join(
        [
            _metric("Decisions", metrics.get("decisions", 0)),
            _metric("Fills", metrics.get("fills", 0)),
            _metric("Rejections", metrics.get("rejections", 0)),
            _metric("Rejection Rate", f"{float(metrics.get('rejection_rate', 0.0)):.2%}"),
            _metric("Open Positions", metrics.get("open_positions", 0)),
            _metric("Verification Score", row["verification_score"]),
        ]
    )
    proof_hash = _escape(row["proof_hash"])
    display_name = _escape(row["display_name"])
    handle = _escape(row["handle"])
    mode = _escape(row["mode"].upper())
    status = _escape(str(verification.get("status", "unknown")))
    timestamp = _escape(generated_at)
    rejection_rate = _escape(f"{float(metrics.get('rejection_rate', 0.0)):.2%}")

    page = f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{display_name} · ZERO Network</title>
    <style>
      :root {{
        color-scheme: light;
        --bg: #f7f8f8;
        --ink: #111614;
        --muted: #5d6864;
        --line: #d9dfdc;
        --panel: #ffffff;
        --accent: #0b6b53;
        --accent-soft: #dff2eb;
      }}
      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        background: var(--bg);
        color: var(--ink);
        font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        line-height: 1.5;
      }}
      main {{
        max-width: 920px;
        margin: 0 auto;
        padding: 56px 24px;
      }}
      header {{
        display: grid;
        gap: 12px;
        padding-bottom: 28px;
        border-bottom: 1px solid var(--line);
      }}
      h1 {{
        margin: 0;
        font-size: clamp(2rem, 5vw, 4.25rem);
        line-height: 0.98;
        letter-spacing: 0;
      }}
      .handle {{
        color: var(--muted);
        font-size: 1rem;
      }}
      .summary {{
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 12px;
        margin: 28px 0;
      }}
      .panel {{
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: 8px;
        padding: 18px;
      }}
      .label {{
        color: var(--muted);
        font-size: 0.78rem;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .value {{
        margin-top: 8px;
        font-size: 1.35rem;
        font-weight: 700;
      }}
      .grid {{
        display: grid;
        grid-template-columns: 1.4fr 1fr;
        gap: 18px;
      }}
      h2 {{
        margin: 0 0 14px;
        font-size: 1rem;
      }}
      dl {{
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 12px;
        margin: 0;
      }}
      dt {{
        color: var(--muted);
        font-size: 0.78rem;
      }}
      dd {{
        margin: 4px 0 0;
        font-size: 1.15rem;
        font-weight: 700;
      }}
      ul {{
        list-style: none;
        margin: 0;
        padding: 0;
        display: grid;
        gap: 10px;
      }}
      li {{
        display: flex;
        justify-content: space-between;
        gap: 16px;
        border-bottom: 1px solid var(--line);
        padding-bottom: 10px;
      }}
      li:last-child {{ border-bottom: 0; padding-bottom: 0; }}
      code {{
        display: block;
        overflow-wrap: anywhere;
        border-radius: 8px;
        background: var(--accent-soft);
        color: var(--accent);
        padding: 12px;
        font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
        font-size: 0.84rem;
      }}
      footer {{
        margin-top: 28px;
        color: var(--muted);
        font-size: 0.9rem;
      }}
      @media (max-width: 720px) {{
        main {{ padding: 36px 16px; }}
        .summary, .grid, dl {{ grid-template-columns: 1fr; }}
      }}
    </style>
  </head>
  <body>
    <main>
      <header>
        <h1>{display_name}</h1>
        <div class="handle">@{handle} · {mode} · {status}</div>
      </header>
      <section class="summary" aria-label="Profile summary">
        <div class="panel"><div class="label">Decisions</div><div class="value">{_escape(str(metrics.get("decisions", 0)))}</div></div>
        <div class="panel"><div class="label">Rejection Rate</div><div class="value">{rejection_rate}</div></div>
        <div class="panel"><div class="label">Verification</div><div class="value">{_escape(str(row["verification_score"]))}</div></div>
      </section>
      <section class="grid">
        <div class="panel">
          <h2>Aggregate Behavior</h2>
          <dl>
{metric_items}
          </dl>
        </div>
        <div class="panel">
          <h2>Verification Badges</h2>
          <ul>
{badge_items}
          </ul>
        </div>
      </section>
      <section class="panel" style="margin-top:18px">
        <h2>Proof Hash</h2>
        <code>{proof_hash}</code>
      </section>
      <footer>
        Generated {timestamp}. Public ZERO Network profiles are aggregate proof-of-process surfaces, not financial advice.
      </footer>
    </main>
  </body>
</html>
"""
    assert_public_profile_safe({"html": page})
    return page


def load_public_profiles(path: str | Path) -> tuple[dict[str, Any], ...]:
    profiles = []
    with Path(path).open(encoding="utf-8") as fh:
        for line_number, line in enumerate(fh, start=1):
            stripped = line.strip()
            if not stripped:
                continue
            profile = json.loads(stripped)
            if not isinstance(profile, dict):
                raise ValueError(f"profile log line {line_number} must be a JSON object")
            _public_leaderboard_row(profile)
            profiles.append(profile)
    return tuple(profiles)


def publish_profile(
    profile: dict[str, Any],
    *,
    consent: bool,
    publish_path: str | None,
) -> dict[str, Any]:
    if not consent:
        return {
            "ok": False,
            "published": False,
            "reason": "explicit consent required",
            "profile": profile,
        }
    if not publish_path:
        return {
            "ok": False,
            "published": False,
            "reason": "ZERO_NETWORK_PUBLISH_PATH is not configured",
            "profile": profile,
        }
    assert_public_profile_safe(profile)
    path = Path(publish_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(profile, sort_keys=True, separators=(",", ":")) + "\n")
    return {
        "ok": True,
        "published": True,
        "reason": "published to local ZERO Network proof log",
        "path": str(path),
        "proof_hash": profile["verification"]["proof_hash"],
        "profile": profile,
    }


def privacy_policy() -> dict[str, Any]:
    return {
        "default": "private",
        "publication": "opt-in",
        "included": [
            "aggregate decision counts",
            "aggregate fill and rejection counts",
            "aggregate notional",
            "verification badge status",
            "proof hash",
        ],
        "excluded": [
            "raw decisions",
            "trace ids",
            "idempotency keys",
            "wallet addresses",
            "exchange order ids",
            "private notes",
            "strategy source labels",
            "per-trade symbols",
        ],
    }


def assert_public_profile_safe(payload: dict[str, Any]) -> None:
    body = json.dumps(payload, sort_keys=True).lower()
    forbidden = [
        "trace_id",
        "idempotency_key",
        "wallet_address",
        "private_key",
        "exchange_response",
        "api:/execute",
        "strategy:",
        "0x" + ("1" * 16),
    ]
    for token in forbidden:
        if token in body:
            raise ValueError(f"public profile contains forbidden token: {token}")


def sha256_json(payload: dict[str, Any]) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def _public_leaderboard_row(profile: dict[str, Any]) -> dict[str, Any]:
    if profile.get("schema_version") != PROFILE_SCHEMA_VERSION:
        raise ValueError("public profile schema_version must be zero.network.profile.v1")
    assert_public_profile_safe(profile)

    row = profile.get("leaderboard_row")
    if not isinstance(row, dict):
        raise ValueError("public profile missing leaderboard_row")

    handle = str(profile.get("profile", {}).get("handle", ""))
    proof_hash = str(profile.get("verification", {}).get("proof_hash", ""))
    if row.get("handle") != handle:
        raise ValueError("leaderboard row handle must match profile handle")
    if row.get("proof_hash") != proof_hash:
        raise ValueError("leaderboard row proof_hash must match profile proof_hash")

    return {
        "handle": handle,
        "display_name": str(profile.get("profile", {}).get("display_name") or handle),
        "mode": str(row.get("mode", profile.get("mode", "paper"))),
        "decisions": int(row.get("decisions", 0)),
        "rejection_rate": float(row.get("rejection_rate", 0.0)),
        "open_positions": int(row.get("open_positions", 0)),
        "verification_score": float(row.get("verification_score", 0.0)),
        "proof_hash": proof_hash,
    }


def _metric(label: str, value: Any) -> str:
    return f"""            <div><dt>{_escape(label)}</dt><dd>{_escape(str(value))}</dd></div>"""


def _escape(value: str) -> str:
    return html.escape(value, quote=True)
