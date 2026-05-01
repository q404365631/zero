# Open-Core Boundary

ZERO is open core.

This public repository contains the self-hosted runtime that engineers can run,
inspect, test, and extend locally. ZERO Cloud remains the commercial product
around hosted operations, managed deployment, private data services, team
workflows, and enterprise controls.

## Open Source

- Local engine runtime
- Paper-trading adapter
- Safety and risk primitives
- Operator CLI
- Local API contracts
- Strategy and data-adapter examples
- Contributor tests and CI
- Documentation needed to understand, build, and modify the runtime

## Commercial

- Hosted control plane
- Managed teams and operator identities
- Fleet deployment and monitoring
- Premium connectors and private datasets
- Model/key gateway
- Enterprise audit exports
- Commercial support and SLAs

## Boundary Rules

- Public examples must run without real funds.
- Public docs must not reference private infrastructure, direct host details, or
  private legal material.
- Public code must not require ZERO Cloud to run paper mode.
- Cloud integrations must be optional and explicit.
- A public contributor must be able to run the default test suite from a clean
  checkout.

## Contribution Policy

Contributions are welcome in the public runtime. Commercial-only features should
start as a design discussion so maintainers can decide whether the feature
belongs in the open runtime, a public extension interface, or ZERO Cloud.
