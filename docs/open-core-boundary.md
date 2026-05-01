# Open Runtime Boundary

ZERO is open runtime plus commercial intelligence.

This public repository contains the self-hosted runtime that engineers can run,
inspect, test, and extend locally. Deployment should be Railway-first and
self-custodial: operators own their Railway project, environment variables,
exchange credentials, and runtime state.

ZERO Intelligence is the commercial product. It sells advantaged access to
aggregated, verified autonomous behavior through APIs, subscriptions, datasets,
webhooks, benchmarks, and enterprise support.

## Open Source

- Local engine runtime
- Paper-trading adapter
- Safety and risk primitives
- Operator CLI
- Local API contracts
- Strategy and data-adapter examples
- Railway and Docker deployment templates
- Public profile, leaderboard, and verification contracts
- Contributor tests and CI
- Documentation needed to understand, build, and modify the runtime

## Commercial

- Realtime ZERO Intelligence API
- Historical intelligence datasets
- Advanced filters, cohorts, and benchmarks
- Commercial intelligence connectors and enrichment feeds
- Higher rate limits, webhooks, and bulk exports
- Commercial redistribution rights
- Enterprise support, reliability commitments, and SLAs

## Boundary Rules

- Public examples must run without real funds.
- Public docs must not reference private infrastructure, direct host details, or
  private legal material.
- Public code must not require a ZERO-hosted control plane to run paper mode.
- Railway deployment must be optional, reproducible, and self-custodial.
- Runtime telemetry and profile publication must be opt-in.
- Public profiles and leaderboards are part of the public product surface.
- Core runtime and venue adapters should be public; commercial connectors should
  enrich, distribute, or integrate ZERO Intelligence rather than gate basic
  runtime operation.
- Paid features must be based on speed, scale, history, reliability, support, or
  commercial intelligence access, not on basic runtime use.
- A public contributor must be able to run the default test suite from a clean
  checkout.

## Contribution Policy

Contributions are welcome in the public runtime. Commercial-only features should
start as a design discussion so maintainers can decide whether the feature
belongs in the open runtime, a public extension interface, the public network,
or ZERO Intelligence.
