# Release Process

ZERO does not publish production trading claims from this repository. Releases
ship installable open-source runtime artifacts, safety-contract changes, and
developer-facing examples.

## Release Gates

Before tagging:

```bash
just ci
just release-preflight
```

Required checks:

- Public proof gate for demo proof pack, Network proof chain, read-only MCP
  smoke test, and committed MCP transcript
- Python engine lint and tests
- Rust CLI formatting, clippy, and tests
- Local paper API smoke through the Rust CLI
- Hardening gate for threat model, runbooks, distribution policy, release
  verification, and public packet contracts
- Paper example smoke test
- Package dry run for Python artifacts and Rust crates
- Container smoke test in GitHub Actions
- Required docs check
- Secret scan in GitHub Actions
- CodeQL and OpenSSF Scorecard workflows

## Versioning

Use semver once public packages are published.

- Patch: bug fixes, docs, tests, non-breaking safety clarifications
- Minor: additive APIs, new examples, new adapters
- Major: breaking API, CLI, config, or safety-contract changes

## Release Notes

Every release note should include:

- What changed
- How to run it locally
- Safety impact
- Migration notes when applicable
- Known limitations

Version-specific notes live in `docs/releases/<tag>.md`. The release workflow
uses that file when it exists and falls back to `.github/RELEASE_TEMPLATE.md`
for unrehearsed tags.

## Artifacts

Target launch artifacts:

- Source archive
- Rust CLI binaries plus `SHA256SUMS`
- Python wheel/source distribution plus `SHA256SUMS`
- Container image tarball for the local paper runtime plus `SHA256SUMS`
- `SBOM.spdx.json`
- `PROVENANCE.json`

The first public release may ship source-only if the quickstart is reliable and
the limitation is called out clearly.

## Verification

Download the artifact bundle and verify its checksum file before running it:

```bash
cd dist
shasum -a 256 -c SHA256SUMS
scripts/release_verify.py dist
```

The checksum file uses the standard two-column `sha256  filename` format. A
failed checksum means the artifact should not be used.

`SBOM.spdx.json` and `PROVENANCE.json` must be present in the same directory as
the release assets. They are included in `SHA256SUMS`, so checksum verification
also detects tampering with dependency and provenance metadata.

For GitHub Release assets, download all files attached to the release into one
directory and verify the combined checksum manifest:

```bash
shasum -a 256 -c SHA256SUMS
scripts/release_verify.py .
```

Verify the GitHub artifact attestation for any downloaded executable:

```bash
gh attestation verify zero-linux -R zero-intel/zero
```

Use `zero-macos` for the macOS binary.

## Published Release Evidence

Verify a published GitHub Release from a clean download directory:

```bash
just release-evidence v0.1.1
```

The evidence command downloads the release, verifies `SHA256SUMS`, runs
`scripts/release_verify.py`, verifies executable artifact attestations, and
renders the Homebrew formula from the downloaded checksum manifest. It does not
publish package registries or mutate release assets.

The current `v0.1.1` clean-download verification is recorded in
[docs/releases/v0.1.1-evidence.md](releases/v0.1.1-evidence.md).

## Public Proof Gate

Run the public proof gate before tagging, publishing release notes, or
announcing a release candidate:

```bash
just public-proof
```

The gate verifies that the committed demo proof pack, Network proof chain,
read-only MCP server, and committed MCP transcript are internally consistent.
The tag-triggered release workflow runs the same proof commands before registry
readiness and before any package, CLI binary, or container artifact is built.

## Hardening Gate

Run the local hardening gate before tagging or publishing:

```bash
scripts/hardening_gate.sh
```

The gate verifies that the threat model, incident runbooks, distribution policy,
release verification template, intelligence contracts, and shell scripts are
present and parseable. It is intentionally lightweight so it can run inside
`just ci` on every pull request.

## CLI Install Path

Install the latest CLI binary with checksum and attestation verification:

```bash
curl -fsSL https://raw.githubusercontent.com/zero-intel/zero/main/scripts/install.sh | bash
```

The installer requires `gh`. It downloads the latest release asset for the host
OS, verifies `SHA256SUMS`, verifies the GitHub artifact attestation, and installs
`zero` to `~/.local/bin` by default. Set `ZERO_VERSION=vX.Y.Z` to install a
specific release or `ZERO_INSTALL_DIR=/path/to/bin` to choose another location.

Homebrew is also supported through the public repo tap:

```bash
brew tap zero-intel/zero https://github.com/zero-intel/zero
brew install zero
```

The formula lives at `Formula/zero.rb`, installs the `zero` CLI from the
checksummed GitHub Release asset, and does not require private package registry
access. It is generated from `SHA256SUMS` by `scripts/homebrew_formula.py`.

## Package Dry Run

Run the non-publishing package check before opening a release PR or tagging:

```bash
just public-proof
just registry-readiness
just package-dry-run
```

`just registry-readiness` checks PyPI metadata, Cargo registry metadata,
per-crate publish metadata inheritance, optional live dependencies, and
documentation guardrails. It is intentionally non-publishing. `just
package-dry-run` then builds the Python engine wheel and source distribution
into a temporary directory, and runs `cargo package --workspace --no-verify`
for the Rust crate graph using a temporary Cargo target directory. Neither
command requires PyPI, crates.io, Homebrew, Docker, or GitHub publishing tokens.

PyPI should use Trusted Publishing from GitHub Actions when enabled. Do not add
long-lived PyPI tokens to repository secrets or examples.

Current package-name assumptions:

- PyPI candidate: `zero-engine`
- crates.io candidates: the `zero-*` workspace crates plus the `zero` binary crate
- Homebrew: `zero-intel/zero` public repo tap with `Formula/zero.rb`

## Release Rehearsal

Run the release rehearsal before tagging:

```bash
just release-rehearsal
just draft-release-rehearsal
```

The rehearsal creates a temporary GitHub Actions-style artifact directory,
assembles it through `scripts/assemble_release_assets.sh`, verifies the bundle
with `scripts/release_verify.py`, then tampers with the Linux binary and proves
verification fails. This is a rollback and integrity drill: a maintainer should
not publish a release unless the verifier catches the tampered-artifact case.

The assembly step also runs `scripts/release_provenance.py` to generate
`SBOM.spdx.json` and `PROVENANCE.json` before writing the combined checksum
manifest.

`just draft-release-rehearsal` dry-runs the GitHub draft release rehearsal path:
it builds temporary release assets, verifies them, and renders a Homebrew formula
from the combined checksum manifest. It does not contact GitHub unless explicitly
run as:

```bash
scripts/draft_release_rehearsal.sh --execute
```

The execute mode creates a temporary draft prerelease, downloads all attached
assets into a fresh directory, runs `scripts/release_verify.py`, renders the
Homebrew formula from the fresh download, then deletes the draft release and its
temporary tag. Use `--keep` only when a maintainer needs to inspect the draft
release manually.

## Tag Workflow Rehearsal

Before the first public release candidate, and after any material release
workflow change, run the high-fidelity GitHub tag workflow drill:

```bash
scripts/release_workflow_rehearsal.sh --execute
```

The script creates a temporary prerelease tag on `origin/main`, waits for the
real `.github/workflows/release.yml` run, verifies that `public-proof`,
registry-readiness, package builds, CLI builds, container smoke, and draft
release assembly all succeeded, downloads the generated draft release from
GitHub, verifies checksums, release provenance, Homebrew formula rendering, and
executable attestations, then deletes the draft release and temporary tag. Use
`--keep` only when a maintainer needs to inspect the temporary draft manually.

## Current Automation

`.github/workflows/release.yml` runs on tags shaped like `v*.*.*` and builds:

- Public proof preflight before release artifacts are built
- Registry-readiness preflight before package artifacts are built
- Python wheel and source distribution for `engine/`
- Linux and macOS CLI binaries for the `zero` crate
- Paper-mode Docker image smoke tests and an exported image tarball
- SHA-256 checksum files for each artifact group
- A draft GitHub Release containing the wheel, source distribution, CLI
  binaries, paper image tarball, `SBOM.spdx.json`, `PROVENANCE.json`, and a
  combined `SHA256SUMS`
- Release asset verification through `scripts/release_verify.py` before
  attestation and draft release upload
- GitHub artifact attestations for release assets listed in the combined
  checksum manifest
- Dry-run draft release rollback rehearsal in CI; execute mode remains
  maintainer-triggered only
- Maintainer-triggered tag workflow rehearsal through
  `scripts/release_workflow_rehearsal.sh --execute`

The workflow uploads artifacts to the GitHub Actions run and attaches the
assembled release bundle to a draft GitHub Release. It does not publish to PyPI,
crates.io, Docker Hub, or GHCR yet. Package publishing should be added only
after repository ownership, package names, signing, and token permissions are
finalized.

## Homebrew Formula

The committed formula is `Formula/zero.rb`. To update it for a new release,
render the formula from a downloaded and verified release directory:

```bash
scripts/homebrew_formula.py <downloaded-release-dir> --tag v0.1.1 --output zero.rb
```

The formula uses the `zero-macos` and `zero-linux` checksums from `SHA256SUMS`,
installs the CLI only, states that the public runtime defaults to paper mode,
and links to the release and safety docs in its caveats.

## Signing And Provenance

The release workflow uses GitHub artifact attestations through `actions/attest`.
For public repositories, GitHub signs the attestation with Sigstore-backed
provenance. Maintainers should leave releases in draft until checksum and
attestation verification both pass from a fresh checkout or clean download
directory.

ZERO also ships local provenance metadata:

- `SBOM.spdx.json`: SPDX 2.3 package/component metadata generated from
  `engine/pyproject.toml`, `cli/Cargo.lock`, workspace crate manifests, and
  release assets.
- `PROVENANCE.json`: source commit, branch/tag, dirty-state flag, asset hashes,
  and policy assertions that paper mode is default, live execution evidence is
  not claimed, and package-registry publication remains disabled.

Do not publish package-registry artifacts until the registry channel has an
owner, rollback path, least-privilege token plan, and documented support
expectation in [distribution.md](distribution.md).
