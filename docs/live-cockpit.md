# Live Cockpit

The live cockpit is the read-only operator view for self-custodial live
readiness. It exists so an operator can diagnose live state from the terminal
without stitching together raw HTTP calls.

```bash
zero --api http://127.0.0.1:8765 run live-cockpit
curl -fsS 'http://127.0.0.1:8765/live/cockpit' | python3 -m json.tool
```

The response is `zero.live_cockpit.v1` and includes:

- `preflight`: failed custody, journal, reconciliation, risk, and emergency
  checks from `/live/preflight`;
- `immune`: open risk-blocking breakers from `/immune`;
- `reconciliation`: local runtime versus Hyperliquid account status;
- `certification`: dry-run live execution drill status;
- `heartbeat`: dead-man configuration and expiry state;
- `live_records`: recent live submit/refusal/exchange-error evidence;
- `operator_context`: the resolved operator audit identity;
- `operator_actions.recent`: recent live control attempts by operator context;
- `next_action`: the first concrete action required before live risk can
  increase.

The cockpit is not an execution endpoint. Risk-reducing actions remain separate
commands:

- `/pause-entries`
- `/kill`
- `/flatten-all`

`/resume-entries` is intentionally friction-gated because it can reopen new
risk-increasing entries.

## Drill Bundle

For a repeatable operator drill, collect the full read-only cockpit stack:

```bash
scripts/live_cockpit_drill.py http://127.0.0.1:8765
```

The script writes `artifacts/live-cockpit-drill/<timestamp>/manifest.json`,
the raw redacted packets, and `SHA256SUMS`. It collects `/health`,
`/v2/status`, `/live/preflight`, `/live/cockpit`, `/immune`, `/hl/reconcile`,
`/live/certification`, `/live/receipts`, `/live/evidence`, `/metrics`, and
`/audit/export?limit=100`.

Verify a captured bundle before sharing it or treating it as launch evidence:

```bash
scripts/live_cockpit_drill_verify.py artifacts/live-cockpit-drill/<timestamp>
```

In public paper mode the drill fails unless live readiness is fail-closed:
`ready=false`, `live_mode=refused`, and `risk_increasing_allowed=false`. It
also checks schema versions, risk-reducing actions, dry-run certification,
checksums, and common redaction leaks. The verifier recomputes `SHA256SUMS`,
checks the packet inventory, replays the manifest summary from packet payloads,
and enforces the same fail-closed and redaction rules.

## Launch Rule

Do not treat `ready=true` as permission to scale capital. It means local
controls are coherent enough for an operator-owned tiny-capital canary. Capture
the cockpit packet, `/live/preflight`, `/immune`, `/hl/reconcile`,
`/live/certification`, `/metrics`, and `/audit/export?limit=1000` before and
after the canary.
