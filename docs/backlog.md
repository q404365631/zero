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

### Add Homebrew tap

Labels: `help wanted`, `release`, `cli`, `packaging`

Add a Homebrew tap or formula after public release artifact names stabilize.

Acceptance:

- `README.md` links the Homebrew install command.
- The formula installs the checksummed GitHub Release binary or builds from source.
- `docs/release.md` names Homebrew as a supported distribution path.
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
