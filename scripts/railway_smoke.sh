#!/usr/bin/env bash
set -euo pipefail

IMAGE="${ZERO_RAILWAY_IMAGE:-zero-public:railway-smoke}"
HOST_PORT="${ZERO_RAILWAY_SMOKE_PORT:-18765}"
CONTAINER_NAME="zero-railway-smoke-${HOST_PORT}"
API="http://127.0.0.1:${HOST_PORT}"
STATE_DIR="$(mktemp -d)"

cleanup() {
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
  rm -rf "${STATE_DIR}"
}
trap cleanup EXIT

start_container() {
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
  docker run -d \
    --name "${CONTAINER_NAME}" \
    -p "${HOST_PORT}:${HOST_PORT}" \
    -v "${STATE_DIR}:/tmp/zero" \
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
}

docker build -t "${IMAGE}" .
start_container

curl -fsS "${API}/health" | python3 -m json.tool >/dev/null
curl -fsS "${API}/market/quote?symbol=BTC" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["source"] == "paper:static"'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"railway-smoke"}' \
  "${API}/execute" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is True; assert p["simulated"] is True; assert p["trace_id"].startswith("trace-")'
curl -fsS \
  -H "content-type: application/json" \
  -H "x-zero-mode: live" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"railway-live-refused"}' \
  "${API}/execute" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is False; assert p["simulated"] is False; assert p["reason"] == "live executor not configured"'
curl -fsS "${API}/journal?limit=1" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["count"] == 1; assert p["decisions"][0]["symbol"] == "BTC"; assert p["decisions"][0]["trace_id"].startswith("trace-")'
curl -fsS "${API}/metrics" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.metrics.v1"; assert p["api"]["execute_count"] >= 2; assert p["api"]["execute_rejected"] >= 1'
curl -fsS "${API}/immune" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.immune.v1"; assert p["risk_increasing_allowed"] is False; assert p["summary"]["risk_blocking"] >= 1'
curl -fsS "${API}/operator/context" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.operator_context.v1"; assert p["handle"] == "local-operator"; assert p["scope"] == "local-private"'
curl -fsS "${API}/live/preflight" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_preflight.v1"; assert p["ready"] is False; assert p["live_mode"] == "refused"'
curl -fsS "${API}/live/cockpit" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_cockpit.v1"; assert p["ready"] is False; assert p["risk_increasing_allowed"] is False; assert p["operator_context"]["handle"] == "local-operator"; assert p["preflight"]["summary"]["failed"] >= 1'
curl -fsS "${API}/live/certification" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_certification.v1"; assert p["mode"] == "dry_run"; assert p["passed"] is True; assert p["summary"]["orders_placed_live"] == 0'
curl -fsS "${API}/network/profile" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.network.profile.v1"; assert p["profile"]["publish_enabled"] is False; assert p["verification"]["deployment_claim_hash"] == p["deployment_claim"]["claim_hash"]; assert p["verification"]["deployment_heartbeat_hash"] == p["deployment_heartbeat"]["heartbeat_hash"]; assert "railway-smoke" not in body; assert "trace-" not in body'
curl -fsS "${API}/deployment/claim" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.deployment.claim.v1"; assert p["claim_hash"].startswith("sha256:"); assert p["signature"]["status"] == "unsigned_local"; assert "railway-smoke" not in body; assert "trace-" not in body'
curl -fsS "${API}/deployment/heartbeat" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.deployment.heartbeat.v1"; assert p["heartbeat_hash"].startswith("sha256:"); assert p["deployment_claim_hash"].startswith("sha256:"); assert p["signature"]["status"] == "unsigned_local"; assert p["signature"]["signed_heartbeat_hash"] == p["heartbeat_hash"]; assert p["liveness"]["status"] == "paper_only"; assert "railway-smoke" not in body; assert "trace-" not in body'
curl -fsS "${API}/network/leaderboard" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.network.leaderboard.v1"; assert len(p["rows"]) == 1; assert p["rows"][0]["proof_hash"].startswith("sha256:"); assert p["rows"][0]["deployment_claim_hash"].startswith("sha256:"); assert p["rows"][0]["deployment_heartbeat_hash"].startswith("sha256:")'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"consent":false}' \
  "${API}/network/publish" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "explicit consent required"'
curl -fsS "${API}/intelligence/snapshot" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.snapshot.v1"; assert p["access"]["class"] == "public_delayed"; assert p["source"]["proof_hash"].startswith("sha256:"); assert p["source"]["deployment_claim_hash"].startswith("sha256:"); assert p["source"]["deployment_heartbeat_hash"].startswith("sha256:"); assert "railway-smoke" not in body; assert "trace-" not in body'
curl -fsS "${API}/intelligence/catalog" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.intelligence.catalog.v1"; assert p["public"]["model_gateway_status"]["schema_version"] == "zero.model_gateway.status.v1"; assert "local runtime use" in p["commercial"]["not_metered_by"]; assert "freshness" in p["commercial"]["metered_by"]'
curl -fsS "${API}/intelligence/model-gateway" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.model_gateway.status.v1"; assert p["mode"] == "fail_closed"; assert p["routing"]["structured_output"] is None; assert p["privacy"]["prompts_included"] is False; assert "sk-" not in body; assert "private_key" not in body; assert "railway-smoke" not in body; assert "trace-" not in body'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"consent":false}' \
  "${API}/intelligence/export" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "explicit consent required"'
curl -fsS \
  -H "content-type: application/json" \
  -d '{}' \
  "${API}/live/kill" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "live executor not configured"; assert p["operator_context"]["handle"] == "local-operator"'

docker rm -f "${CONTAINER_NAME}" >/dev/null
start_container

curl -fsS "${API}/health" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["recovery"]["status"] == "recovered"; assert p["recovery"]["current_positions"] == 1'
curl -fsS "${API}/audit/export?limit=5" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.audit.v1"; assert p["operator_context"]["handle"] == "local-operator"; assert p["deployment_claim"]["claim_hash"].startswith("sha256:"); assert p["deployment_heartbeat"]["heartbeat_hash"].startswith("sha256:"); assert p["deployment_heartbeat"]["deployment_claim_hash"] == p["deployment_claim"]["claim_hash"]; assert p["decisions"][0]["trace_id"].startswith("trace-")'
curl -fsS "${API}/positions" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["count"] == 1; assert p["positions"][0]["symbol"] == "BTC"'
curl -fsS \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"railway-smoke"}' \
  "${API}/execute" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is True; assert p["fill_id"] == "paper-railway-"'
curl -fsS "${API}/positions" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["count"] == 1; assert p["positions"][0]["size"] == 0.01'
