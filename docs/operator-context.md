# Operator Context

ZERO records who asked for live-affecting actions without treating identity as
custody. Authentication still belongs to the deployment boundary and bearer
token. Operator context is an audit envelope that makes team use, agentic
contribution, and incident review legible.

The local runtime resolves context in this order:

1. `X-Zero-Operator-*` request headers;
2. `ZERO_OPERATOR_*` environment variables;
3. the local default `local-operator`.

Supported headers:

- `X-Zero-Operator-Id`
- `X-Zero-Operator-Handle`
- `X-Zero-Operator-Role`
- `X-Zero-Operator-Scope`

Supported environment variables:

- `ZERO_OPERATOR_ID`
- `ZERO_OPERATOR_HANDLE`
- `ZERO_OPERATOR_ROLE`
- `ZERO_OPERATOR_SCOPE`

Inspect the active context:

```bash
curl -fsS http://127.0.0.1:8765/operator/context | jq .
```

`zero` CLI requests attach the handle from `~/.zero/config.toml` when present.
The engine includes the resolved `zero.operator_context.v1` packet in:

- `/operator/context`
- `/deployment/claim` as a public-safe handle/role/scope/source subset
- `/live/cockpit`
- `/audit/export`
- live control responses for `/live/heartbeat`, `/live/pause`,
  `/live/resume`, `/live/kill`, and `/live/flatten`
- accepted/refused live execution records

This is the foundation for multi-operator teams. It is intentionally not a
permission system yet: risk-increasing commands remain protected by CLI
friction, preflight, immune breakers, reconciliation, and live execution policy.

Local mutable state is partitioned separately from this HTTP audit envelope.
See [Operator Isolation](operator-isolation.md) for the filesystem and keychain
model.
