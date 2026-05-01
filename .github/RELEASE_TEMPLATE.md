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
- [ ] Draft GitHub Release contains the Python package, CLI binaries, paper image tarball, and `SHA256SUMS`.
- [ ] `shasum -a 256 -c SHA256SUMS` passes after downloading all attached release assets.
- [ ] `gh attestation verify zero-linux -R zero-intel/zero` passes.
- [ ] `gh attestation verify zero-macos -R zero-intel/zero` passes.

## Known Limitations

- Live exchange execution is not included in this repository.
- Railway deployment, public profiles, leaderboards, realtime ZERO Intelligence,
  historical datasets, and enterprise support are not included in this release.

## Migration Notes

- None.
