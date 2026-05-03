# Review Ownership

ZERO uses `.github/CODEOWNERS` as the public source of truth for review
ownership. The initial owner is `@squaeragent`; additional maintainers or teams
can be added after they have repository access and understand the safety model.

## Protected Surfaces

The ownership gate covers:

- GitHub workflows, issue templates, labels, and release automation.
- Engine execution, safety, immune, reconciliation, and Hyperliquid boundaries.
- CLI friction, operator configuration, headless control, doctor, and engine
  client crates.
- Public contracts, OpenAPI schemas, live evidence docs, threat model,
  incident runbooks, release docs, and distribution policy.

## Change Policy

Routine documentation and example changes can still be reviewed normally.
Changes to protected surfaces require approval from a listed owner of the
affected area and must preserve paper-first defaults, fail-closed behavior,
credential redaction, and risk-reducing command priority.

Do not remove or weaken CODEOWNERS entries to make a pull request easier to
merge. If ownership needs to change, update `.github/CODEOWNERS`,
`docs/review-ownership.md`, and the CODEOWNERS checker in the same pull
request.

## Local Check

```bash
just codeowners-check
```

This validates that required safety-critical patterns exist, each entry has at
least one syntactically valid owner, wildcard ownership is present, and no
placeholder owners are committed.
