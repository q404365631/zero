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

## Artifacts

Target launch artifacts:

- Source archive
- Rust CLI binaries plus `SHA256SUMS`
- Python wheel/source distribution plus `SHA256SUMS`
- Container image tarball for the local paper runtime plus `SHA256SUMS`

The first public release may ship source-only if the quickstart is reliable and
the limitation is called out clearly.

## Verification

Download the artifact bundle and verify its checksum file before running it:

```bash
cd dist
shasum -a 256 -c SHA256SUMS
```

The checksum file uses the standard two-column `sha256  filename` format. A
failed checksum means the artifact should not be used.

For GitHub Release assets, download all files attached to the release into one
directory and verify the combined checksum manifest:

```bash
shasum -a 256 -c SHA256SUMS
```

## Package Dry Run

Run the non-publishing package check before opening a release PR or tagging:

```bash
just package-dry-run
```

The check builds the Python engine wheel and source distribution into a
temporary directory, then runs `cargo package --workspace --no-verify` for the
Rust crate graph using a temporary Cargo target directory. It does not require
PyPI, crates.io, Homebrew, Docker, or GitHub publishing tokens.

Current package-name assumptions:

- PyPI candidate: `zero-engine`
- crates.io candidates: the `zero-*` workspace crates plus the `zero` binary crate
- Homebrew candidate: a future `zero-intel/zero` tap or equivalent one-line installer

## Current Automation

`.github/workflows/release.yml` runs on tags shaped like `v*.*.*` and builds:

- Python wheel and source distribution for `engine/`
- Linux and macOS CLI binaries for the `zero` crate
- Paper-mode Docker image smoke tests and an exported image tarball
- SHA-256 checksum files for each artifact group
- A draft GitHub Release containing the wheel, source distribution, CLI
  binaries, paper image tarball, and a combined `SHA256SUMS`

The workflow uploads artifacts to the GitHub Actions run and attaches the
assembled release bundle to a draft GitHub Release. It does not publish to PyPI,
crates.io, Homebrew, or Docker Hub yet. Package publishing should be added only
after repository ownership, package names, signing, and token permissions are
finalized.

## Signing And Provenance

The release workflow currently provides checksum verification, not a signed
provenance chain. Before marking a release as final, maintainers should add
keyless artifact signing or another verifiable provenance mechanism and document
the verification command here.
