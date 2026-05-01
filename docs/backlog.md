# Launch Backlog

This is the public launch backlog seed. Convert these into GitHub issues before
opening the repository.

## Good First Issues

### Improve local paper API output docs

Labels: `good first issue`, `docs`, `api`

Add a short "Expected output" section for `just paper-api-smoke` to
`docs/local-development.md`. Use abbreviated terminal output, not a long
transcript.

Acceptance:

- The docs show a healthy `zero doctor` row.
- The docs show `run status` reading from `http://127.0.0.1:8765`.
- The docs mention that the auth warning is expected without a token.

### Add paper API examples for `/execute`

Labels: `good first issue`, `docs`, `examples`

Add a small `curl` example for `POST /execute` to the engine README.

Acceptance:

- The example uses paper mode only.
- The example includes an idempotency key.
- The example explains that `simulated=true` is the expected result.

## Help Wanted

### Release artifact checksums

Labels: `help wanted`, `release`, `security`

Generate SHA-256 checksum files for release workflow artifacts.

Acceptance:

- Python package, CLI binary, and container artifact outputs have checksums.
- Checksums are uploaded beside the artifacts.
- Release docs explain how to verify them.

## Maintainer Tasks

### First release candidate

Labels: `release`

Tag the first source-only release after the public repo is created on GitHub and
CI is green.

Acceptance:

- Release notes include safety impact and known limitations.
- Artifacts from `.github/workflows/release.yml` are attached.
- No claims depend on private production data.
