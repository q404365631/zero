# Governance

ZERO uses maintainer-led governance.

## Maintainers

Maintainers are responsible for:

- reviewing pull requests
- triaging issues
- protecting safety-critical paths
- publishing releases
- deciding public/private project boundaries

## Decision Policy

Routine changes can be merged by one maintainer after CI passes.

Safety-critical changes require explicit maintainer approval from an owner of the affected area.
Review ownership is declared in `.github/CODEOWNERS` and validated by
`just codeowners-check`.

Large design changes should start as a proposal issue.

## Public vs Commercial Boundary

The public project contains the local runtime, operator terminal, paper mode,
local APIs, safety gates, venue-adapter contracts, examples, proof contracts,
and extension interfaces.

ZERO Intelligence is the commercial product. Its public contracts may be
discussed in this repo, but proprietary hosted implementation details,
commercial datasets, billing systems, customer-specific infrastructure, and
SLA operations are not required for open-source contribution.

## Stewardship Pledge

We want contributors and operators to know which boundaries are stable.

- We will not move existing Apache-2.0 public runtime features behind a
  proprietary paywall.
- We will not add mandatory telemetry to the public runtime.
- We will not make live trading easier than paper trading in public examples.
- We will not publish private operator journals, private deployment state, or
  exchange credentials.
- We will give at least six months of public notice before any public repo
  license change.
- We will keep commercial hosted implementation separate from this public repo.
