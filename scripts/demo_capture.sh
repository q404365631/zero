#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ZERO_DEMO_PORT:-8765}"
API="http://127.0.0.1:${PORT}"
PYTHON_BIN="${PYTHON:-python3}"
ZERO_BIN="${ZERO_BIN:-}"
LOG="${TMPDIR:-/tmp}/zero-demo-paper-api.log"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "${SERVER_PID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

redact() {
  sed -E \
    -e "s#${HOME}#~#g" \
    -e 's#/private/var/folders/[^ ]+#<tmp>#g' \
    -e 's#/var/folders/[^ ]+#<tmp>#g' \
    -e 's#/tmp/[^ ]+#<tmp>#g'
}

run_zero() {
  if [[ -n "${ZERO_BIN}" ]]; then
    "${ZERO_BIN}" --api "${API}" "$@"
  else
    (cd "${ROOT}/cli" && cargo run -q -p zero -- --api "${API}" "$@")
  fi
}

section() {
  printf '\n## %s\n\n' "$1"
}

cmd() {
  printf '$ %s\n' "$*"
}

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

cat <<'INTRO'
# ZERO Paper Demo Capture

This capture is generated from a local paper runtime. It uses no exchange
credentials and never places live orders.
INTRO

section "Version Probe"
cmd "curl -fsS ${API}/ | python3 -m json.tool"
curl -fsS "${API}/" | "${PYTHON_BIN}" -m json.tool

section "Doctor"
cmd "zero --api ${API} doctor"
run_zero doctor | redact

section "Status"
cmd "zero --api ${API} run status"
run_zero run status | redact

section "Risk"
cmd "zero --api ${API} run risk"
run_zero run risk | redact

section "Paper Execute"
cmd "curl -fsS -H 'content-type: application/json' -d '{...}' ${API}/execute | python3 -m json.tool"
curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"demo-capture"}' \
  "${API}/execute" \
  | "${PYTHON_BIN}" -m json.tool

section "Positions"
cmd "zero --api ${API} run positions"
run_zero run positions | redact

section "Public Proof Packet"
cmd "curl -fsS ${API}/network/profile | python3 -m json.tool"
curl -fsS "${API}/network/profile" | "${PYTHON_BIN}" -m json.tool

section "Delayed Intelligence Snapshot"
cmd "curl -fsS ${API}/intelligence/snapshot | python3 -m json.tool"
curl -fsS "${API}/intelligence/snapshot" | "${PYTHON_BIN}" -m json.tool

section "Live Preflight"
cmd "curl -fsS ${API}/live/preflight | python3 -m json.tool"
curl -fsS "${API}/live/preflight" | "${PYTHON_BIN}" -m json.tool
