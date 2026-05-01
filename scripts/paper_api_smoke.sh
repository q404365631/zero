#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ZERO_PAPER_API_PORT:-8765}"
API="http://127.0.0.1:${PORT}"
LOG="${TMPDIR:-/tmp}/zero-paper-api-smoke.log"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "${SERVER_PID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

cd "${ROOT}"

python -m zero_engine.api --port "${PORT}" >"${LOG}" 2>&1 &
SERVER_PID="$!"

READY=0
for _ in {1..50}; do
  if curl -fsS "${API}/health" >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 0.1
done

if [[ "${READY}" != "1" ]]; then
  echo "paper API did not become ready at ${API}" >&2
  cat "${LOG}" >&2 || true
  exit 1
fi

curl -fsS "${API}/v2/status" | python -m json.tool >/dev/null

(
  cd "${ROOT}/cli"
  cargo run -q -p zero -- --api "${API}" doctor >/tmp/zero-paper-api-doctor.txt
  cargo run -q -p zero -- --api "${API}" run status >/tmp/zero-paper-api-status.txt
)

curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"smoke-1"}' \
  "${API}/execute" \
  | python -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is True; assert p["simulated"] is True'

(
  cd "${ROOT}/cli"
  cargo run -q -p zero -- --api "${API}" run positions >/tmp/zero-paper-api-positions.txt
)

grep -q "BTC" /tmp/zero-paper-api-positions.txt
