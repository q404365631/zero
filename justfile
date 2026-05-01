set shell := ["bash", "-uc"]

bootstrap:
    cd engine && python3 -m pip install -e ".[dev]"

demo:
    cd engine && python3 -m zero_engine.demo

example:
    PYTHONPATH="$PWD/engine/src" python3 examples/paper-trading/run.py

strategy-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/paper-trading/strategy_demo.py

strategy-plugin-example:
    PYTHONPATH="$PWD/engine/src:$PWD/examples/strategy-plugin" python3 examples/strategy-plugin/run.py

market-data-adapter-example:
    PYTHONPATH="$PWD/engine/src:$PWD/examples/market-data-adapter" python3 examples/market-data-adapter/run.py

paper-api:
    cd engine && python3 -m zero_engine.api

paper-api-smoke:
    scripts/paper_api_smoke.sh

demo-capture:
    scripts/demo_capture.sh

hardening-gate:
    scripts/hardening_gate.sh

package-dry-run:
    scripts/package_dry_run.sh

railway-smoke:
    scripts/railway_smoke.sh

checksum output *artifacts:
    python3 scripts/write_sha256s.py "{{output}}" {{artifacts}}

engine-lint:
    cd engine && ruff check .

engine-format:
    cd engine && ruff format .

engine-test:
    cd engine && PYTHONPATH="$PWD/src" pytest

cli-lint:
    cd cli && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings

cli-test:
    cd cli && cargo test --workspace

docs-check:
    test -f docs/local-development.md
    test -f docs/first-10-minutes.md
    test -f docs/demo-terminal.md
    test -f docs/cli-quickstart.md
    test -f docs/api.md
    test -f docs/api-compatibility.md
    test -f docs/strategy-plugins.md
    test -f docs/market-data-adapters.md
    test -f docs/positioning.md
    test -f docs/open-core-boundary.md
    test -f docs/zero-network.md
    test -f docs/zero-intelligence.md
    test -f docs/threat-model.md
    test -f docs/incident-runbooks.md
    test -f docs/distribution.md
    test -f docs/hyperliquid-readonly.md
    test -f docs/production-readiness.md
    test -f docs/release.md
    test -f docs/releases/v0.1.1.md
    test -f docs/launch-scorecard.md
    test -f docs/backlog.md
    test -f docs/launch-issues.md
    test -f .github/RELEASE_TEMPLATE.md
    test -f examples/paper-trading/run.py
    test -f examples/paper-trading/strategy_demo.py
    test -f examples/paper-trading/scenario.json
    test -f examples/paper-trading/candles.jsonl
    test -f examples/strategy-plugin/README.md
    test -f examples/strategy-plugin/plugin.py
    test -f examples/strategy-plugin/run.py
    test -f examples/market-data-adapter/README.md
    test -f examples/market-data-adapter/adapter.py
    test -f examples/market-data-adapter/run.py
    test -f contracts/paper-api/v2_status.json
    test -f contracts/paper-api/execute_accepted.json
    test -f contracts/paper-api/execute_rejected.json
    test -f contracts/intelligence/snapshot.json
    test -f contracts/intelligence/catalog.json
    test -f openapi/zero-paper-api.v1.yaml
    test -x scripts/assemble_release_assets.sh
    test -x scripts/install.sh
    test -x scripts/demo_capture.sh
    test -x scripts/openapi_contract_check.py
    test -x scripts/package_dry_run.sh
    test -x scripts/hardening_gate.sh
    test -x scripts/railway_start.sh
    test -x scripts/railway_smoke.sh
    test -f Dockerfile
    test -f compose.yaml
    test -f railway.toml
    test -f docs/railway-deploy.md
    python3 scripts/openapi_contract_check.py

container-build:
    docker build -t zero-public:local .

container-demo: container-build
    docker run --rm zero-public:local

container-example: container-build
    docker run --rm zero-public:local python /app/examples/paper-trading/run.py

lint: engine-lint cli-lint docs-check hardening-gate

test: engine-test cli-test

container-smoke:
    docker build -t zero-public:local .
    docker run --rm zero-public:local
    docker run --rm zero-public:local python /app/examples/paper-trading/run.py

ci: lint test paper-api-smoke example strategy-example strategy-plugin-example market-data-adapter-example package-dry-run
