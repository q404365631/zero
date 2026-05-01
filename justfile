set shell := ["bash", "-uc"]

bootstrap:
    cd engine && python3 -m pip install -e ".[dev]"

demo:
    cd engine && python3 -m zero_engine.demo

lint:
    cd engine && ruff check .

test:
    cd engine && pytest

ci: lint test
