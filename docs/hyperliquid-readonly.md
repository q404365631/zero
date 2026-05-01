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

Read-only Hyperliquid support is Cycle 2 in the production-readiness plan.

It unlocks:

- real market visibility without custody;
- Railway paper deployments that can prove liveness;
- live-data paper trading in the next cycle;
- safer live execution later because diagnostics and exchange failure handling
  exist before signing exists.
