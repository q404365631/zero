# ZERO Network

ZERO Network is the public proof layer for verified autonomous behavior. It is
not a hosted control plane and it does not require operators to send exchange
credentials to ZERO.

## Defaults

- Runtime behavior is private by default.
- Profile publication is opt-in.
- Public profiles contain aggregate behavior only.
- Public leaderboard rows are derived from the same redacted profile packet.
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
    "proof_hash": "sha256:..."
  },
  "metrics": {
    "decisions": 12,
    "fills": 1,
    "rejections": 11,
    "rejection_rate": 0.9167
  }
}
```

`GET /network/leaderboard` returns `zero.network.leaderboard.v1` with rows
derived from the local profile.

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

- handle;
- mode;
- decision count;
- rejection rate;
- open position count;
- verification score;
- proof hash.

It excludes raw trades and strategy details.
