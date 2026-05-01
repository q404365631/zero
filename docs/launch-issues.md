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

## Help Wanted: Package Publishing Dry Runs

Labels: `help wanted`, `release`, `packaging`

Add non-publishing checks for the first public package paths.

Acceptance:

- The Python engine package can build an sdist and wheel locally.
- The Rust CLI crates pass `cargo package --no-verify` or documented equivalent.
- The check does not require publishing tokens.
- Any name-ownership assumptions are documented in `docs/release.md`.

## Help Wanted: CLI Quickstart Terminal Capture

Labels: `help wanted`, `cli`, `docs`

Add an abbreviated terminal capture for the CLI quickstart.

Acceptance:

- The capture shows `zero doctor` against `http://127.0.0.1:8765`.
- The capture shows `zero run status`.
- The capture excludes machine-specific paths and secrets.

## Maintainer Task: First Release Candidate

Labels: `release`

Tag the first release candidate after the public repo is created on GitHub and
CI is green.

Acceptance:

- Release notes use `.github/RELEASE_TEMPLATE.md`.
- Artifacts from `.github/workflows/release.yml` are attached or linked.
- `SHA256SUMS` verification is called out.
- No claims depend on private production data.
