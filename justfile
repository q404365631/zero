set shell := ["bash", "-uc"]

bootstrap:
    cd engine && python3 -m pip install -e ".[dev]"

demo:
    cd engine && python3 -m zero_engine.demo

example:
    cd examples/paper-trading && python3 run.py

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
    test -f docs/backlog.md
    test -f examples/paper-trading/run.py
    test -f examples/paper-trading/scenario.json
    test -f examples/paper-trading/candles.jsonl
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

ci: lint test example
