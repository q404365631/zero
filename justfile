set shell := ["bash", "-uc"]

bootstrap:
    cd engine && python3 -m pip install -e ".[dev]"

demo:
    cd engine && python3 -m zero_engine.demo

example:
    cd examples/paper-trading && python3 run.py

strategy-example:
    cd examples/paper-trading && python3 strategy_demo.py

paper-api:
    cd engine && python3 -m zero_engine.api

paper-api-smoke:
    scripts/paper_api_smoke.sh

package-dry-run:
    scripts/package_dry_run.sh

checksum output *artifacts:
    python3 scripts/write_sha256s.py "{{output}}" {{artifacts}}

engine-lint:
    cd engine && ruff check .

engine-format:
    cd engine && ruff format .

engine-test:
    cd engine && pytest

cli-lint:
    cd cli && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings

cli-test:
    cd cli && cargo test --workspace

docs-check:
    test -f docs/local-development.md
    test -f docs/api.md
    test -f docs/open-core-boundary.md
    test -f docs/release.md
    test -f docs/launch-scorecard.md
    test -f docs/backlog.md
    test -f docs/launch-issues.md
    test -f .github/RELEASE_TEMPLATE.md
    test -f examples/paper-trading/run.py
    test -f examples/paper-trading/strategy_demo.py
    test -f examples/paper-trading/scenario.json
    test -f examples/paper-trading/candles.jsonl
    test -f contracts/paper-api/v2_status.json
    test -f contracts/paper-api/execute_accepted.json
    test -f contracts/paper-api/execute_rejected.json
    test -x scripts/package_dry_run.sh
    test -f Dockerfile
    test -f compose.yaml

container-build:
    docker build -t zero-public:local .

container-demo: container-build
    docker run --rm zero-public:local

container-example: container-build
    docker run --rm zero-public:local python /app/examples/paper-trading/run.py

lint: engine-lint cli-lint docs-check

test: engine-test cli-test

container-smoke:
    docker build -t zero-public:local .
    docker run --rm zero-public:local
    docker run --rm zero-public:local python /app/examples/paper-trading/run.py

ci: lint test paper-api-smoke example strategy-example package-dry-run
