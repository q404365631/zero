# ZERO

Self-hosted autonomous trading engine and operator terminal.

ZERO is for engineers who want to build, test, and supervise AI-assisted trading systems without handing execution to a black box. The public repo starts with paper mode, local operation, explicit safety gates, inspectable decisions, and extension points for market data, strategies, and operator workflows.

> This repository is the open-core runtime. Hosted team workflows, managed deployments, private data services, and enterprise integrations live in ZERO Cloud.

## What Is Open Source

- Local engine runtime
- Paper-trading adapter
- Safety and risk gates
- Operator CLI
- API contracts
- Strategy/plugin examples
- Tests and reproducible local demo

## What Is Commercial

- Hosted control plane
- Managed operator teams
- Fleet deployment and monitoring
- Model/key gateway
- Premium connectors
- Enterprise audit exports, support, and SLAs

## Quickstart

```bash
git clone https://github.com/zero-intel/zero.git
cd zero
just bootstrap
just demo
```

The first public demo runs in paper mode and requires no exchange private key.

## Repository Layout

```text
zero/
├── engine/              Python engine runtime
├── cli/                 Rust operator terminal
├── docs/                Architecture, safety model, API, contributor docs
├── examples/            Paper trading and plugin examples
└── .github/             CI, issue templates, PR template
```

## Development

```bash
python3 -m venv .venv
source .venv/bin/activate
just bootstrap
just lint
just test
just ci
```

## Safety

ZERO must be safe by default:

- Paper mode is the default local demo.
- Risk-increasing actions need explicit operator confirmation.
- Risk-reducing actions must not be blocked by friction gates.
- Decisions should be logged with source, timestamp, and confidence.
- No secrets are required for first-run contribution work.

Read [docs/safety-model.md](docs/safety-model.md) before adding execution or risk logic.

## Contributing

Start with [CONTRIBUTING.md](CONTRIBUTING.md). Good first issues are labeled `good first issue`; larger design work should start as a discussion or proposal.

## License

Apache-2.0. See [LICENSE](LICENSE).
