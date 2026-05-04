# ZERO Network Freshness

ZERO Network publishes public-safe proof of process, not custody, PnL, or live
trading authority. A profile packet can stay cryptographically valid while its
operator state becomes old. Hosted Network pages must separate proof validity
from freshness.

## Core Rule

- Proof hashes remain historical audit evidence.
- Freshness badges expire when the packet is no longer recent.
- Stale packets can remain visible as archived proof.
- Expired packets should not receive active leaderboard freshness treatment.

This prevents an old but valid packet from implying current operator health,
current runtime liveness, or current risk discipline.

## Public-Safe Timestamps

Freshness should be derived only from public-safe timestamps already present in
or bound to redacted packets:

- `generated_at` from `zero.network.profile.v1`;
- deployment heartbeat timestamp from `zero.deployment.heartbeat.v1`;
- hosted ingestion timestamp assigned by the Network service.

Hosted pages must not request exchange credentials, wallet signatures,
exchange order IDs, raw journals, trace IDs, idempotency keys, per-trade
symbols, strategy labels, or private notes to calculate freshness.

## Freshness Windows

Use conservative defaults until real hosted traffic gives enough evidence to
adjust them.

| Packet type | Fresh | Stale | Expired |
| --- | --- | --- | --- |
| Paper profile | 24 hours | 7 days | 30 days |
| Live-observed profile | 15 minutes | 2 hours | 24 hours |
| Deployment heartbeat | 10 minutes | 30 minutes | 2 hours |

Paper profiles prove process and reproducibility, so they can remain fresh for
longer. Live-observed profiles imply more current operational liveness, so they
must expire faster. Deployment heartbeats are liveness evidence and should use
the strictest window.

## Badge Policy

Hosted pages may show:

- `proof_valid`: profile hash, leaderboard row, and optional deployment
  identity chain verify;
- `fresh`: packet is inside the fresh window;
- `stale`: packet is outside the fresh window but inside the stale window;
- `expired`: packet is outside the stale window.

Freshness badges must be removed or downgraded when the newest bound timestamp
crosses its window. `proof_valid` may remain visible if the packet still
verifies. `live_observed` must never imply that ZERO controls funds or that the
operator is currently live.

## Leaderboard Policy

Leaderboard rank remains proof-of-process, not PnL.

Hosted leaderboards should:

- rank active rows only when profile proof is valid and freshness is not
  expired;
- mark stale rows clearly in the active table or move them below fresh rows;
- move expired rows to historical views unless explicitly requested;
- keep ranking based on aggregate verification score, decisions, rejection
  discipline, and deterministic tie-breaks;
- never rank by unverified screenshots, claimed PnL, raw trade logs, or private
  strategy details.

If a row is stale or expired, the page should explain that historical proof
still verifies but current operator liveness is not being asserted.

## Ingestion Behavior

Hosted ingestion should preserve existing refusal rules for missing consent,
proof mismatches, inconsistent aggregate metrics, duplicate accepted handles,
and duplicate accepted proof hashes. Freshness is an additional display and
eligibility layer; it should not rewrite historical proof.

Recommended packet treatment:

- accept and archive valid packets even when stale;
- withhold `fresh` treatment when timestamps exceed the relevant window;
- reject or quarantine packets with impossible future timestamps;
- refuse active leaderboard eligibility for expired packets;
- include freshness status and evaluated timestamp in public-safe ingestion
  results.

## Operator Guidance

Operators who want an active public profile should republish a fresh redacted
packet from their local runtime. Republishing must remain opt-in and must not
require hosted custody, exchange credential upload, or private journal upload.

Freshness is an honesty signal. It tells viewers when the public proof was last
observed without turning ZERO Network into a custody service or a performance
claim.
