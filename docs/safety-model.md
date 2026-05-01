# Safety Model

ZERO treats trading execution as safety-critical software.

## Defaults

- Paper mode is the default demo path.
- No real private key is required for contributor setup.
- Risk gates fail closed when required data is missing.
- Risk-reducing actions must not be delayed by confirmation friction.
- Operator-visible state must include source and freshness where possible.

## Safety-Critical Areas

- position sizing
- exchange execution
- kill switches
- stop-loss and liquidation-distance logic
- private key handling
- operator command friction
- paper/live isolation

Changes in these areas need focused regression tests and maintainer review.

