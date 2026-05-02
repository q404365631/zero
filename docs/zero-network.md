# ZERO Network

ZERO Network is the public proof layer for verified autonomous behavior. It is
not a hosted control plane and it does not require operators to send exchange
credentials to ZERO.

## Defaults

- Runtime behavior is private by default.
- Profile publication is opt-in.
- Public profiles contain aggregate behavior only.
- Public leaderboard rows are derived from the same redacted profile packet.
- Deployment identity is represented by a public-safe
  `zero.deployment.claim.v1` claim hash, not by custody or hosted permission.
- Raw decisions, trace IDs, idempotency keys, wallet addresses, exchange order
  IDs, strategy source labels, private notes, and per-trade symbols are excluded.

## Local Endpoints

`GET /network/profile` returns a local public-safe profile:

```json
{
  "schema_version": "zero.network.profile.v1",
  "profile": {
    "handle": "local-operator",
    "publish_enabled": false
  },
  "verification": {
    "proof_hash": "sha256:...",
    "deployment_claim_hash": "sha256:..."
  },
  "metrics": {
    "decisions": 12,
    "fills": 1,
    "rejections": 11,
    "rejection_rate": 0.9167
  }
}
```

`GET /deployment/claim` returns the signature-ready deployment identity packet
that the profile proof binds. Local deployments report
`signature.status = unsigned_local` unless external signing metadata is provided
through deployment environment variables.

`GET /network/leaderboard` returns `zero.network.leaderboard.v1` with ranked
rows derived from the same redacted profile format.

`POST /network/publish` requires explicit consent and a local publish path:

```bash
ZERO_NETWORK_HANDLE=my-handle \
ZERO_NETWORK_PUBLISH_PATH=.zero/network/published.jsonl \
zero-paper-api --journal .zero/decisions.jsonl

curl -fsS \
  -H "content-type: application/json" \
  -d '{"consent":true}' \
  http://127.0.0.1:8765/network/publish
```

The public runtime writes a JSONL proof packet to the configured local path. It
does not upload to a ZERO-hosted service. A future hosted Network ingestion API
can consume the same packet without changing the local privacy contract.

## Public Index Page Builder

The public repository includes a deterministic static index for checked Network
contract pages:

```bash
just network-index-page-example
```

The index links the public-safe profile and leaderboard pages, explains the
opt-in aggregate publication model, and refuses remote or script-style links.
It uses no JavaScript, remote assets, journals, private execution details, or
external links.

See [examples/network-index-page](../examples/network-index-page) and
[contracts/network/index.html](../contracts/network/index.html).

## Public Profile Page Builder

The public repository includes a deterministic static page builder for one
already-redacted `zero.network.profile.v1` packet:

```bash
just network-profile-page-example
```

The builder emits HTML for aggregate behavior, verification badges, and proof
hash only. It escapes profile-provided text and uses no JavaScript, remote
assets, raw journals, symbols, trace IDs, idempotency keys, wallet addresses,
exchange order IDs, strategy labels, or private notes.

See [examples/network-profile-page](../examples/network-profile-page) and
[contracts/network/profile.html](../contracts/network/profile.html).

## Public Leaderboard Builder

The public repository includes a deterministic builder that turns already
redacted `zero.network.profile.v1` JSONL packets into
`zero.network.leaderboard.v1`:

```bash
just network-leaderboard-example
```

The builder:

- accepts public profile packets only;
- rejects malformed rows and mismatched proof hashes;
- ranks deterministically by verification score, decisions, rejection rate, and
  handle;
- emits only public-safe row fields;
- never reads raw journals, symbols, trace IDs, idempotency keys, wallet
  addresses, exchange order IDs, or private notes.

See [examples/network-leaderboard](../examples/network-leaderboard).

## Public Leaderboard Page Builder

The public repository also includes a deterministic static page builder for an
already-redacted `zero.network.leaderboard.v1` payload:

```bash
just network-leaderboard-page-example
```

The page renders rank, handle, display name, mode, aggregate counts,
verification score, and proof hash only. It escapes row-provided text and uses
no JavaScript, remote assets, raw journals, symbols, trace IDs, idempotency
keys, wallet addresses, exchange order IDs, strategy labels, or private notes.

See [examples/network-leaderboard-page](../examples/network-leaderboard-page)
and [contracts/network/leaderboard.html](../contracts/network/leaderboard.html).

## Static Page Smoke Gate

Checked Network pages are covered by a deterministic smoke gate:

```bash
just network-pages-smoke
```

The gate parses `contracts/network/index.html`,
`contracts/network/profile.html`, and `contracts/network/leaderboard.html`.
It verifies each page title and primary heading, requires expected local links,
and fails on JavaScript, event handlers, remote references, missing local link
targets, or raw private runtime tokens.

## Verification Badges

- `paper_verified`: aggregate paper behavior was observed.
- `durable_journal`: behavior is backed by a local durable journal.
- `live_observed`: live execution records exist. This badge does not imply
  custody transfer and must never include exchange credentials.

## Leaderboard Rules

The first open-source leaderboard model is intentionally conservative. It ranks
verified public behavior by aggregate activity and rejection discipline rather
than PnL screenshots. It is a proof-of-process surface, not financial advice.

The leaderboard row includes:

- rank;
- handle;
- display name;
- mode;
- decision count;
- rejection rate;
- open position count;
- verification score;
- proof hash.
- deployment claim hash.

It excludes raw trades and strategy details.
