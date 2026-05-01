# ZERO Engine

Paper-first trading engine runtime for ZERO.

This package is the public seed of the open-core engine. It intentionally starts with a small, testable safety contract before live exchange adapters are added.

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
`/journal`, `/metrics`, `/audit/export`, `/hl/status`, `/market/quote`,
`/live/preflight`, `/operator/state`, `POST /execute`, `POST /auto/toggle`, and
`POST /operator/events`.

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

Live custody preflight is visible through `/live/preflight`. It is a non-secret
readiness gate for the future Hyperliquid live executor: private keys are never
accepted over HTTP, diagnostics are redacted, and the public runtime still
returns `live_mode=refused` until live execution ships.

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
- No real exchange private key is required.

The private ZERO engine will be ported into this package behind stable public contracts, not copied wholesale.
