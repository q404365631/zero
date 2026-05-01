# Launch Scorecard

This scorecard keeps the public repo honest before launch. It separates what is
ready for open-source contributors from what is paper-only or intentionally
reserved for ZERO Cloud.

## Current Score

**90/100**

## Ready

- Paper-first Python engine runtime
- Deterministic paper scenarios and strategy example
- Inspectable paper decision log
- Local paper HTTP API with CLI-compatible endpoints
- Rust operator CLI with doctor, TUI, command tests, and safety invariants
- Public CI for engine, CLI, docs, paper example, paper API smoke, and container smoke
- Release workflow for Python package, CLI binaries, container image artifact, and checksums
- Public contribution, security, governance, support, and issue templates
- Public boundary audit from the private repo

## Paper-Only

- `POST /execute` in the public engine returns `simulated=true`
- Local market prices are deterministic fixtures, not exchange data
- The public API is for local development and CLI inspection
- Container image is a paper runtime, not a production trading service

## Intentionally Not Shipped

- Live exchange execution
- Hosted team control plane
- Managed deployments and fleet monitoring
- Model/key gateway
- Premium connectors
- Enterprise audit exports and SLAs

## Remaining To Reach 100

- Publish repository and confirm all GitHub Actions pass on GitHub-hosted runners
- Add signed GitHub Releases after ownership and token permissions are finalized
- Add Homebrew tap or one-line binary install path
- Add contributor issue set from `docs/backlog.md`
- Add release artifact checksum verification to the first release notes
- Add screenshots or terminal captures for the CLI quickstart
- Add API compatibility tests that pin Rust client fixtures against the Python paper API

## Definition Of 100

The repo is 100/100 when a new engineer can clone it, run one command, inspect a
paper engine through the CLI, pass CI locally, verify release artifacts, and pick
up a scoped contributor issue without private context.
