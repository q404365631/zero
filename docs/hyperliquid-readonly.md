# Hyperliquid Read-Only Runtime

ZERO is Hyperliquid-first, but the public runtime must become real without
introducing custody risk too early.

The first production wedge is read-only Hyperliquid access:

- public market mids through the Hyperliquid info endpoint;
- optional account reads for an operator-supplied master or sub-account address;
- no private keys;
- no signing;
- no order placement;
- no live execution path.

## Local Use

Start the paper API with Hyperliquid reads enabled:

```bash
zero-paper-api --hyperliquid
```

Inspect public mids:

```bash
curl -fsS 'http://127.0.0.1:8765/hl/status?symbol=BTC'
```

The response includes:

- `enabled=true`
- `exchange=hyperliquid`
- `secrets_required=false`
- `mids`

Without `--hyperliquid`, `/hl/status` returns `enabled=false` and the paper
runtime remains fully deterministic and offline.

To make paper mode use live Hyperliquid mids for quotes, evaluation, positions,
and paper fills:

```bash
zero-paper-api --journal .zero/decisions.jsonl --hyperliquid-live-prices
curl -fsS 'http://127.0.0.1:8765/market/quote?symbol=BTC'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"live-paper-1"}' \
  http://127.0.0.1:8765/execute
```

`--hyperliquid-live-prices` implies the read-only Hyperliquid adapter. It still
does not require credentials, cannot sign payloads, and cannot place exchange
orders. If Hyperliquid market data is unavailable or a requested symbol is not
present in `allMids`, paper execution fails closed instead of falling back to
fixture prices.

## Live Custody Preflight

The public paper runtime also exposes:

```bash
curl -fsS 'http://127.0.0.1:8765/live/preflight'
```

This is a non-secret readiness gate for optional local live mode. It never
accepts a private key over HTTP. Local deployments can set wallet and key
material through local process configuration or the CLI keychain helpers, and
the endpoint reports only redacted diagnostics. Public paper deployments should
return `ready=false` and `live_mode=refused`.

## Safety Boundary

The read-only adapter may call Hyperliquid `info` methods such as `allMids` and
`clearinghouseState`.

It must not:

- call exchange/order endpoints;
- accept private keys;
- sign payloads;
- infer custody from an agent wallet address;
- make paper examples depend on external network access.

Account reads must use the actual master or sub-account address. Agent wallet
addresses can return empty account data and should not be used for
operator-state verification.

## Production Path

Read-only Hyperliquid support is Cycle 2 in the production-readiness plan. The
optional live executor is documented in [API Contract](api.md), and it must stay
outside this read-only adapter boundary.
Live custody preflight is Cycle 7.

It unlocks:

- real market visibility without custody;
- Railway paper deployments that can prove liveness;
- live-data paper trading through the same quote path as later live execution;
- safer live execution later because diagnostics and exchange failure handling
  exist before signing exists.
