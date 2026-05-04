## What Changed

- 

## How To Run Locally

```bash
git clone https://github.com/zero-intel/zero.git
cd zero
python3 -m venv .venv
source .venv/bin/activate
just bootstrap
just demo
just paper-api-smoke
```

## Safety Impact

- Paper mode remains the default first-run path.
- `POST /execute` in the public engine is simulated and returns `simulated=true`.
- No exchange credentials or private production data are required.

## Verification

- [ ] `just ci`
- [ ] `just public-proof`
- [ ] `just release-preflight`
- [ ] `scripts/hardening_gate.sh`
- [ ] `just registry-readiness`
- [ ] `just release-rehearsal`
- [ ] `just draft-release-rehearsal`
- [ ] `scripts/release_workflow_rehearsal.sh --execute` has passed for a temporary rehearsal tag, or this release explains why the high-fidelity tag workflow drill was intentionally skipped.
- [ ] Draft GitHub Release contains the Python package, CLI binaries, paper image tarball, and `SHA256SUMS`.
- [ ] `scripts/release_verify.py <downloaded-release-dir>` passes.
- [ ] `SBOM.spdx.json` and `PROVENANCE.json` are attached and included in `SHA256SUMS`.
- [ ] `scripts/homebrew_formula.py <downloaded-release-dir> --tag <tag> --output zero.rb` renders a formula without publishing a tap.
- [ ] `shasum -a 256 -c SHA256SUMS` passes after downloading all attached release assets.
- [ ] `gh attestation verify zero-linux -R zero-intel/zero` passes.
- [ ] `gh attestation verify zero-macos -R zero-intel/zero` passes.
- [ ] After publication, `scripts/release_evidence.py <tag>` passes from a clean download.
- [ ] PyPI, crates.io, Homebrew, Docker Hub, and GHCR package registry publication remains disabled unless this release explicitly includes an ownership-proof section.
- [ ] `docs/threat-model.md`, `docs/incident-runbooks.md`, `docs/dependency-policy.md`, and `docs/distribution.md` are reviewed for this release.

## Known Limitations

- Live exchange execution is self-custodial and must pass local preflight.
- Realtime ZERO Intelligence, hosted historical datasets, and enterprise
  support are not included in this release.

## Migration Notes

- None.
