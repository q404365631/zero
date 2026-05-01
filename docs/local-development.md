# Local Development

ZERO's public development path must work without exchange credentials, cloud
accounts, or private deployment access.

## Prerequisites

- Python 3.11 or newer
- Rust toolchain from `cli/rust-toolchain.toml`
- `just`
- Git

## First Run

```bash
git clone https://github.com/zero-intel/zero.git
cd zero
python3 -m venv .venv
source .venv/bin/activate
just bootstrap
just demo
just example
```

Expected behavior:

- The engine installs in editable mode.
- The demo runs in paper mode.
- At least one paper order is filled.
- At least one unsafe or over-limit order is rejected.
- No real exchange private key is requested.

## Daily Commands

```bash
just engine-lint
just engine-test
just cli-lint
just cli-test
just ci
```

`just ci` is the local release gate. If a change touches only docs, run
`just docs-check` and state that scope in the pull request.

## Python Engine

```bash
cd engine
python -m pip install -e ".[dev]"
ruff check .
pytest
python -m zero_engine.demo
```

The public engine starts with a small safety contract. Keep changes focused and
add tests beside the behavior being changed.

## Rust CLI

```bash
cd cli
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The CLI is an operator surface. Any command that increases risk must keep the
friction model intact; risk-reducing commands must remain fast.

## Local State

Ignored local state includes:

- `.venv/`
- `.pytest_cache/`
- `.ruff_cache/`
- `target/`
- `dist/`
- `*.db`, `*.sqlite*`, `*.wal`, `*.shm`
- `.env`

Do not commit generated state, local credentials, runtime databases, or exchange
material.
