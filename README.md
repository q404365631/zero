# ZERO

[![CI](https://github.com/zero-intel/zero/actions/workflows/ci.yml/badge.svg)](https://github.com/zero-intel/zero/actions/workflows/ci.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/zero-intel/zero/badge)](https://securityscorecards.dev/viewer/?uri=github.com/zero-intel/zero)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Self-hosted Autonomous Risk Operations runtime and operator terminal.

ZERO is for engineers who want to build, test, and supervise AI-assisted trading systems without handing execution to a black box. The public repo starts with paper mode, local operation, explicit safety gates, inspectable decisions, and extension points for market data, strategies, operator workflows, and deployment targets.

> This repository is the open runtime. Railway-first deployment and public profiles belong in the public product surface. ZERO Intelligence is the commercial API and subscription layer built from verified autonomous behavior.

## What Is Open Source

- Local engine runtime
- Paper-trading adapter
- Safety and risk gates
- Operator CLI
- API contracts
- Strategy/plugin examples
- Railway and Docker deployment paths
- Public profile and leaderboard contracts
- Delayed public intelligence snapshots
- Tests and reproducible local demo

## What Is Commercial

- Realtime ZERO Intelligence API
- Historical intelligence datasets
- Advanced cohort and benchmark analytics
- Commercial intelligence connectors and enrichment feeds
- Higher API limits, webhooks, and exports
- Commercial redistribution rights
- Enterprise support, reliability commitments, and SLAs

## Quickstart

```bash
git clone https://github.com/zero-intel/zero.git
cd zero
just bootstrap
just demo
just example
```

The first public demo runs in paper mode and requires no exchange private key.

After the first release is published, install the latest CLI binary with:

```bash
curl -fsSL https://raw.githubusercontent.com/zero-intel/zero/main/scripts/install.sh | bash
```

The installer downloads the latest GitHub Release asset for your OS, verifies
`SHA256SUMS`, verifies the GitHub artifact attestation with `gh`, and installs
`zero` to `~/.local/bin`.

To inspect the local paper engine from the operator CLI:

```bash
just paper-api
cd cli
cargo run -p zero -- --api http://127.0.0.1:8765 doctor
cargo run -p zero -- --api http://127.0.0.1:8765 run status
```

See [docs/cli-quickstart.md](docs/cli-quickstart.md) for a redacted terminal
capture of the expected CLI output.

Container path:

```bash
docker compose run --rm zero-paper-example
```

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

For full setup, see [docs/local-development.md](docs/local-development.md).

## Safety

ZERO must be safe by default:

- Paper mode is the default local demo.
- Risk-increasing actions need explicit operator confirmation.
- Risk-reducing actions must not be blocked by friction gates.
- Decisions should be logged with source, timestamp, and confidence.
- No secrets are required for first-run contribution work.

Read [docs/safety-model.md](docs/safety-model.md) before adding execution or risk logic.

## Docs

- [CLI operator terminal](cli/README.md)
- [Architecture](docs/architecture.md)
- [Safety model](docs/safety-model.md)
- [Local development](docs/local-development.md)
- [CLI quickstart capture](docs/cli-quickstart.md)
- [API contract](docs/api.md)
- [Open-core boundary](docs/open-core-boundary.md)
- [ZERO Network](docs/zero-network.md)
- [ZERO Intelligence](docs/zero-intelligence.md)
- [Threat model](docs/threat-model.md)
- [Incident runbooks](docs/incident-runbooks.md)
- [Distribution readiness](docs/distribution.md)
- [Hyperliquid read-only runtime](docs/hyperliquid-readonly.md)
- [Railway paper deployment](docs/railway-deploy.md)
- [Production readiness](docs/production-readiness.md)
- [Release process](docs/release.md)
- [Launch scorecard](docs/launch-scorecard.md)
- [Launch backlog](docs/backlog.md)
- [Launch issue set](docs/launch-issues.md)
- [Roadmap](docs/roadmap.md)

## Contributing

Start with [CONTRIBUTING.md](CONTRIBUTING.md). Good first issues are labeled `good first issue`; larger design work should start as a discussion or proposal.

## License

Apache-2.0. See [LICENSE](LICENSE).
