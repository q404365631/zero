#!/usr/bin/env bash
set -euo pipefail

IMAGE="${ZERO_RAILWAY_IMAGE:-zero-public:railway-smoke}"
HOST_PORT="${ZERO_RAILWAY_SMOKE_PORT:-18765}"
CONTAINER_NAME="zero-railway-smoke-${HOST_PORT}"
API="http://127.0.0.1:${HOST_PORT}"

cleanup() {
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker build -t "${IMAGE}" .
cleanup
docker run -d \
  --name "${CONTAINER_NAME}" \
  -p "${HOST_PORT}:${HOST_PORT}" \
  -e PORT="${HOST_PORT}" \
  -e ZERO_JOURNAL_PATH=/tmp/zero/decisions.jsonl \
  -e ZERO_HYPERLIQUID_LIVE_PRICES=false \
  "${IMAGE}" \
  /app/scripts/railway_start.sh >/dev/null

READY=0
for _ in {1..80}; do
  if curl -fsS "${API}/health" >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 0.25
done

if [[ "${READY}" != "1" ]]; then
  echo "Railway smoke service did not become ready at ${API}" >&2
  docker logs "${CONTAINER_NAME}" >&2 || true
  exit 1
fi

curl -fsS "${API}/health" | python3 -m json.tool >/dev/null
curl -fsS "${API}/market/quote?symbol=BTC" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["source"] == "paper:static"'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"railway-smoke"}' \
  "${API}/execute" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is True; assert p["simulated"] is True'
curl -fsS "${API}/journal?limit=1" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["count"] == 1; assert p["decisions"][0]["symbol"] == "BTC"'
