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
- `next_action`: the first concrete action required before live risk can
  increase.

The cockpit is not an execution endpoint. Risk-reducing actions remain separate
commands:

- `/pause-entries`
- `/kill`
- `/flatten-all`

`/resume-entries` is intentionally friction-gated because it can reopen new
risk-increasing entries.

## Launch Rule

Do not treat `ready=true` as permission to scale capital. It means local
controls are coherent enough for an operator-owned tiny-capital canary. Capture
the cockpit packet, `/live/preflight`, `/immune`, `/hl/reconcile`,
`/live/certification`, `/metrics`, and `/audit/export?limit=1000` before and
after the canary.
