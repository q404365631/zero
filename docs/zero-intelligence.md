# ZERO Intelligence

ZERO Intelligence is the commercial product built from verified autonomous
behavior.

It is not a hosted deployment product. Operators should be able to run ZERO
locally, through Docker, or on Railway without paying ZERO and without sending
exchange credentials to ZERO. Railway is the preferred hosted deployment path
because it gives operators their own project, secrets, services, volumes,
databases, logs, and billing relationship.

## Public Surfaces

- Open-source ZERO Runtime
- Operator CLI
- Local paper mode
- Self-custodial live mode, once shipped
- Railway and Docker deployment templates
- Public profiles
- Public leaderboards
- Public verification badges
- Public benchmark pages
- Delayed or rate-limited public intelligence snapshots

## Commercial Surfaces

- Realtime intelligence API
- Historical decision and risk datasets
- Advanced filters, cohorts, and benchmark analytics
- Commercial intelligence connectors and enrichment feeds
- Webhooks and streaming feeds
- Higher API limits and bulk exports
- Commercial redistribution rights
- Enterprise support, reliability commitments, and SLAs

## Public Runtime Contract

The open runtime now emits the same safe packets that the commercial data
product should ingest later:

- `GET /intelligence/snapshot` returns `zero.intelligence.snapshot.v1`, a
  delayed public aggregate derived from a verified ZERO Network profile.
  Its source binds both the deployment claim hash and deployment heartbeat hash.
- `GET /intelligence/catalog` returns `zero.intelligence.catalog.v1`, the
  commercial API, billing, scope, dataset, and rate-limit contract.
- `GET /intelligence/commercial` returns `zero.intelligence.commercial.v1`, the
  pinned hosted API boundary for plans, scopes, datasets, usage events,
  webhooks, exports, reliability tiers, and privacy.
- `GET /v1/intelligence/snapshots`, `/history`, `/cohorts`, and `/benchmarks`
  expose the hosted-compatible read API shape. Delayed snapshots are public;
  realtime, history, cohort, and benchmark scopes require a bearer token when
  `ZERO_INTELLIGENCE_API_TOKEN` is configured.
- `POST /v1/intelligence/webhooks` and `/exports` expose the hosted-compatible
  write API shape for signed webhook fixtures and aggregate export jobs.
- `GET /intelligence/model-gateway` returns `zero.model_gateway.status.v1`, the
  provider-agnostic, fail-closed model routing status for advisory intelligence,
  including optional OpenAI, Anthropic, Ollama, and OpenRouter adapter readiness,
  bounded retry policy, usage counters, and public-safe cost-estimate source.
- `POST /intelligence/export` writes an opt-in local JSONL packet when
  `ZERO_INTELLIGENCE_EXPORT_PATH` is configured and the request includes
  `{"consent":true}`.

The public runtime does not upload intelligence packets to ZERO. Hosted
ingestion is a future commercial API surface, not a requirement for local
operation.

## Packaging

- Free: runtime, CLI, public profiles, public leaderboards, delayed snapshots,
  and low API quota.
- Pro Operator: subscription for higher API quota, alerts, webhooks, longer
  history, saved views, and profile verification features.
- Team/Fund: subscription plus usage for team API keys, cohort analytics,
  realtime feeds, exports, and private benchmarks.
- Enterprise: contract pricing for SLOs, support, compliance needs, custom
  retention, and commercial redistribution.

## Hosted API Shape

The paid hosted API should use bearer API keys, explicit scopes, usage events,
and standard rate-limit headers. The checked contract fixture lives at
[contracts/intelligence/commercial.json](../contracts/intelligence/commercial.json).

The public server now includes a reference implementation of the hosted API
boundary under `/v1/intelligence/*`. It is not a production billing service;
it is a contract harness for clients, docs, Railway smoke tests, and
contributor work.

- `x-zero-ratelimit-limit`
- `x-zero-ratelimit-remaining`
- `x-zero-ratelimit-reset`
- `x-zero-ratelimit-policy`

Primary scopes:

- `intelligence:read:delayed`
- `intelligence:read:realtime`
- `intelligence:read:history`
- `intelligence:cohorts`
- `intelligence:exports`
- `intelligence:webhooks`
- `intelligence:redistribute`

Primary datasets:

- `verified_behavior_snapshots`
- `cohort_benchmarks`
- `risk_operations_history`
- `leaderboard_history`

Primary usage events:

- `snapshot.delayed.read`
- `snapshot.realtime.read`
- `history.query`
- `cohort.query`
- `benchmark.query`
- `webhook.delivery`
- `export.created`
- `redistribution.reported`

Reference auth and signing variables:

```text
ZERO_INTELLIGENCE_API_TOKEN=...
ZERO_INTELLIGENCE_API_PLAN=team_fund
ZERO_INTELLIGENCE_API_ACCOUNT_ID=acct_...
ZERO_INTELLIGENCE_WEBHOOK_SIGNING_KEY=...
```

The reference implementation enforces paid scopes when a token is configured,
emits real `x-zero-ratelimit-*` headers, and returns webhook signature fixtures
with:

```text
x-zero-signature-timestamp
x-zero-signature
x-zero-signature-algorithm
```

The signature payload is `timestamp + "." + canonical_json_body` signed with
HMAC-SHA256. The signing key is never returned.

## Data Rules

- Local runtime use is private by default.
- Telemetry is opt-in.
- Public profile publishing is opt-in.
- Public profile packets must use the `zero.network.profile.v1` redaction
  contract.
- Verified live badges require proof without custody.
- Aggregated intelligence must not expose raw private operator secrets,
  exchange credentials, private notes, or non-consented strategy details.
- Model gateway status must not expose prompts, raw model outputs, provider
  secret values, or hosted request metadata.
- Model gateway health probes are config-only by default; explicit network
  probes must return only public provider, attempt, token-count, and status
  metadata.
- Model gateway audit bundles must prove fail-closed controls and evidence
  requirements without including prompts, raw outputs, headers, request IDs, or
  secret values.
- Model gateway costs must come from operator-configured prices or provider
  usage metadata; the public runtime must not bake stale vendor pricing into
  source code.
- Paid intelligence should monetize speed, scale, history, reliability, and
  commercial access, not basic runtime use.
- Core runtime and venue adapters should remain public. Commercial connectors
  should exist for intelligence enrichment, partner integrations, redistribution,
  and enterprise data delivery.

## Flywheel

```text
Open runtime -> verified behavior -> public network proof -> paid intelligence
```

The runtime creates behavior. The network verifies behavior. ZERO Intelligence
turns verified behavior into a commercial API and subscription business.
