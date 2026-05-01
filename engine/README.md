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
