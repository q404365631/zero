# Launch Issues

Create these issues before opening the repository. They are intentionally small
and scoped so a new contributor can land a useful first PR without private
context.

Validate the seed issues locally:

```bash
just launch-issue-config-check
```

Maintainers can create missing GitHub issues after labels are synced:

```bash
just github-label-sync
just github-launch-issue-sync
```

The sync only creates missing issues with exact matching titles. It does not
edit existing issues, reopen closed issues, or delete anything.

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

## Good First Issue: Add ZERO Network Stale Publication Window Docs

Labels: `good first issue`, `network`, `docs`, `security`

Add a short stale-publication policy document for public Network profiles and
leaderboards. The ingestion contract already rejects missing consent, proof
mismatches, inconsistent aggregate metrics, and duplicate accepted handles or
proofs. This issue should document how hosted Network pages should mark stale
or expired packets once real publication timestamps are available.

Acceptance:

- The document explains why stale packets should lose freshness badges even
  when proof hashes remain valid.
- The document proposes public-safe freshness windows for paper and live
  profile packets.
- The document keeps leaderboard rank proof-of-process, not PnL.
- The policy does not require exchange credentials or hosted custody.
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
