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

The maintained collector is:

```bash
scripts/live_canary_rehearsal.py http://127.0.0.1:8765 --mode refusal
```

`--mode refusal` is the public-paper default. It captures preflight,
heartbeat, cockpit, certification, reconciliation, a fail-closed live execute
attempt when the engine is not ready, `/live/receipts`, `/live/evidence`,
metrics, audit export, a manifest, and `SHA256SUMS`.

Verify the bundle before sharing it:

```bash
scripts/live_canary_verify.py artifacts/live-canary-rehearsal/<timestamp> \
  --require-mode refusal
```

The verifier checks required packets, `SHA256SUMS`, manifest consistency,
receipt/evidence hashes, HTTP status codes, and common redaction failures.

When the bundle represents a real canary, attach the operator-owned
Hyperliquid order/fill export before sharing evidence:

```bash
scripts/live_canary_exchange_evidence.py \
  artifacts/live-canary-rehearsal/<timestamp> \
  hyperliquid-export.json \
  --require-match

scripts/live_canary_verify.py artifacts/live-canary-rehearsal/<timestamp> \
  --require-mode canary \
  --require-live-accepted \
  --require-exchange-evidence
```

The exchange evidence packet hashes the raw export and venue identifiers,
omits raw wallet/order/client-order data, and matches accepted ZERO receipts by
symbol, side, and quantity.

For the one-command refusal proof used by public paper deployments:

```bash
scripts/live_canary_operator.py http://127.0.0.1:8765 --mode refusal
scripts/live_canary_operator_verify.py artifacts/live-canary-operator/<timestamp>
```

For a real canary, use the operator command after exporting exchange records:

```bash
scripts/live_canary_operator.py \
  --bundle artifacts/live-canary-rehearsal/<timestamp> \
  --mode canary \
  --exchange-export hyperliquid-export.json \
  --require-live-accepted \
  --require-exchange-evidence

scripts/live_canary_operator_verify.py artifacts/live-canary-operator/<timestamp>
```

The operator report exits nonzero if accepted live receipts exist without
matching exchange-side evidence.

For an operator-owned real canary, the command is intentionally explicit:

```bash
scripts/live_canary_rehearsal.py http://127.0.0.1:8765 \
  --mode canary \
  --symbol BTC \
  --side buy \
  --size 0.001 \
  --confirm-live-risk I_UNDERSTAND_THIS_CAN_PLACE_A_REAL_HYPERLIQUID_ORDER
```

Canary mode refuses before order submission unless `/live/preflight`,
`/live/cockpit`, and `/live/certification` agree that risk can increase. After
an accepted canary attempt it captures receipts and evidence after pause,
flatten, and kill controls.

Before any canary:

1. Save `/live/preflight`, `/live/cockpit`, `/live/receipts`, `/hl/reconcile`,
   `/live/certification`, `/health`, `/live/evidence`, and `/metrics`.
2. Confirm the decision journal is durable and record its checksum.
3. Confirm the kill-switch path exists and is locally writable.
4. Set tiny live limits: low max notional, low daily loss, and low order rate.
5. Run `/live-certify` and require all drills to pass.
6. Capture `/live/evidence` and verify its `evidence_hash` and signature
   status before placing any canary order.

During the canary:

1. Submit one tiny risk-increasing order with a unique idempotency key.
2. Immediately run `/pause-entries`.
3. Run `/flatten-all` and verify reduce-only behavior.
4. Run `/kill` and verify open-order cancellation.
5. Export `/audit/export?limit=1000`, `/metrics`, `/live/preflight`,
   `/live/cockpit`, `/live/receipts`, `/live/evidence`, `/hl/reconcile`, and
   exchange-side order/fill records.

Exit gate: exchange state, local live records, the decision journal, and
reconciliation all agree. If they do not, treat it as P0 and follow
[Incident Runbooks](incident-runbooks.md).
