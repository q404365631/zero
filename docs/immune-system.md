# Immune System

ZERO's immune system is the runtime layer that decides whether new
risk-increasing actions are allowed. It is separate from strategy logic: a
strategy can propose an order, but the immune system can still block it.

The local API exposes the current breaker state:

```bash
curl -fsS 'http://127.0.0.1:8765/immune' | python3 -m json.tool
```

The CLI exposes the same packet:

```text
/immune
```

The response is `zero.immune.v1` and includes:

- `risk_increasing_allowed`;
- a summary of open and closed breakers;
- one breaker row per protective control;
- evidence fields that explain why the breaker is open or closed.

## Breakers

The first public breaker set covers:

- stale market data;
- Hyperliquid account reconciliation;
- exchange dead-man freshness;
- operator pause;
- operator inactivity;
- kill switch;
- daily loss limit;
- live order velocity;
- recent exchange submit errors;
- max exposure.

Risk-reducing controls remain available when breakers are open. Operators must
still be able to pause, kill, flatten, and reduce risk while the system refuses
new exposure.

## Live Start Gate

`/live/preflight` embeds the immune report and adds an `immune_breakers` check.
Live start is refused until local custody, journal, account reconciliation,
emergency controls, dry-run certification, and all risk-blocking breakers pass.

Default paper deployments usually report open live breakers because live
custody is intentionally absent. That is expected and does not affect paper
execution.
