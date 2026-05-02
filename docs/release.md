# Release Process

ZERO does not publish production trading claims from this repository. Releases
ship installable open-source runtime artifacts, safety-contract changes, and
developer-facing examples.

## Release Gates

Before tagging:

```bash
just ci
```

Required checks:

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

After the first GitHub Release is published, install the latest CLI binary with:

```bash
curl -fsSL https://raw.githubusercontent.com/zero-intel/zero/main/scripts/install.sh | bash
```

The installer requires `gh`. It downloads the latest release asset for the host
OS, verifies `SHA256SUMS`, verifies the GitHub artifact attestation, and installs
`zero` to `~/.local/bin` by default. Set `ZERO_VERSION=vX.Y.Z` to install a
specific release or `ZERO_INSTALL_DIR=/path/to/bin` to choose another location.

## Package Dry Run

Run the non-publishing package check before opening a release PR or tagging:

```bash
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
- Homebrew candidate: a future `zero-intel/zero` tap or equivalent one-line installer

## Release Rehearsal

Run the release rehearsal before tagging:

```bash
just release-rehearsal
```

The rehearsal creates a temporary GitHub Actions-style artifact directory,
assembles it through `scripts/assemble_release_assets.sh`, verifies the bundle
with `scripts/release_verify.py`, then tampers with the Linux binary and proves
verification fails. This is a rollback and integrity drill: a maintainer should
not publish a release unless the verifier catches the tampered-artifact case.

The assembly step also runs `scripts/release_provenance.py` to generate
`SBOM.spdx.json` and `PROVENANCE.json` before writing the combined checksum
manifest.

## Current Automation

`.github/workflows/release.yml` runs on tags shaped like `v*.*.*` and builds:

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

The workflow uploads artifacts to the GitHub Actions run and attaches the
assembled release bundle to a draft GitHub Release. It does not publish to PyPI,
crates.io, Homebrew, or Docker Hub yet. Package publishing should be added only
after repository ownership, package names, signing, and token permissions are
finalized.

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
