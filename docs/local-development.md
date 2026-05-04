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
- The local candle fixture is read from disk.
- No real exchange private key is requested.

## Daily Commands

```bash
just engine-lint
just engine-test
just cli-lint
just cli-test
just stale-artifact-check
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

Run the local paper API in one terminal:

```bash
just paper-api
```

Then inspect it from the CLI in another terminal:

```bash
cd cli
cargo run -p zero -- --api http://127.0.0.1:8765 doctor
cargo run -p zero -- --api http://127.0.0.1:8765 run status
```

The same integration check is available as:

```bash
just paper-api-smoke
```

Expected abbreviated output from the manual CLI path:

```text
[ ok ] engine_reachable  zero-paper-engine v0.1.1 (http://127.0.0.1:8765/)
[ ok ] engine_healthy    ok
[warn] auth              no token set - read-only endpoints only
[ ok ] ws_reachable      ws://127.0.0.1:8765/ws

engine: regime=PAPER MARKET. Local deterministic demo.  confidence=90 (paper)
today: trades=0  wins=0  pnl=+0.00
```

The auth warning is expected for the local paper API when no token is set.
See [cli-quickstart.md](cli-quickstart.md) for the fuller redacted terminal
capture.

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

CLI runtime state is operator-partitioned under
`<zero_dir>/operators/<operator-slug>/`. The active config remains at
`<zero_dir>/config.toml`; the default session DB, TUI log, headless socket, and
headless state live under the operator partition. `zero doctor` warns when
legacy shared state files remain at the top level. See
[Operator Isolation](operator-isolation.md).

Ignored local state includes:

- `.venv/`
- `.pytest_cache/`
- `.ruff_cache/`
- `__pycache__/`
- `*.pyc`
- `target/`
- `dist/`
- `build/`
- `*.egg-info/`
- `coverage.xml`
- `*.db`, `*.sqlite*`, `*.wal`, `*.shm`
- `.env`

Do not commit generated state, local credentials, runtime databases, or exchange
material.

## Container Path

The container path is also paper-only:

```bash
docker build -t zero-public:local .
docker run --rm zero-public:local
docker run --rm zero-public:local python /app/examples/paper-trading/run.py
```

With Compose:

```bash
docker compose run --rm zero-paper-example
```

## Container Troubleshooting

`just container-smoke` requires a Docker-compatible daemon, not only the Docker
CLI binary. If `docker --version` works but `just container-smoke` fails with a
message like `Cannot connect to the Docker daemon`, start whichever local
daemon your workstation uses and rerun the command.

Common local options include Docker Desktop, Colima, OrbStack, Rancher Desktop,
or another Docker-compatible runtime. ZERO does not require a specific desktop
product.

The container smoke path remains paper-only. It builds the local image, runs
the default paper demo, and runs the deterministic paper trading example. It
must not request exchange credentials, wallet material, live-capital
configuration, or private ZERO infrastructure.
