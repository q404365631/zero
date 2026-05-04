# Deployment Identity

ZERO deployment identity is a public-safe claim plus heartbeat protocol that
says which runtime produced an evidence surface and whether its live liveness
state was fresh when the packet was generated. It is not custody,
authentication, or hosted control-plane permission.

## Endpoint

```bash
curl -fsS http://127.0.0.1:8765/deployment/claim | jq .
curl -fsS http://127.0.0.1:8765/deployment/heartbeat | jq .
```

The response is `zero.deployment.claim.v1`:

```json
{
  "schema_version": "zero.deployment.claim.v1",
  "deployment": {
    "deployment_id": "local-paper",
    "kind": "local",
    "environment": "paper",
    "owner": "local-operator",
    "version": "0.1.2"
  },
  "operator": {
    "handle": "local-operator",
    "role": "owner",
    "scope": "local-private",
    "source": "runtime-default"
  },
  "claim_hash": "sha256:...",
  "signature": {
    "status": "unsigned_local",
    "signed_claim_hash": "sha256:..."
  }
}
```

The packet includes aggregate evidence counts, market-source mode, journal
durability, and live-executor configuration. It excludes raw decisions, symbols,
trace IDs, idempotency keys, wallet material, exchange credentials, and private
notes.

`GET /deployment/heartbeat` returns `zero.deployment.heartbeat.v1`. It binds to
the claim through `deployment_claim_hash`, reports only public-safe liveness
fields, and exposes `heartbeat_hash` plus `signature.signed_heartbeat_hash`.
In paper-only mode the liveness status is `paper_only`; with a live executor it
reports `fresh` or `expired` from the dead-man heartbeat state.

## Network Binding

`GET /network/profile` embeds the deployment claim and binds
`verification.deployment_claim_hash` into the profile proof hash. It also embeds
the deployment heartbeat and binds `verification.deployment_heartbeat_hash`.
Leaderboard rows carry both hashes. `GET /intelligence/snapshot` includes the
hashes in `source.deployment_claim_hash` and
`source.deployment_heartbeat_hash`.

Pinned fixtures live at
[`contracts/deployment/claim.json`](../contracts/deployment/claim.json) and
[`contracts/deployment/heartbeat.json`](../contracts/deployment/heartbeat.json).

This lets a future hosted Network or Intelligence API verify that a profile,
leaderboard row, delayed snapshot, and audit export all came from the same local
runtime claim without making paper mode depend on hosted infrastructure.

## Environment

Set these variables when the deployment boundary has a durable identity:

- `ZERO_DEPLOYMENT_ID`
- `ZERO_DEPLOYMENT_KIND`
- `ZERO_DEPLOYMENT_ENVIRONMENT`
- `ZERO_DEPLOYMENT_OWNER`
- `ZERO_DEPLOYMENT_VERSION`
- `ZERO_DEPLOYMENT_PUBLIC_KEY`
- `ZERO_DEPLOYMENT_SIGNATURE`
- `ZERO_DEPLOYMENT_SIGNER`
- `ZERO_DEPLOYMENT_HEARTBEAT_PUBLIC_KEY`
- `ZERO_DEPLOYMENT_HEARTBEAT_SIGNATURE`
- `ZERO_DEPLOYMENT_HEARTBEAT_SIGNER`

When `ZERO_DEPLOYMENT_PUBLIC_KEY` and `ZERO_DEPLOYMENT_SIGNATURE` are present,
the packet reports `signature.status = signed_external`. The open runtime does
not generate or manage signing keys yet; key custody belongs to the deployment
boundary until a dedicated signing design is added.

Heartbeat signing can use distinct heartbeat variables so a deployment boundary
can rotate liveness attestations separately from longer-lived claim metadata.

## Identity Evidence Bundle

Operators can bind the claim and heartbeat into a public-safe evidence bundle
without exposing private key material:

```bash
curl -fsS "$ZERO_RAILWAY_URL/audit/export?limit=1" > audit.json
python3 - <<'PY'
import json

with open("audit.json", encoding="utf-8") as fh:
    audit = json.load(fh)
with open("deployment_claim.json", "w", encoding="utf-8") as fh:
    json.dump(audit["deployment_claim"], fh)
with open("deployment_heartbeat.json", "w", encoding="utf-8") as fh:
    json.dump(audit["deployment_heartbeat"], fh)
PY

scripts/deployment_identity_evidence.py create \
  deployment_claim.json \
  deployment_heartbeat.json \
  --private-key operator-signing-key.pem \
  --public-key operator-signing-public.pem \
  --signer "$ZERO_DEPLOYMENT_OWNER"

scripts/deployment_identity_evidence.py verify \
  artifacts/deployment-identity/<timestamp> \
  --require-signature
```

The bundle contains:

- `deployment_claim.json`;
- `deployment_heartbeat.json`;
- `identity_bundle.json` with schema `zero.deployment_identity_evidence.v1`;
- optional `IDENTITY_SIGNATURE.json`;
- `SHA256SUMS`.

`IDENTITY_SIGNATURE.json` signs a canonical payload containing the claim hash,
heartbeat hash, packet checksums, and public-key hash using `openssl dgst
-sha256`. The verifier recomputes the claim and heartbeat hashes, checks the
heartbeat-to-claim binding, enforces public-safe privacy flags, verifies
checksums, and validates the OpenSSL signature against the included public key.
The private key is never written into the evidence bundle.
