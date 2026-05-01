# Launch Issues

Create these issues before opening the repository. They are intentionally small
and scoped so a new contributor can land a useful first PR without private
context.

## Good First Issue: Add Paper Example Output Summary

Labels: `good first issue`, `docs`, `examples`

Add a short "Expected output" section to `examples/paper-trading/README.md`.
Use abbreviated JSON, not a long transcript.

Acceptance:

- The README explains fills, rejections, and reduce-only behavior.
- The example command remains `just example`.
- The output summary stays deterministic and paper-only.

## Good First Issue: Add Docker Daemon Troubleshooting Note

Labels: `good first issue`, `docs`, `containers`

Add a short troubleshooting note to `docs/local-development.md` for
`just container-smoke` when Docker is installed but the daemon is not running.

Acceptance:

- The note explains the daemon requirement without assuming Docker Desktop.
- The note keeps the container path paper-only.
- The note does not weaken CI expectations.

## Help Wanted: One-Line CLI Install Path

Labels: `help wanted`, `release`, `cli`, `packaging`

Add a documented one-line install path for the Rust CLI once public release
artifacts are available.

Acceptance:

- `README.md` shows the install command near the quickstart.
- `docs/release.md` names the supported install path and artifact source.
- The command verifies checksums before placing a binary on `PATH`.
- The path works without private package registry access.

## Maintainer Task: Signed Release Provenance

Labels: `release`, `security`

Add signed release provenance once the public repository, tags, and token
permissions are finalized.

Acceptance:

- `SHA256SUMS` is signed or accompanied by verifiable provenance.
- `docs/release.md` explains verification from a fresh clone.
- Signing does not require contributor secrets for normal pull requests.

## Maintainer Task: First Release Candidate

Labels: `release`

Tag the first release candidate after the public repo is created on GitHub and
CI is green.

Acceptance:

- Release notes use `.github/RELEASE_TEMPLATE.md`.
- Artifacts from `.github/workflows/release.yml` are attached or linked.
- `SHA256SUMS` verification is called out.
- No claims depend on private production data.
