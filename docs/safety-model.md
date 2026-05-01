# Safety Model

ZERO treats trading execution as safety-critical software.

## Defaults

- Paper mode is the default demo path.
- No real private key is required for contributor setup.
- Risk gates fail closed when required data is missing.
- Risk-reducing actions must not be delayed by confirmation friction.
- Operator-visible state must include source and freshness where possible.
- Every paper order decision records source, timestamp, allowed/rejected state,
  and reason.

## Safety-Critical Areas

- position sizing
- exchange execution
- kill switches
- stop-loss and liquidation-distance logic
- private key handling
- operator command friction
- paper/live isolation

Changes in these areas need focused regression tests and maintainer review.

## Incident And Threat Review

Maintainers must review [threat-model.md](threat-model.md) and
[incident-runbooks.md](incident-runbooks.md) before releasing changes that touch
live execution, custody, public packet serialization, release artifacts, or
Railway deployment defaults.

Public packet serializers must stay aggregate-only. Any leak of trace IDs,
idempotency keys, per-trade symbols, private notes, wallet material, exchange
responses, or strategy source labels is a P1 privacy incident.
