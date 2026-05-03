# Live Canary Operator

`scripts/live_canary_operator.py` is the maintained one-command workflow for
public-safe live canary evidence.

It wraps:

- `scripts/live_canary_rehearsal.py`
- `scripts/live_canary_exchange_evidence.py`
- `scripts/live_canary_verify.py`

The output is `operator_report.json`, a public-safe report that records command
status, bundle location, verification status, privacy flags, and next actions.
It does not include raw private keys, raw exchange exports, raw idempotency
keys, or the live-risk confirmation phrase.

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
4. verifies the bundle with exchange-evidence requirements; and
5. writes `operator_report.json`.

Expected result:

```text
zero live canary operator: ok=True bundle=artifacts/live-canary-operator/.../bundle exchange=True report=artifacts/live-canary-operator/.../operator_report.json
```

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
```

The final report is publishable only when:

- the bundle contains an accepted live canary receipt;
- exchange evidence is attached;
- every accepted ZERO receipt matches exchange-side order/fill evidence; and
- the verifier passes with `--require-live-accepted` and
  `--require-exchange-evidence`.

## Failure Rules

The operator command exits nonzero when:

- rehearsal collection fails;
- an accepted live receipt exists without exchange evidence;
- exchange evidence does not match every accepted receipt;
- verification fails; or
- canary mode is requested without the exact confirmation phrase.

This is deliberate. A bundle with accepted live risk but no exchange-side proof
must not be treated as public launch evidence.
