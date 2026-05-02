# Model Gateway

ZERO's model gateway is the provider-agnostic boundary for advisory
intelligence. It is not an execution authority, and trading must remain safe
when no model provider is configured.

## Contract

```bash
curl -fsS http://127.0.0.1:8765/intelligence/model-gateway | jq .
```

The response is `zero.model_gateway.status.v1`. It reports configured provider
state, capability routing, public usage counters, and privacy guarantees. It
does not include prompts, raw model outputs, API key values, exchange
credentials, wallet material, trace IDs, or idempotency keys.

Default local behavior is fail-closed:

```json
{
  "schema_version": "zero.model_gateway.status.v1",
  "mode": "fail_closed",
  "default_provider": "none",
  "routing": {
    "structured_output": null
  }
}
```

The pinned fixture lives at
[`contracts/intelligence/model_gateway.json`](../contracts/intelligence/model_gateway.json).

## Providers

The open runtime registers these provider families:

- `mock` for deterministic local CI and conformance tests.
- `openai` for hosted OpenAI-compatible reasoning, chat, embeddings, and
  structured output.
- `anthropic` for hosted reasoning, chat, and structured output.
- `ollama` for local model serving.
- `openrouter` for hosted provider routing.

External providers are optional and operator-configured. The public runtime
reports whether a provider appears configured, but it does not expose secret
values. Network calls stay disabled unless the operator explicitly enables the
model network boundary.

## Safety Rules

- Missing provider means `failed_closed`, never fabricated certainty.
- Model output must pass structured JSON validation before use.
- Usage events record provider, model, capability, status, prompt length, output
  length, and estimated cost only.
- Model output is advisory-only; live execution remains gated by preflight,
  reconciliation, immune breakers, dead-man heartbeat, and live policy.

## Environment

- `ZERO_MODEL_PROVIDER=mock|openai|anthropic|ollama|openrouter`
- `ZERO_MODEL_NAME`
- `ZERO_MODEL_MOCK_ENABLED=true`
- `ZERO_MODEL_ALLOW_NETWORK=true`
- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `OLLAMA_BASE_URL`
- `OPENROUTER_API_KEY`

For public CI, use the mock provider. For production, provider keys should be
scoped per operator and managed outside the repository.
