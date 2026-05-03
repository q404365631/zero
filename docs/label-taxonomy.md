# Label Taxonomy

ZERO uses labels as routing metadata for humans and coding agents. A label in a
launch issue, issue template, or backlog item must exist in `.github/labels.yml`.

Run:

```bash
just label-taxonomy-check
```

Maintainers can also check or apply the configured labels against GitHub:

```bash
just github-label-check
just github-label-sync
```

`github-label-sync` only creates or updates labels listed in
`.github/labels.yml`. It does not delete extra repository labels.

## Routing Labels

- `needs triage`: maintainer review is required before work starts.
- `agent-eligible`: scoped for a coding, review, docs, or design agent.
- `good first issue`: small contributor task.
- `good-first-strategy`: paper-only strategy contribution suitable for a first
  engine PR.
- `help wanted`: maintainer-approved community work.

## Safety Labels

- `safety`: touches execution, risk, credentials, or operator friction.
- `safety-critical`: requires explicit safety review before implementation or
  merge.
- `security`: vulnerability response, public proof privacy, or security docs.

## Surface Labels

- `engine`: Python engine runtime.
- `cli`: Rust operator terminal.
- `docs`: documentation.
- `examples`: examples, templates, and demos.
- `strategy`: strategy API, plugin, runner, or deterministic strategy example.
- `market-data`: market-data adapter interfaces and fixtures.
- `contracts`: public API, Network, Intelligence, or proof packet contracts.
- `network`: ZERO Network profiles, leaderboards, and publication contracts.
- `design`: operator-facing design, public pages, or documentation IA.
- `ci`: continuous integration and repository automation.
- `containers`: Docker, Compose, or container smoke paths.
- `packaging`: package registries, Homebrew, release artifacts, or installers.

## Review Labels

- `design-review`: needs product, UX, narrative, or information architecture
  review.
- `docs-gap`: missing, stale, misleading, or agent-hostile documentation.
- `proof-pack`: proof packet methodology, reproducibility, or public evidence.
- `mcp`: MCP server, tools, resources, or transcript.

## Type Labels

- `bug`: something is not working as intended.
- `enhancement`: new feature or improvement.
- `release`: release packaging, artifacts, or versioning.

## Rules

- Do not create labels ad hoc in issue bodies. Update `.github/labels.yml`,
  this document, and `scripts/label_taxonomy_check.py` together.
- Do not edit public GitHub labels by hand when `.github/labels.yml` is meant
  to be authoritative. Use `just github-label-sync`.
- Prefer one routing label, one surface label, and one type or review label.
- Add `safety-critical` whenever a change touches live-capable behavior,
  operator friction, risk, credentials, live evidence, or public proof privacy.
