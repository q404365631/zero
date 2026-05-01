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
- Paper example smoke test
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
- Rust CLI binaries
- Python package
- Container image for the local paper runtime

The first public release may ship source-only if the quickstart is reliable and
the limitation is called out clearly.

## Current Automation

`.github/workflows/release.yml` runs on tags shaped like `v*.*.*` and builds:

- Python wheel and source distribution for `engine/`
- Linux and macOS CLI binaries for the `zero` crate
- Paper-mode Docker image smoke tests

The workflow uploads artifacts to the GitHub Actions run. It does not publish to
PyPI, crates.io, Homebrew, Docker Hub, or GitHub Releases yet. Publishing should
be added only after repository ownership, package names, signing, and token
permissions are finalized.
