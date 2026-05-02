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

strategy-runner-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/strategy-runner/run.py

market-data-adapter-example:
    PYTHONPATH="$PWD/engine/src:$PWD/examples/market-data-adapter" python3 examples/market-data-adapter/run.py

network-leaderboard-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-leaderboard/build.py

network-profile-page-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-profile-page/build.py

network-leaderboard-page-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-leaderboard-page/build.py

network-index-page-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-index-page/build.py

runtime-loop-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/runtime-loop/run.py

network-pages-smoke:
    scripts/network_pages_smoke.py

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

release-rehearsal:
    scripts/release_rehearsal.sh

release-verify dir:
    scripts/release_verify.py "{{dir}}"

railway-smoke:
    scripts/railway_smoke.sh

deployment-evidence url:
    scripts/deployment_evidence.sh "{{url}}"

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
    test -f AGENTS.md
    test -f docs/local-development.md
    test -f docs/first-10-minutes.md
    test -f docs/demo-terminal.md
    test -f docs/cli-quickstart.md
    test -f docs/api.md
    test -f docs/api-compatibility.md
    test -f docs/runtime-bus.md
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
    test -f docs/live-certification.md
    test -f docs/live-cockpit.md
    test -f docs/operator-context.md
    test -f docs/deployment-identity.md
    test -f docs/model-gateway.md
    test -f docs/operator-isolation.md
    test -f docs/immune-system.md
    test -f docs/production-readiness.md
    test -f docs/autonomous-os-plan.md
    test -f docs/agentic-contribution.md
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
    test -f examples/strategy-runner/README.md
    test -f examples/strategy-runner/close-strength.yaml
    test -x examples/strategy-runner/run.py
    test -f examples/market-data-adapter/README.md
    test -f examples/market-data-adapter/adapter.py
    test -f examples/market-data-adapter/run.py
    test -f examples/runtime-loop/README.md
    test -x examples/runtime-loop/run.py
    test -f examples/network-leaderboard/README.md
    test -f examples/network-leaderboard/build.py
    test -f examples/network-leaderboard/profiles.jsonl
    test -f examples/network-profile-page/README.md
    test -f examples/network-profile-page/build.py
    test -f examples/network-leaderboard-page/README.md
    test -f examples/network-leaderboard-page/build.py
    test -f examples/network-index-page/README.md
    test -f examples/network-index-page/build.py
    test -f contracts/paper-api/v2_status.json
    test -f contracts/paper-api/execute_accepted.json
    test -f contracts/paper-api/execute_rejected.json
    test -f contracts/deployment/claim.json
    test -f contracts/deployment/heartbeat.json
    test -f contracts/network/profile.json
    test -f contracts/network/leaderboard.json
    test -f contracts/network/ingestion.json
    test -f contracts/network/profile.html
    test -f contracts/network/leaderboard.html
    test -f contracts/network/index.html
    test -f contracts/intelligence/snapshot.json
    test -f contracts/intelligence/catalog.json
    test -f contracts/intelligence/commercial.json
    test -f contracts/intelligence/model_gateway.json
    test -f contracts/intelligence/model_gateway_health.json
    test -f contracts/intelligence/model_gateway_audit.json
    test -f openapi/zero-paper-api.v1.yaml
    test -x scripts/assemble_release_assets.sh
    test -x scripts/install.sh
    test -x scripts/demo_capture.sh
    test -x scripts/openapi_contract_check.py
    test -x scripts/network_pages_smoke.py
    test -x scripts/package_dry_run.sh
    test -x scripts/release_verify.py
    test -x scripts/release_rehearsal.sh
    test -x scripts/hardening_gate.sh
    test -x scripts/railway_start.sh
    test -x scripts/railway_doctor.py
    test -x scripts/deployment_evidence.py
    test -x scripts/deployment_evidence.sh
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

ci: lint test paper-api-smoke example strategy-example strategy-plugin-example strategy-runner-example market-data-adapter-example runtime-loop-example network-leaderboard-example network-profile-page-example network-leaderboard-page-example network-index-page-example network-pages-smoke package-dry-run release-rehearsal
