# Launch Backlog

This is the public launch backlog seed. Convert these into GitHub issues before
opening the repository.

## Good First Issues

### Add a paper-market fixture file

Labels: `good first issue`, `engine`, `examples`

Create a small deterministic JSON fixture under `examples/paper-trading/` and
update the example to read it instead of hardcoding orders. Keep the current
output shape stable.

Acceptance:

- `python examples/paper-trading/run.py` still works.
- Engine tests still pass.
- No network or credentials are required.

### Improve paper example README output section

Labels: `good first issue`, `docs`, `examples`

Add a short "Expected output" section to `examples/paper-trading/README.md`.
Use abbreviated JSON, not a long transcript.

Acceptance:

- The README explains fills, rejections, and reduce-only behavior.
- The example command remains `just example`.

### Add CLI quickstart link from root README

Labels: `good first issue`, `cli`, `docs`

Make the CLI quickstart easier to find from the root README without duplicating
the long CLI README.

Acceptance:

- Root README links to `cli/README.md`.
- No CLI behavior changes.

## Help Wanted

### Strategy plugin template

Labels: `help wanted`, `engine`, `examples`

Design a minimal strategy plugin interface for paper mode. Start with a proposal
before code.

Acceptance:

- Proposal names the public interface and safety constraints.
- The first implementation is paper-only.
- Tests cover at least one accepted and one rejected order.

### Market data adapter template

Labels: `help wanted`, `engine`, `examples`

Create a local adapter example that reads deterministic candles from disk.

Acceptance:

- No external API key.
- Adapter is documented in `docs/api.md` or a dedicated adapter doc.
- CI covers the example.

### Container smoke test in CI

Labels: `help wanted`, `ci`, `release`

Add a CI job that builds the public Docker image and runs the paper example in
the container.

Acceptance:

- The job does not publish an image.
- The job runs only paper mode.
- The job fails if the example exits non-zero.

## Maintainer Tasks

### First release candidate

Labels: `release`

Tag the first source-only release after the public repo is created on GitHub and
CI is green.

Acceptance:

- Release notes include safety impact and known limitations.
- Artifacts from `.github/workflows/release.yml` are attached.
- No claims depend on private production data.
