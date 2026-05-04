# Live Evidence

`GET /live/evidence` produces a public-safe evidence packet for a supervised
self-custodial live canary.

The packet is `zero.live_evidence.v1`. It does not include raw decisions,
wallet material, exchange credentials, trace tokens, idempotency keys, or
private notes. Instead, it records hash-only artifacts for the packets an
operator must capture before and after any tiny-capital canary:

- `/live/preflight`
- `/live/cockpit`
- `/live/receipts`
- `/hl/reconcile`
- `/immune`
- `/live/certification`
- `/audit/export?limit=100`
- `/deployment/claim`
- `/deployment/heartbeat`

For historical operator evidence that already exists outside the public repo,
use the separate proof bridge in [Live Trading Evidence](proof/live/README.md).
That packet is `zero.live_trading_evidence.v1`: it summarizes private
Hyperliquid fills, trades, and live decisions with hashed and bucketed fields,
then verifies that raw custody material was not published.

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

`GET /live/receipts` is the local drill packet behind the
`live_execution_receipts` artifact. It includes exact order intents and hashes
for request, operator context, trace token, idempotency token, and venue
acknowledgement. `/live/evidence` only includes the artifact hash.

This is still not a substitute for exchange-side records. A valid canary
evidence bundle must also include Hyperliquid order/fill records and show that
`/pause-entries`, `/flatten-all`, and `/kill` were available and captured.

`GET /live/canary-policy` returns `zero.live_canary_policy.v1`, the policy
packet that decides how the evidence can be used. It separates qualified
refusal proof from publishable live canary proof:

- refusal proof can show fail-closed behavior without claiming live execution;
- accepted live receipts require exchange-side evidence and follow-through
  captures before they are publishable;
- the next recommended operator action is explicit, so a not-ready deployment
  cannot be marketed as live.

Use `scripts/live_canary_exchange_evidence.py` to attach those exchange-side
records without publishing raw venue payloads:

```bash
scripts/live_canary_exchange_evidence.py \
  artifacts/live-canary-rehearsal/<timestamp> \
  hyperliquid-export.json \
  --require-match

scripts/live_canary_verify.py artifacts/live-canary-rehearsal/<timestamp> \
  --require-exchange-evidence
```

The generated `exchange_evidence.json` includes normalized order/fill facts,
hashes of raw records and venue identifiers, a source-file hash, receipt
matches, and checksum updates. It does not include wallet addresses, raw order
IDs, raw client order IDs, or raw venue payloads.

`scripts/live_canary_operator.py` is the higher-level workflow for operators.
It collects or finalizes the bundle, attaches exchange evidence, runs the
verifier, embeds the live canary policy, and writes `operator_report.json` with
public-safe status and next actions. `scripts/live_canary_operator_verify.py`
then verifies the operator report, recursive checksums, redaction posture,
policy consistency, accepted-live exchange-evidence rules, and nested canary
bundle.
