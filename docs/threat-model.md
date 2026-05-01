# Threat Model

This model covers the public ZERO runtime, CLI, Railway paper deployment,
ZERO Network proof packets, and ZERO Intelligence packet contracts.

## Assets

- Operator exchange credentials and wallet addresses
- Live execution authority
- Decision journal and audit export
- Public profile, leaderboard, and intelligence packets
- Release artifacts, checksums, and attestations
- CLI configuration and local keychain references

## Trust Boundaries

- Local operator machine: trusted for runtime configuration and key storage.
- Public runtime API: trusted only for the operator who controls the process.
- Railway project: operator-owned hosting boundary, not ZERO custody.
- Hyperliquid public info endpoints: untrusted read-only market data source.
- Hyperliquid exchange signing path: live-only, local, and preflight gated.
- GitHub release pipeline: trusted only after checksum and attestation checks.
- ZERO Network and ZERO Intelligence packets: public-safe aggregate data only.

## Threats And Controls

| Threat | Impact | Current Controls | Required Response |
|---|---|---|---|
| Private key committed or logged | Loss of funds | No first-run secrets, redaction tests, local-only key helpers, secret scan | Rotate key, revoke affected API wallet, invalidate release if artifact leaked it |
| Public profile leaks raw trades or trace IDs | Privacy breach | `zero.network.profile.v1` redaction tests and smoke checks | Stop publishing, patch redaction, rotate proof hash, publish incident note |
| Intelligence packet leaks private details | Commercial/data breach | `zero.intelligence.snapshot.v1` aggregate-only tests and smoke checks | Disable export path, patch serializer, rerun privacy gate |
| Paper deployment accidentally enables live execution | Unexpected real orders | Public Railway smoke proves live mode refused, live executor requires explicit env and preflight | Kill service, unset live env, inspect journal and exchange orders |
| Exchange outage or malformed market data | Bad decisions | Read-only quote source labeling, fail-closed missing symbols, operator-visible errors | Pause automation, switch to deterministic paper source, document outage window |
| Duplicate order submission | Over-exposure | Idempotency keys, no retry on live order submit, deterministic client order IDs | Kill live executor, reconcile exchange fills, resume only after audit |
| Dead-man or kill switch failure | Continued exposure | Dead-man heartbeat, pause, kill, reduce-only flatten controls | Trigger kill, manually flatten at venue, open P0 incident |
| Release artifact tampering | Compromised installs | SHA256SUMS plus GitHub artifact attestations | Pull draft release, rotate affected tag, rebuild from clean runner |
| Dependency compromise | Runtime compromise | Dependabot, CodeQL, OpenSSF Scorecard, lockfile review | Pin or remove dependency, cut patched release |
| Contributor bypasses safety gates | Regression | CI, smoke tests, safety review issue template, branch protection | Revert via PR, add regression test, document bypass path |

## Non-Goals

- ZERO does not custody exchange funds.
- ZERO does not require a hosted control plane for local runtime use.
- Public paper deployments are not production trading services.
- Public snapshots are not financial advice and do not expose strategy details.

## Review Triggers

Update this document before merging changes that touch:

- exchange signing or live execution;
- private key handling;
- decision journals, audit export, or recovery;
- public profile, leaderboard, or intelligence serialization;
- release artifact generation, installation, or verification;
- Railway deployment defaults.

## Residual Risk

The public runtime can now demonstrate safety boundaries and operational
responses, but real capital still depends on operator custody hygiene,
exchange-side controls, deployment configuration, and external market behavior.
Live operation must be treated as a local operator decision, not a hosted ZERO
guarantee.
