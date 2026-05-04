# ZERO Network Stale Profile Fixture

This example builds a deterministic public-safe fixture where the profile proof
still verifies, but the freshness badge has expired.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/network-stale-profile/build.py
```

Or use:

```bash
just network-stale-profile-example
```

The output separates proof validity from operator freshness:

- `proof.status=valid` means the profile proof hash recomputes.
- `freshness.status=stale` means the packet is archive evidence, not active
  operator status.
- `claim_boundary.active_operator_status_asserted=false` keeps the public
  fixture from implying current liveness.

The fixture contains no wallet material, raw trades, private notes, raw
exchange order IDs, trace IDs, idempotency keys, symbols, or strategy labels.
