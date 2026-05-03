# Live Canary Operator

`scripts/live_canary_operator.py` is the maintained one-command workflow for
public-safe live canary evidence.

It wraps:

- `scripts/live_canary_rehearsal.py`
- `scripts/live_canary_exchange_evidence.py`
- `scripts/live_canary_verify.py`

The output is `operator_report.json`, a public-safe report that records command
status, bundle location, verification status, privacy flags, the live canary
policy, and next actions. It does not include raw private keys, raw exchange
exports, raw idempotency keys, or the live-risk confirmation phrase. The
workflow also writes recursive `SHA256SUMS` for the operator report, nested
bundle, and generated evidence files.

The policy object is `zero.live_canary_policy.v1`. It is the public launch
contract for readiness, policy arm/disarm, bounded launch window, evidence,
shadow review, qualification, follow-through, and the next recommended action.
Render it directly from a bundle or operator workflow directory:

```bash
scripts/live_canary_policy.py artifacts/live-canary-operator/<timestamp>
```

## Refusal Proof

Use this on public paper deployments and not-ready live deployments:

```bash
scripts/live_canary_operator.py http://127.0.0.1:8765 --mode refusal
```

The command:

1. captures the live canary rehearsal bundle;
2. proves live execution fails closed when gates are not ready;
3. attaches an empty exchange export because there are no accepted live
   receipts;
4. embeds the canary policy lifecycle;
5. verifies the bundle with exchange-evidence requirements; and
6. writes `operator_report.json`.

Expected result:

```text
zero live canary operator: ok=True bundle=artifacts/live-canary-operator/.../bundle exchange=True report=artifacts/live-canary-operator/.../operator_report.json
```

Verify the workflow directory before sharing it:

```bash
scripts/live_canary_operator_verify.py artifacts/live-canary-operator/<timestamp>
```

The verifier checks `operator_report.json`, recursive `SHA256SUMS`, privacy
flags, common redaction leaks, accepted-live-receipt exchange-evidence rules,
the embedded live canary policy, and the nested canary bundle via
`scripts/live_canary_verify.py`.

## Real Canary Finalization

A real canary is intentionally two-phase. First, run the rehearsal with the
explicit live-risk confirmation:

```bash
scripts/live_canary_rehearsal.py http://127.0.0.1:8765 \
  --mode canary \
  --symbol BTC \
  --side buy \
  --size 0.001 \
  --confirm-live-risk I_UNDERSTAND_THIS_CAN_PLACE_A_REAL_HYPERLIQUID_ORDER
```

Then export the matching Hyperliquid order/fill records from the operator
account and finalize the bundle:

```bash
scripts/live_canary_operator.py \
  --bundle artifacts/live-canary-rehearsal/<timestamp> \
  --mode canary \
  --exchange-export hyperliquid-export.json \
  --require-live-accepted \
  --require-exchange-evidence

scripts/live_canary_operator_verify.py artifacts/live-canary-operator/<timestamp>
```

The final report is publishable only when:

- the bundle contains an accepted live canary receipt;
- exchange evidence is attached;
- every accepted ZERO receipt matches exchange-side order/fill evidence; and
- the embedded canary policy reports `qualified=true` and
  `publishable_canary_evidence=true`; and
- the verifier passes with `--require-live-accepted` and
  `--require-exchange-evidence`.

Refusal-mode evidence can still be useful, but it is not live-trading proof.
The policy marks it as `refusal_evidence_qualified=true` and recommends the
next operator action without claiming accepted live execution.

## Failure Rules

The operator command exits nonzero when:

- rehearsal collection fails;
- an accepted live receipt exists without exchange evidence;
- exchange evidence does not match every accepted receipt;
- the live canary policy is missing or contradicts the report;
- verification fails; or
- canary mode is requested without the exact confirmation phrase.

This is deliberate. A bundle with accepted live risk but no exchange-side proof
must not be treated as public launch evidence.
