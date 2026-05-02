# Live Certification

ZERO live mode is self-custodial and local-only. The public repo ships a
dry-run certification harness so operators and contributors can prove the live
execution safety contract without exchange credentials or live orders.

Run the harness through the local API:

```bash
zero-paper-api --journal .zero/decisions.jsonl
curl -fsS 'http://127.0.0.1:8765/live/certification' | python3 -m json.tool
```

Or from the CLI:

```text
/live-certify
```

The response is `zero.live_certification.v1` and must report:

- `mode=dry_run`;
- `orders_placed_live=0`;
- every drill with `status=pass`;
- `live_start_certified=true`.

## Required Drills

The harness proves these behaviors against a fake exchange:

- heartbeat arms the exchange dead-man switch before risk can increase;
- risk-increasing orders fail closed without a fresh heartbeat;
- duplicate idempotency keys make only one exchange submit attempt;
- exchange submit outages become auditable refused records and do not retry;
- pause blocks new entries;
- reduce-only flatten still works while paused;
- kill cancels open orders and blocks later risk increases;
- rejected exchange dead-man scheduling does not arm the executor;
- order-rate limits block bursts;
- daily-loss limits block new entries.

## Tiny-Capital Canary

A real live canary is not part of the default public CI path. It requires
explicit operator approval, a separate Hyperliquid API wallet, and tiny capital
that the operator is willing to risk.

Before any canary:

1. Save `/live/preflight`, `/hl/reconcile`, `/live/certification`, `/health`,
   and `/metrics`.
2. Confirm the decision journal is durable and record its checksum.
3. Confirm the kill-switch path exists and is locally writable.
4. Set tiny live limits: low max notional, low daily loss, and low order rate.
5. Run `/live-certify` and require all drills to pass.

During the canary:

1. Submit one tiny risk-increasing order with a unique idempotency key.
2. Immediately run `/pause-entries`.
3. Run `/flatten-all` and verify reduce-only behavior.
4. Run `/kill` and verify open-order cancellation.
5. Export `/audit/export?limit=1000`, `/metrics`, `/live/preflight`,
   `/hl/reconcile`, and exchange-side order/fill records.

Exit gate: exchange state, local live records, the decision journal, and
reconciliation all agree. If they do not, treat it as P0 and follow
[Incident Runbooks](incident-runbooks.md).
