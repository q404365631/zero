# Launch Backlog

This is the public launch backlog seed. Convert these into GitHub issues before
opening the repository.

## Good First Issues

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
