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
curl -fsS \
  -H "content-type: application/json" \
  -H "x-zero-mode: live" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"smoke-live-refused"}' \
  "${API}/execute" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is False; assert p["simulated"] is False; assert p["reason"] == "live executor not configured"'

curl -fsS "${API}/metrics" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.metrics.v1"; assert p["api"]["execute_count"] >= 1'
curl -fsS "${API}/immune" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.immune.v1"; assert p["risk_increasing_allowed"] is False; assert p["summary"]["risk_blocking"] >= 1'
curl -fsS "${API}/operator/context" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.operator_context.v1"; assert p["handle"] == "local-operator"; assert p["scope"] == "local-private"'
curl -fsS "${API}/audit/export?limit=5" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.audit.v1"; assert p["operator_context"]["handle"] == "local-operator"; assert p["deployment_claim"]["claim_hash"].startswith("sha256:"); assert p["deployment_heartbeat"]["heartbeat_hash"].startswith("sha256:"); assert p["deployment_heartbeat"]["deployment_claim_hash"] == p["deployment_claim"]["claim_hash"]; assert p["decisions"][0]["trace_id"].startswith("trace-")'
curl -fsS "${API}/deployment/claim" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.deployment.claim.v1"; assert p["claim_hash"].startswith("sha256:"); assert p["signature"]["status"] == "unsigned_local"; assert p["signature"]["signed_claim_hash"] == p["claim_hash"]; assert "smoke-1" not in body; assert "trace-" not in body'
curl -fsS "${API}/deployment/heartbeat" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.deployment.heartbeat.v1"; assert p["heartbeat_hash"].startswith("sha256:"); assert p["deployment_claim_hash"].startswith("sha256:"); assert p["signature"]["status"] == "unsigned_local"; assert p["signature"]["signed_heartbeat_hash"] == p["heartbeat_hash"]; assert p["liveness"]["status"] == "paper_only"; assert p["liveness"]["live_executor_configured"] is False; assert "smoke-1" not in body; assert "trace-" not in body'
curl -fsS "${API}/network/profile" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.network.profile.v1"; assert p["profile"]["publish_enabled"] is False; assert p["metrics"]["decisions"] >= 1; assert p["verification"]["deployment_claim_hash"] == p["deployment_claim"]["claim_hash"]; assert p["verification"]["deployment_heartbeat_hash"] == p["deployment_heartbeat"]["heartbeat_hash"]; assert "smoke-1" not in body; assert "trace-" not in body'
curl -fsS "${API}/network/leaderboard" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.network.leaderboard.v1"; assert len(p["rows"]) == 1; assert p["rows"][0]["proof_hash"].startswith("sha256:"); assert p["rows"][0]["deployment_claim_hash"].startswith("sha256:"); assert p["rows"][0]["deployment_heartbeat_hash"].startswith("sha256:")'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"consent":false}' \
  "${API}/network/publish" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "explicit consent required"'
curl -fsS "${API}/intelligence/snapshot" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.snapshot.v1"; assert p["access"]["class"] == "public_delayed"; assert p["source"]["proof_hash"].startswith("sha256:"); assert p["source"]["deployment_claim_hash"].startswith("sha256:"); assert p["source"]["deployment_heartbeat_hash"].startswith("sha256:"); assert "smoke-1" not in body; assert "trace-" not in body'
curl -fsS "${API}/intelligence/catalog" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.intelligence.catalog.v1"; assert "local runtime use" in p["commercial"]["not_metered_by"]; assert "freshness" in p["commercial"]["metered_by"]'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"consent":false}' \
  "${API}/intelligence/export" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "explicit consent required"'
curl -fsS "${API}/live/preflight" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_preflight.v1"; assert p["ready"] is False; assert p["live_mode"] == "refused"; assert "private" not in json.dumps(p).lower() or "never commit" in json.dumps(p).lower()'
curl -fsS "${API}/live/cockpit" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_cockpit.v1"; assert p["ready"] is False; assert p["risk_increasing_allowed"] is False; assert p["operator_context"]["handle"] == "local-operator"; assert p["preflight"]["summary"]["failed"] >= 1; assert "/kill" in p["operator_actions"]["risk_reducing"]'
curl -fsS "${API}/live/certification" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_certification.v1"; assert p["mode"] == "dry_run"; assert p["passed"] is True; assert p["summary"]["orders_placed_live"] == 0'
curl -fsS \
  -H "content-type: application/json" \
  -d '{}' \
  "${API}/live/kill" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "live executor not configured"; assert p["operator_context"]["handle"] == "local-operator"'

(
  cd "${ROOT}/cli"
  cargo run -q -p zero -- --api "${API}" run positions >/tmp/zero-paper-api-positions.txt
  cargo run -q -p zero -- --api "${API}" run live-cockpit >/tmp/zero-paper-api-live-cockpit.txt
  cargo run -q -p zero -- --api "${API}" run immune >/tmp/zero-paper-api-immune.txt
)

grep -q "BTC" /tmp/zero-paper-api-positions.txt
grep -q "live-cockpit:" /tmp/zero-paper-api-live-cockpit.txt
grep -q "immune:" /tmp/zero-paper-api-immune.txt
