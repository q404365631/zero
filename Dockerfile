# syntax=docker/dockerfile:1

FROM python:3.12-slim AS paper-engine

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    ZERO_MODE=paper

WORKDIR /app

COPY engine/pyproject.toml engine/README.md /app/engine/
COPY engine/src /app/engine/src
COPY examples /app/examples

RUN python -m pip install --no-cache-dir --upgrade pip \
    && python -m pip install --no-cache-dir /app/engine

CMD ["zero-paper-demo"]
