# Model Gateway

ZERO's model gateway is the provider-agnostic boundary for advisory
intelligence. It is not an execution authority, and trading must remain safe
when no model provider is configured.

## Contract

```bash
curl -fsS http://127.0.0.1:8765/intelligence/model-gateway | jq .
```

The response is `zero.model_gateway.status.v1`. It reports configured provider
state, capability routing, adapter class, public usage counters, and privacy
guarantees. It does not include prompts, raw model outputs, API key values,
exchange credentials, wallet material, trace IDs, or idempotency keys.

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

## Health And Audit

```bash
curl -fsS http://127.0.0.1:8765/intelligence/model-gateway/health | jq .
curl -fsS http://127.0.0.1:8765/intelligence/model-gateway/audit | jq .
```

`GET /intelligence/model-gateway/health` returns
`zero.model_gateway.health.v1`. The default probe is config-only: it reports
selected provider, configuration gaps, retry budgets, and whether a provider is
ready for an explicit network probe. It does not call a hosted provider.

To run a real provider probe, call:

```bash
curl -fsS 'http://127.0.0.1:8765/intelligence/model-gateway/health?network=true' | jq .
```

The explicit network probe sends a minimal structured-readiness request through
the selected provider and reports only public metadata: provider, status,
attempts, token counts when available, and a public reason. It never returns the
prompt, raw model output, headers, provider request IDs, or secret values.

`GET /intelligence/model-gateway/audit` returns
`zero.model_gateway.audit.v1`. It is the production model-operations bundle:
status, config-only health, usage totals, control assertions, evidence
requirements, and privacy guarantees in one packet.

Pinned fixtures:

- [`contracts/intelligence/model_gateway_health.json`](../contracts/intelligence/model_gateway_health.json)
- [`contracts/intelligence/model_gateway_audit.json`](../contracts/intelligence/model_gateway_audit.json)

## Providers

The open runtime registers these provider families:

- `mock` for deterministic local CI and conformance tests.
- `openai` for hosted Responses API reasoning, chat, embeddings, and structured
  output.
- `anthropic` for hosted reasoning, chat, and structured output.
- `ollama` for local model serving.
- `openrouter` for hosted provider routing.

External providers use the `http_json` adapter behind the same fail-closed
contract as the mock provider. They are optional and operator-configured. The
public runtime reports whether a provider appears configured, but it does not
expose secret values. Network calls stay disabled unless the operator explicitly
enables the model network boundary with `ZERO_MODEL_ALLOW_NETWORK=true`.
OpenAI and OpenRouter requests use JSON Schema response formats; Ollama receives
the same schema through its local `format` field; Anthropic receives the schema
as an explicit return-only-JSON instruction.

## Retry And Cost Policy

Hosted model retries are bounded by `ZERO_MODEL_MAX_ATTEMPTS`, clamped to `1..3`.
`ZERO_MODEL_TIMEOUT_S` is clamped to `1..120`. Retry attempts and timeout
budgets are public in provider status so operators can audit the model boundary
without exposing secrets.

Provider token usage is recorded when the provider returns usage metadata. Dollar
costs are estimated only when the operator supplies per-provider token prices;
the runtime does not ship stale vendor pricing.

## Safety Rules

- Missing provider means `failed_closed`, never fabricated certainty.
- Model output must pass structured JSON validation before use.
- Hosted provider request failures return `failed_closed` after the bounded
  retry budget is exhausted.
- Usage events record provider, model, capability, status, prompt length, output
  length, attempts, token counts, and estimated cost only.
- Model output is advisory-only; live execution remains gated by preflight,
  reconciliation, immune breakers, dead-man heartbeat, and live policy.

## Environment

- `ZERO_MODEL_PROVIDER=mock|openai|anthropic|ollama|openrouter`
- `ZERO_MODEL_NAME`
- `ZERO_MODEL_MOCK_ENABLED=true`
- `ZERO_MODEL_ALLOW_NETWORK=true`
- `ZERO_MODEL_MAX_ATTEMPTS=1`
- `ZERO_MODEL_TIMEOUT_S=30`
- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `OLLAMA_BASE_URL`
- `OPENROUTER_API_KEY`
- `ZERO_OPENAI_BASE_URL` for an OpenAI-compatible override
- `ZERO_ANTHROPIC_BASE_URL` for an Anthropic-compatible override
- `ZERO_OPENROUTER_BASE_URL` for an OpenRouter-compatible override
- `ZERO_MODEL_OPENAI_INPUT_COST_PER_1M_TOKENS_USD`
- `ZERO_MODEL_OPENAI_OUTPUT_COST_PER_1M_TOKENS_USD`
- `ZERO_MODEL_ANTHROPIC_INPUT_COST_PER_1M_TOKENS_USD`
- `ZERO_MODEL_ANTHROPIC_OUTPUT_COST_PER_1M_TOKENS_USD`
- `ZERO_MODEL_OLLAMA_INPUT_COST_PER_1M_TOKENS_USD`
- `ZERO_MODEL_OLLAMA_OUTPUT_COST_PER_1M_TOKENS_USD`
- `ZERO_MODEL_OPENROUTER_INPUT_COST_PER_1M_TOKENS_USD`
- `ZERO_MODEL_OPENROUTER_OUTPUT_COST_PER_1M_TOKENS_USD`

For public CI, use the mock provider. For production, provider keys should be
scoped per operator and managed outside the repository.

For hosted ZERO Intelligence, provider keys must live in the hosted secret
manager or in the operator's own deployment environment. Public status packets
may expose whether a provider is configured, but must never expose key values,
request headers, raw prompts, raw outputs, or provider request identifiers.
