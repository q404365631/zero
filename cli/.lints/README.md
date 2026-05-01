# CLI lint gates

Hard enforcements of the spec's anti-patterns (§25) and honesty
discipline (§3.1, §3.8).

- `check-anti-patterns.sh` — grep-based lint for forbidden strings:
  celebration copy, marketing copy in chrome, model strings in chrome,
  auto-execute defaults, and unguarded numeric formatting of engine
  state.

Run locally before pushing:

```
cd cli && ./.lints/check-anti-patterns.sh
```

CI runs these on every PR touching `cli/**`.

The clippy + type-level enforcement (ADR-010) lives inside the crates;
this directory is the last-line grep safety net.
