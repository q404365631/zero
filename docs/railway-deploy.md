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

## Deploy

```bash
railway link
railway up
```

After deployment:

```bash
curl -fsS "$ZERO_RAILWAY_URL/health"
curl -fsS "$ZERO_RAILWAY_URL/market/quote?symbol=BTC"
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

ZERO Intelligence snapshot and catalog endpoints are also public-safe aggregate
contracts. The snapshot is delayed public intelligence. The catalog describes
the paid hosted API boundary for realtime access, history, cohorts, webhooks,
exports, redistribution, and reliability commitments.

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
