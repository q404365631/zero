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
- [ ] Release artifacts include `SHA256SUMS`.
- [ ] `shasum -a 256 -c SHA256SUMS` passes for every artifact bundle.

## Known Limitations

- Live exchange execution is not included in this repository.
- Hosted control plane, managed deployments, model/key gateway, premium connectors,
  and enterprise audit exports are commercial ZERO Cloud surfaces.

## Migration Notes

- None.
