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

### Add API compatibility fixture tests

Labels: `help wanted`, `api`, `cli`, `tests`

Pin Python paper API responses against the Rust client's expected wire shapes.

Acceptance:

- Fixtures cover `/v2/status`, `/positions`, `/risk`, `/brief`, `/rejections`,
  and `POST /execute`.
- Tests fail if the Python paper API drops a field required by the Rust CLI.
- Tests remain paper-only and require no network beyond localhost.

## Maintainer Tasks

### First release candidate

Labels: `release`

Tag the first source-only release after the public repo is created on GitHub and
CI is green.

Acceptance:

- Release notes include safety impact and known limitations.
- Artifacts from `.github/workflows/release.yml` are attached.
- No claims depend on private production data.
