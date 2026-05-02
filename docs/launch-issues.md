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

## Good First Issue: Add ZERO Network Anti-Gaming Rules

Labels: `good first issue`, `network`, `docs`, `security`

Add a short anti-gaming rules document for public Network profiles and
leaderboards.

Acceptance:

- The document explains why leaderboard rank is proof-of-process, not PnL.
- The document names first-pass abuse cases: fake volume, duplicate handles,
  replayed proof packets, sybil profiles, and stale publication.
- The document proposes public-safe mitigations without requiring exchange
  credentials or hosted custody.
- `docs/zero-network.md` links to the document.

## Help Wanted: Homebrew Tap

Labels: `help wanted`, `release`, `cli`, `packaging`

Add a Homebrew tap or formula after public release artifact names stabilize.

Acceptance:

- `README.md` links the Homebrew install command.
- The formula installs the checksummed GitHub Release binary or builds from source.
- `docs/release.md` names Homebrew as a supported distribution path.
- The path works without private package registry access.

## Maintainer Task: First Public Release Verification

Labels: `release`, `security`

Verify the first public release from a clean download directory before
publishing it.

Acceptance:

- The draft GitHub Release includes all expected assets.
- `shasum -a 256 -c SHA256SUMS` passes.
- `gh attestation verify zero-linux -R zero-intel/zero` passes.
- `gh attestation verify zero-macos -R zero-intel/zero` passes.

## Maintainer Task: First Release Candidate

Labels: `release`

Tag the first release candidate after the public repo is created on GitHub and
CI is green.

Acceptance:

- Release notes use `.github/RELEASE_TEMPLATE.md`.
- Artifacts from `.github/workflows/release.yml` are attached or linked.
- `SHA256SUMS` verification is called out.
- No claims depend on private production data.
