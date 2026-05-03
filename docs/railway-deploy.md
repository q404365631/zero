# Railway Paper Deployment

Railway is the preferred hosted path for ZERO paper mode. It gives operators a
publicly reachable runtime without introducing ZERO-hosted custody or a private
control plane.

This deployment is still paper-only:

- no private keys;
- no signing;
- no order placement;
- `POST /execute` records simulated fills only;
- `X-Zero-Mode: live` and `POST /live/*` fail closed because no live executor
  is configured;
- live Hyperliquid mids are read-only when enabled.

## What The Repo Provides

- `railway.toml` selects the Dockerfile build, `/health` healthcheck, and
  restart policy.
- `/app/scripts/railway_start.sh` listens on Railway's injected `PORT`.
- The default journal path is `/data/decisions.jsonl`.
- `ZERO_HYPERLIQUID_LIVE_PRICES=true` is the default so paper mode uses live
  read-only Hyperliquid mids.

## Required Railway Setup

1. Create a Railway project from this GitHub repository.
2. Add a persistent volume mounted at `/data`.
3. Confirm the service variables:

```text
ZERO_JOURNAL_PATH=/data/decisions.jsonl
ZERO_HYPERLIQUID_LIVE_PRICES=true
```

Railway injects `PORT`; do not hardcode it. The container binds
`0.0.0.0:${PORT}` and exposes `/health`.

Optional ZERO Intelligence API reference variables:

```text
ZERO_INTELLIGENCE_API_TOKEN=...
ZERO_INTELLIGENCE_API_PLAN=team_fund
ZERO_INTELLIGENCE_API_ACCOUNT_ID=acct_...
ZERO_INTELLIGENCE_WEBHOOK_SIGNING_KEY=...
```

Do not reuse production tokens in public demos. These variables only exercise
the hosted-compatible contract surface on your own Railway service.

## Deploy

```bash
railway link
railway up
```

After deployment:

```bash
curl -fsS "$ZERO_RAILWAY_URL/health"
curl -fsS "$ZERO_RAILWAY_URL/market/quote?symbol=BTC"
scripts/railway_doctor.py "$ZERO_RAILWAY_URL"
scripts/deployment_evidence.sh "$ZERO_RAILWAY_URL"
```

The quote response should show:

```json
{
  "symbol": "BTC",
  "source": "hyperliquid:allMids",
  "live": true
}
```

## Connect The CLI

```bash
zero --api "$ZERO_RAILWAY_URL" doctor
zero --api "$ZERO_RAILWAY_URL" run quote BTC
zero --api "$ZERO_RAILWAY_URL" run status
```

Risk-increasing commands remain locally gated by the CLI. The public Railway
runtime still treats execution as paper simulation.

## Remote Doctor

Use the Railway doctor before sharing a deployment, after every deploy, and
during incident response:

```bash
scripts/railway_doctor.py "$ZERO_RAILWAY_URL"
scripts/railway_doctor.py "$ZERO_RAILWAY_URL" --json
scripts/railway_doctor.py "$ZERO_RAILWAY_URL" \
  --token "$ZERO_INTELLIGENCE_API_TOKEN" \
  --expect-paper
```

The doctor checks `/health`, `/v2/status`, `/metrics`, `/market/quote`,
`/immune`, `/live/preflight`, `/live/cockpit`, public ZERO Network packets,
delayed ZERO Intelligence packets, hosted-compatible `/v1/intelligence/*`
headers, and paid-scope fail-closed behavior. With a token, it also verifies
that the paid history scope accepts the configured bearer token without leaking
the token, trace IDs, private keys, or raw runtime data.

Warnings are allowed for local ephemeral test services. A public Railway demo
should use a mounted `/data` volume so `durable_journal` reports `ok`.

## Deployment Evidence Pack

Before sharing a public demo, promoting a Railway deploy, or closing an
incident, capture a redacted evidence folder:

```bash
scripts/deployment_evidence.sh "$ZERO_RAILWAY_URL"
scripts/deployment_evidence.sh "$ZERO_RAILWAY_URL" \
  --token "$ZERO_INTELLIGENCE_API_TOKEN" \
  --railway-logs \
  --signing-key "$ZERO_DEPLOYMENT_EVIDENCE_SIGNING_KEY" \
  --signer "$ZERO_DEPLOYMENT_OWNER"
```

By default the collector writes to
`artifacts/deployment-evidence/<timestamp>/`. The folder contains:

- `doctor.json`;
- redacted `/health`, `/v2/status`, `/metrics`, `/audit/export`, `/immune`,
  `/live/preflight`, `/live/cockpit`, `/live/certification`,
  `/deployment/claim`, `/deployment/heartbeat`, `/network/profile`,
  `/intelligence/snapshot`, and `/v1/intelligence/snapshots` packets;
- optional `railway_logs.txt` when the Railway CLI is installed, linked, and
  authenticated;
- `manifest.json` with git context, doctor summary, file inventory, and
  redaction policy;
- `SHA256SUMS` for the captured files;
- optional `EVIDENCE_SIGNATURE.json` when an evidence signing key is supplied.

The collector redacts supplied tokens, authorization values, private/signing key
patterns, trace IDs, and smoke idempotency keys. Treat the output as
operator-review evidence, not as a substitute for private journal custody.

Verify the folder before sharing it:

```bash
scripts/deployment_evidence_verify.py artifacts/deployment-evidence/<timestamp>
scripts/deployment_evidence_verify.py artifacts/deployment-evidence/<timestamp> \
  --signing-key "$ZERO_DEPLOYMENT_EVIDENCE_SIGNING_KEY" \
  --require-signature
```

The signature is an operator-owned HMAC-SHA256 promotion record over
`manifest.json` and `SHA256SUMS`. It proves the captured pack was not modified
after local signing; it is not a public identity system or a replacement for a
future external signing service.

For a public identity proof, bind the deployment claim and heartbeat to an
operator public key:

```bash
curl -fsS "$ZERO_RAILWAY_URL/audit/export?limit=1" > audit.json
python3 - <<'PY'
import json

with open("audit.json", encoding="utf-8") as fh:
    audit = json.load(fh)
with open("deployment_claim.json", "w", encoding="utf-8") as fh:
    json.dump(audit["deployment_claim"], fh)
with open("deployment_heartbeat.json", "w", encoding="utf-8") as fh:
    json.dump(audit["deployment_heartbeat"], fh)
PY
scripts/deployment_identity_evidence.py create \
  deployment_claim.json \
  deployment_heartbeat.json \
  --private-key operator-signing-key.pem \
  --public-key operator-signing-public.pem \
  --signer "$ZERO_DEPLOYMENT_OWNER"
scripts/deployment_identity_evidence.py verify \
  artifacts/deployment-identity/<timestamp> \
  --require-signature
```

## Rollback Rehearsal

Before a public demo, keep one evidence pack for the current deployment and one
for the previous known-good deployment. Rehearse the rollback plan without
mutating Railway:

```bash
scripts/deployment_rollback_rehearsal.py \
  artifacts/deployment-evidence/current \
  --previous-bundle artifacts/deployment-evidence/previous \
  --signing-key "$ZERO_DEPLOYMENT_EVIDENCE_SIGNING_KEY" \
  --require-signature
```

The rehearsal writes `rollback_rehearsal.json`, `SHA256SUMS`, and optional
`ROLLBACK_REHEARSAL_SIGNATURE.json`. It verifies both evidence packs, checks
that the public paper service remains `live_mode=refused`, confirms the
rollback target is health-checkable, and records the exact post-rollback proof
steps. The script is intentionally plan-only; it never calls Railway and never
changes a deployment.

## Journal Recovery

The paper journal is append-only JSONL. With the volume mounted at `/data`, a
restart replays prior paper decisions before the API serves traffic. Replayed
state restores decisions, fills, open positions, rejections, and idempotency
keys. Inspect the replay status through:

```bash
curl -fsS "$ZERO_RAILWAY_URL/health"
zero --api "$ZERO_RAILWAY_URL" run status
```

The journal itself remains available through:

```bash
curl -fsS "$ZERO_RAILWAY_URL/journal?limit=50"
```

Audit and runtime counters are available without secrets:

```bash
curl -fsS "$ZERO_RAILWAY_URL/metrics"
curl -fsS "$ZERO_RAILWAY_URL/audit/export?limit=100"
curl -fsS "$ZERO_RAILWAY_URL/network/profile"
curl -fsS "$ZERO_RAILWAY_URL/network/leaderboard"
curl -fsS "$ZERO_RAILWAY_URL/intelligence/snapshot"
curl -fsS "$ZERO_RAILWAY_URL/intelligence/catalog"
curl -fsS "$ZERO_RAILWAY_URL/intelligence/commercial"
curl -fsS "$ZERO_RAILWAY_URL/intelligence/model-gateway"
curl -fsS "$ZERO_RAILWAY_URL/intelligence/model-gateway/health"
curl -fsS "$ZERO_RAILWAY_URL/intelligence/model-gateway/audit"
curl -fsS "$ZERO_RAILWAY_URL/v1/intelligence/snapshots"
curl -fsS "$ZERO_RAILWAY_URL/live/preflight"
curl -fsS "$ZERO_RAILWAY_URL/live/cockpit"
```

Every HTTP response carries `X-Zero-Trace-Id`. Paper decisions created through
`POST /execute` write that trace ID into the journal and audit export.

ZERO Network profile and leaderboard endpoints are public-safe aggregate proof
surfaces. They exclude raw decisions, trace IDs, idempotency keys, wallet
addresses, exchange order IDs, private notes, strategy source labels, and
per-trade symbols. Publishing remains opt-in and local unless you configure a
publish path.

ZERO Intelligence snapshot, catalog, and commercial-contract endpoints are also
public-safe aggregate contracts. The snapshot is delayed public intelligence.
The catalog points to `/intelligence/commercial`, which describes the paid
hosted API boundary for realtime access, history, cohorts, webhooks, exports,
redistribution, usage events, rate limits, and reliability commitments.

The `/v1/intelligence/*` reference endpoints are hosted-compatible:

```bash
curl -fsS "$ZERO_RAILWAY_URL/v1/intelligence/snapshots"
curl -fsS -H "Authorization: Bearer $ZERO_INTELLIGENCE_API_TOKEN" \
  "$ZERO_RAILWAY_URL/v1/intelligence/history?limit=10"
curl -fsS -H "Authorization: Bearer $ZERO_INTELLIGENCE_API_TOKEN" \
  -H "content-type: application/json" \
  -d '{"url":"https://example.com/zero","event_types":["snapshot.accepted"]}' \
  "$ZERO_RAILWAY_URL/v1/intelligence/webhooks"
```

Delayed snapshots work without a token. Realtime, history, cohort, benchmark,
webhook, and export scopes require a bearer token when
`ZERO_INTELLIGENCE_API_TOKEN` is set. Responses include `x-zero-ratelimit-*`
headers. Webhook responses include an HMAC-SHA256 signature fixture but never
return signing key material.

`/live/preflight` is intentionally non-secret. It reports whether a
self-custodial Hyperliquid live mode would be allowed to start, but this Railway
paper deployment should keep `ready=false` and `live_mode=refused`. `POST
/live/kill`, `/live/pause`, and `/live/flatten` should return
`ok=false, reason="live executor not configured"` on public paper services; do
not put private exchange keys into the public paper service.

`/live/certification` is also safe on Railway. It runs a dry-run fake-exchange
harness and should report `mode=dry_run`, `passed=true`, and
`summary.orders_placed_live=0`.

`/live/cockpit` is safe on Railway. It should report `ready=false`,
`risk_increasing_allowed=false`, and a `next_action` that explains which local
live-control prerequisite is missing.

`/immune` is safe on Railway too. Public paper services should normally report
`risk_increasing_allowed=false` because local live custody is absent; that is a
correct refusal state for paper deployments.

If a deployment starts without a volume, the API still runs, but the journal is
ephemeral and will be lost on restart. Do not use an ephemeral journal for
operator demos or public behavior verification.

## Failure Modes

- If `/health` fails, check that the service listens on Railway's `PORT`.
- If `/market/quote` fails in live mode, Hyperliquid public market data is
  unavailable or the requested symbol is not present in `allMids`.
- If journal history disappears after redeploy, the `/data` volume is missing
  or mounted to the wrong path.
- If zero-downtime deploys show brief downtime, check whether the service has an
  attached volume. Railway does not run two active deployments against the same
  mounted volume.

For incident handling, use [incident-runbooks.md](incident-runbooks.md). The
Railway-specific P1 runbook requires `/health`, `/v2/status`, `/metrics`,
`/immune`, `/network/profile`, and `/intelligence/snapshot` to recover before
promotion.
