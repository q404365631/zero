# ZERO Network Index Page Example

This example builds a deterministic static HTML index for checked ZERO Network
contract pages.

```bash
just network-index-page-example
```

The page links the public-safe profile and leaderboard pages and explains the
Network publication model: opt-in, aggregate-only, self-custodial, and
proof-of-process rather than financial advice. It uses no JavaScript, remote
assets, journals, private execution details, or external links.

To regenerate the checked contract artifact:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/network-index-page/build.py \
  --output contracts/network/index.html
```
