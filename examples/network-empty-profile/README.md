# ZERO Network Empty Profile Fixture

This example builds a deterministic public-safe `zero.network.profile.v1`
packet with no public decisions.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/network-empty-profile/build.py
```

Or use:

```bash
just network-empty-profile-example
```

The output is intentionally empty: it proves the profile can render without
claiming behavior, PnL, custody, or live trading.
