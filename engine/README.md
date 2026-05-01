# ZERO Engine

Paper-first trading engine runtime for ZERO.

This package is the public seed of the open-core engine. It intentionally starts with a small, testable safety contract before live exchange adapters are added.

## Install

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

## Demo

```bash
zero-paper-demo
```

## Local API

```bash
zero-paper-api
```

The local paper API listens on `http://127.0.0.1:8765` by default and
exposes the paper-mode subset of the engine contract used by the Rust CLI:
`/`, `/health`, `/v2/status`, `/positions`, `/risk`, `/brief`,
`/regime`, `/evaluate/{coin}`, `/pulse`, `/approaching`, `/rejections`,
`/operator/state`, `POST /execute`, `POST /auto/toggle`, and
`POST /operator/events`.

## Test

```bash
pytest
ruff check .
```

## Safety Contract

- Paper mode is the first-run path.
- Risk-increasing orders are evaluated before fill.
- Reduce-only orders bypass risk-increasing friction.
- Rejections are recorded explicitly.
- No real exchange private key is required.

The private ZERO engine will be ported into this package behind stable public contracts, not copied wholesale.
