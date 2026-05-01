# ZERO Network Profile Page Example

This example builds a static public profile page from one already-redacted
`zero.network.profile.v1` packet.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/network-profile-page/build.py
```

Or use:

```bash
just network-profile-page-example
```

The page shows aggregate behavior, verification badges, and proof hash. It does
not render raw decisions, symbols, trace IDs, idempotency keys, wallet
addresses, exchange order IDs, strategy labels, or private notes.

Write the deterministic contract artifact:

```bash
PYTHONPATH="$PWD/engine/src" \
  python3 examples/network-profile-page/build.py \
  --output contracts/network/profile.html
```

Contributor rules:

- Render from redacted public profiles only.
- Keep the page deterministic and static.
- Escape all profile-provided text.
- Do not introduce JavaScript or remote assets.
- Treat public profiles as proof-of-process, not financial advice.
