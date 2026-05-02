# ZERO Engine

Paper-first runtime for ZERO self-custodial onchain operations.

This package is the public seed of the open-core engine. It starts with a
small, testable safety contract and now includes an optional, local-only
Hyperliquid live executor behind explicit custody and kill-switch gates.

## Install

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

## Demo

```bash
zero-paper-demo
```

## Local API

```bash
zero-paper-api
```

The local paper API listens on `http://127.0.0.1:8765` by default and
exposes the paper-mode subset of the engine contract used by the Rust CLI:
`/`, `/health`, `/v2/status`, `/positions`, `/risk`, `/brief`,
`/regime`, `/evaluate/{coin}`, `/pulse`, `/approaching`, `/rejections`,
`/journal`, `/metrics`, `/audit/export`, `/hl/status`, `/hl/account`,
`/hl/reconcile`, `/market/quote`,
`/network/profile`, `/network/leaderboard`, `/intelligence/snapshot`,
`/intelligence/catalog`, `/live/preflight`, `/live/certification`,
`/operator/state`, `POST /execute`, `POST /auto/toggle`, `POST /operator/events`,
`POST /network/publish`, `POST /intelligence/export`, and the live-control
endpoints under `POST /live/*`.

For a replayable local audit log, pass a JSONL journal path:

```bash
zero-paper-api --journal .zero/decisions.jsonl
```

On restart, the API replays that journal before serving traffic. Recovered
state includes decisions, simulated fills, open positions, rejections, and
idempotency keys; `/health` and `/v2/status` expose the recovery summary.

Every HTTP response includes `X-Zero-Trace-Id`. Paper decisions created through
HTTP execution write that trace into the journal. Operators can inspect runtime
counters through `/metrics` and export a structured audit packet through
`/audit/export?limit=100`.

ZERO Network profile and leaderboard contracts are exposed through
`/network/profile` and `/network/leaderboard`. They are aggregate and redacted
by default. To write an opt-in local publish packet, set
`ZERO_NETWORK_PUBLISH_PATH` and call `POST /network/publish` with
`{"consent":true}`.

ZERO Intelligence contracts are exposed through `/intelligence/snapshot` and
`/intelligence/catalog`. Delayed public snapshots are aggregate and redacted.
To write an opt-in local intelligence packet, set
`ZERO_INTELLIGENCE_EXPORT_PATH` and call `POST /intelligence/export` with
`{"consent":true}`.

Live custody preflight is visible through `/live/preflight`. It is a non-secret
readiness gate for the Hyperliquid live executor: private keys are never
accepted over HTTP, diagnostics are redacted, account reconciliation is checked,
and public paper deployments return `live_mode=refused` unless local live
credentials and controls are configured.

Live certification is visible through `/live/certification`. It runs dry-run
fake-exchange drills for heartbeat, idempotency, exchange outages, pause,
reduce-only flatten, kill, rate limits, and loss limits without placing live
orders.

Live execution is optional and self-custodial:

```bash
pip install -e ".[live]"
ZERO_LIVE_EXECUTION_ENABLED=true \
ZERO_HYPERLIQUID_WALLET_ADDRESS=0x... \
ZERO_HYPERLIQUID_API_PRIVATE_KEY=0x... \
zero-paper-api --journal .zero/decisions.jsonl --hyperliquid-live-prices
```

With `X-Zero-Mode: live`, `POST /execute` routes through the live executor.
Without a configured executor it fails closed with `accepted=false` and
`reason="live executor not configured"`. `/live/heartbeat`, `/live/pause`,
`/live/resume`, `/live/kill`, and `/live/flatten` provide the operator controls
the CLI calls.

To enable read-only Hyperliquid market metadata and mids:

```bash
zero-paper-api --hyperliquid
curl -fsS 'http://127.0.0.1:8765/hl/status?symbol=BTC'
```

This does not require exchange credentials and cannot place orders.

To route paper quotes and paper fills through live Hyperliquid mids:

```bash
zero-paper-api --journal .zero/decisions.jsonl --hyperliquid-live-prices
curl -fsS 'http://127.0.0.1:8765/market/quote?symbol=BTC'
```

This still cannot place exchange orders. If live market data is unavailable or a
symbol is missing from Hyperliquid `allMids`, paper execution fails closed
instead of silently using fixture prices.

Paper execution example:

```bash
curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"readme-smoke"}' \
  http://127.0.0.1:8765/execute
```

The response is expected to include `"simulated": true`. Public `/execute`
orders are paper fills and still pass through the same safety evaluation path as
the Python `PaperEngine`.

## Test

```bash
pytest
ruff check .
```

## Safety Contract

- Paper mode is the first-run path.
- Risk-increasing orders are evaluated before fill.
- Reduce-only orders bypass risk-increasing friction.
- Rejections are recorded explicitly.
- No real exchange private key is required for paper mode or contribution work.
- Live mode is local opt-in and must fail closed when preflight is not ready.

The private ZERO engine will be ported into this package behind stable public contracts, not copied wholesale.
