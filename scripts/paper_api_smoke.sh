#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ZERO_PAPER_API_PORT:-8765}"
API="http://127.0.0.1:${PORT}"
LOG="${TMPDIR:-/tmp}/zero-paper-api-smoke.log"
PYTHON_BIN="${PYTHON:-python3}"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "${SERVER_PID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

cd "${ROOT}"

PYTHONPATH="${ROOT}/engine/src${PYTHONPATH:+:${PYTHONPATH}}" \
  "${PYTHON_BIN}" -m zero_engine.api --port "${PORT}" >"${LOG}" 2>&1 &
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

curl -fsS "${API}/v2/status" | "${PYTHON_BIN}" -m json.tool >/dev/null

(
  cd "${ROOT}/cli"
  cargo run -q -p zero -- --api "${API}" doctor >/tmp/zero-paper-api-doctor.txt
  cargo run -q -p zero -- --api "${API}" run status >/tmp/zero-paper-api-status.txt
)

curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"smoke-1"}' \
  "${API}/execute" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is True; assert p["simulated"] is True; assert p["trace_id"].startswith("trace-")'

curl -fsS "${API}/metrics" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.metrics.v1"; assert p["api"]["execute_count"] >= 1'
curl -fsS "${API}/audit/export?limit=5" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.audit.v1"; assert p["decisions"][0]["trace_id"].startswith("trace-")'

(
  cd "${ROOT}/cli"
  cargo run -q -p zero -- --api "${API}" run positions >/tmp/zero-paper-api-positions.txt
)

grep -q "BTC" /tmp/zero-paper-api-positions.txt
