# Operator Isolation

ZERO treats local mutable state as operator-owned. A checkout can be shared by
multiple operators, but runtime artifacts, daemon state, session logs, and
credential-store accounts must stay partitioned.

## Filesystem Layout

`config.toml` remains the active local config:

```text
<zero_dir>/config.toml
```

Mutable state is partitioned by a stable slug derived from
`identity.handle`:

```text
<zero_dir>/operators/<operator-slug>/
  zero.log
  sock
  state/
    state.db
    headless.json
    wraps/
```

The slug is lowercase ASCII, keeps `-`, `_`, and `.`, folds whitespace and
other punctuation to `-`, and falls back to `local-operator`.

## Credentials

New OS keychain writes use operator-specific account names:

```text
operator:<operator-slug>
```

The keychain services remain:

- `dev.getzero.zero` for the engine bearer token;
- `dev.getzero.hyperliquid` for the local Hyperliquid API-wallet private key.

Reads fall back to the legacy `default` account for migration compatibility,
but new writes are operator-scoped. Private keys must never be written to
`config.toml`, journals, audit exports, docs, fixtures, or public issue
threads.

## Doctor Checks

`zero doctor` includes two local isolation rows:

- `operator_partition` verifies the resolved partition path and warns when
  legacy shared artifacts are present at the old top level.
- `credential_partition` reports the derived operator-scoped keychain account
  without reading or printing any secret material.

Legacy shared artifacts include:

- `<zero_dir>/state.db`
- `<zero_dir>/zero.log`
- `<zero_dir>/sock`
- `<zero_dir>/state/headless.json`

Archive or migrate those files before running multi-operator workflows from
one machine.

## Boundary

Operator isolation is not custody delegation or hosted RBAC. It prevents local
state and credential-slot cross-talk on a shared workstation. Authorization,
team membership, billing, and signed hosted deployment identity belong to the
future hosted control plane and ZERO Intelligence API boundary.
