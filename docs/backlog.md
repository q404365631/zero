# Launch Backlog

This is the public launch backlog seed. Use [launch issues](launch-issues.md)
for ready-to-create issue bodies before opening the repository.

## Good First Issues

### Add paper example output summary

Labels: `good first issue`, `docs`, `examples`

Add a short "Expected output" section to `examples/paper-trading/README.md`.
Use abbreviated JSON, not a long transcript.

Acceptance:

- The README explains fills, rejections, and reduce-only behavior.
- The example command remains `just example`.
- The output summary stays deterministic and paper-only.

### Add Docker daemon troubleshooting note

Labels: `good first issue`, `docs`, `containers`

Add a short troubleshooting note to `docs/local-development.md` for
`just container-smoke` when Docker is installed but the daemon is not running.

Acceptance:

- The note explains the daemon requirement without assuming Docker Desktop.
- The note keeps the container path paper-only.
- The note does not weaken CI expectations.

## Help Wanted

### Add one-line CLI install path

Labels: `help wanted`, `release`, `cli`, `packaging`

Add a documented one-line install path for the Rust CLI once public release
artifacts are available.

Acceptance:

- `README.md` shows the install command near the quickstart.
- `docs/release.md` names the supported install path and artifact source.
- The command verifies checksums before placing a binary on `PATH`.
- The path works without private package registry access.

## Maintainer Tasks

### First release candidate

Labels: `release`

Tag the first source-only release after the public repo is created on GitHub and
CI is green.

Acceptance:

- Release notes include safety impact and known limitations.
- Artifacts from `.github/workflows/release.yml` are attached.
- No claims depend on private production data.
