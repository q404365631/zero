# Live Evidence

`GET /live/evidence` produces a public-safe evidence packet for a supervised
self-custodial live canary.

The packet is `zero.live_evidence.v1`. It does not include raw decisions,
wallet material, exchange credentials, trace tokens, idempotency keys, or
private notes. Instead, it records hash-only artifacts for the packets an
operator must capture before and after any tiny-capital canary:

- `/live/preflight`
- `/live/cockpit`
- `/hl/reconcile`
- `/immune`
- `/live/certification`
- `/audit/export?limit=100`
- `/deployment/claim`
- `/deployment/heartbeat`

The packet includes `evidence_hash` and a `signature` object. By default the
signature status is `unsigned_local`. Set `ZERO_LIVE_EVIDENCE_SIGNING_KEY` to
produce a local HMAC-SHA256 signature without echoing the key material:

```bash
ZERO_LIVE_EVIDENCE_SIGNING_KEY="$(openssl rand -hex 32)" \
ZERO_LIVE_EVIDENCE_SIGNER="local-canary-operator" \
zero-paper-api --journal .zero/decisions.jsonl
```

Then capture:

```bash
curl -fsS 'http://127.0.0.1:8765/live/evidence' | python3 -m json.tool
```

This is still not a substitute for exchange-side records. A valid canary
evidence bundle must also include Hyperliquid order/fill records and show that
`/pause-entries`, `/flatten-all`, and `/kill` were available and captured.
