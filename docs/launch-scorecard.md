# Launch Scorecard

This scorecard keeps the public repo honest before launch. It separates what is
ready for open-source contributors from what is paper-only or intentionally
reserved for ZERO Intelligence.

## Current Score

**100/100**

## Ready

- Paper-first Python engine runtime
- Deterministic paper scenarios and strategy example
- Inspectable paper decision log
- Local paper HTTP API with CLI-compatible endpoints
- Railway deployment template, remote doctor, redacted deployment evidence pack,
  evidence verifier, optional HMAC signature, deployment identity evidence,
  plan-only rollback rehearsal, optional Railway log capture, and smoke test
- Hyperliquid read-only market data path
- Optional self-custodial live-executor boundary with fail-closed paper deploys
- Public-safe ZERO Network profile, leaderboard, local publish, and hosted-compatible ingestion contracts
- Delayed public ZERO Intelligence snapshot, catalog, commercial API boundary,
  hosted-compatible `/v1/intelligence/*` reference API, webhook signature
  fixtures, rate-limit headers, and local export contracts
- Rust operator CLI with doctor, TUI, command tests, and safety invariants
- Redacted CLI quickstart capture for doctor, status, risk inspection, live
  cockpit refusal, receipt summary, canary-policy state, and runtime-parity
  proof
- Public CI for engine, CLI, docs, paper example, paper API smoke, and container smoke
- Release workflow for Python package, CLI binaries, container image artifact, and checksums
- Draft GitHub Release assembly with combined release checksums
- Release verifier and tamper-detection rehearsal
- Release SBOM/provenance bundle with checksummed `SBOM.spdx.json` and
  `PROVENANCE.json`
- Published `v0.1.1` release evidence from a clean GitHub download, including
  checksum verification, release verifier output, executable attestations, and
  Homebrew formula rendering
- Draft GitHub Release rollback rehearsal, Homebrew formula renderer, committed
  public repo tap formula, and formula drift check
- GitHub artifact attestations for release asset provenance
- One-command live canary operator workflow with public-safe report,
  exchange-evidence attachment, recursive checksums, and local verifier
- Live cockpit drill bundle for read-only preflight, immune, reconciliation,
  certification, receipts, evidence, metrics, and audit packets, plus a local
  verifier and tamper rehearsal that replay packet-derived readiness
- Threat model, incident runbooks, distribution policy, and hardening gate
- Dependency and supply-chain policy with vulnerability response rules
- One-line CLI install path with checksum and attestation verification
- Registry-readiness gate for PyPI/Cargo metadata and package-channel guardrails
- Package dry-run gate for Python artifacts and the Rust crate graph
- Shared paper API contract fixtures pinned by Python API tests and Rust client tests
- First-class GitHub product page with category narrative, above-the-fold
  terminal proof artifact, quickstart, safety model, open-core boundary,
  capability boundary, operator proof path, and contributor paths
- First-10-minutes guide and reproducible terminal demo capture for source and
  installed release binaries, including the live cockpit/readiness boundary
- Fresh source-tree rehearsal that copies the publishable checkout into a
  temporary directory, reruns hardening, and smokes the paper API through the
  CLI
- Public contribution, security, governance, support, and issue templates
- First-release notes template, live contributor issue board, and launch issue
  seed with five good-first issues and three help-wanted issues
- Reader-focused release verification guide covering checksums, attestations,
  SBOM/provenance metadata, Homebrew formula drift, and clean-download evidence
- CLI doctor troubleshooting guide for missing tokens, stopped paper API, and
  fail-closed live preflight warnings
- Read-only MCP markdown resources for strategy runner, strategy plugin, and
  market-data adapter contributor docs
- Public boundary audit from the private repo

## Paper-Only

- `POST /execute` in the public engine returns `simulated=true`
- Local market prices are deterministic fixtures unless read-only Hyperliquid
  mids are explicitly enabled
- Public hosted deployments should keep live execution disabled unless the
  operator self-custodially configures local credentials and controls
- Container image is a paper runtime, not a production trading service

## Intentionally Not Shipped

- Hosted ZERO Network profile pages, signed identity verification, and production ingestion persistence
- Production hosted realtime ZERO Intelligence API service
- Production hosted historical intelligence warehouse
- Hosted intelligence ingestion persistence, billing provider integration, and
  signed webhook delivery infrastructure
- Commercial intelligence connectors
- Enterprise support and SLAs

## Remaining To Keep 100

- Keep the public GitHub Actions matrix green after every push
- Keep published release evidence green with `just release-evidence v0.1.1`
- Keep package-registry publication disabled until public name ownership,
  Trusted Publishing, owner lists, and rollback procedure are secured
- Keep the committed Homebrew formula generated from release checksums

## Definition Of 100

The repo is 100/100 when a new engineer can clone it, run one command, inspect a
paper engine through the CLI, pass CI locally, verify release artifacts, and pick
up a scoped contributor issue without private context.
