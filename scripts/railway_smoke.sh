#!/usr/bin/env bash
set -euo pipefail

IMAGE="${ZERO_RAILWAY_IMAGE:-zero-public:railway-smoke}"
HOST_PORT="${ZERO_RAILWAY_SMOKE_PORT:-18765}"
CONTAINER_NAME="zero-railway-smoke-${HOST_PORT}"
API="http://127.0.0.1:${HOST_PORT}"
STATE_DIR="$(mktemp -d)"

cleanup() {
  status=$?
  if [[ "${status}" != "0" ]]; then
    echo "Railway smoke failed; container logs follow." >&2
    docker logs "${CONTAINER_NAME}" >&2 || true
  fi
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
  rm -rf "${STATE_DIR}"
}
trap cleanup EXIT

curl_retry() {
  local attempt
  local output
  local error_file
  error_file="$(mktemp)"
  for attempt in {1..40}; do
    if output="$(curl -fsS "$@" 2>"${error_file}")"; then
      rm -f "${error_file}"
      printf '%s' "${output}"
      return 0
    fi
    sleep 0.25
  done
  echo "curl failed after retries: curl -fsS $*" >&2
  cat "${error_file}" >&2 || true
  rm -f "${error_file}"
  return 1
}

start_container() {
  docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
  docker run -d \
    --name "${CONTAINER_NAME}" \
    -p "${HOST_PORT}:${HOST_PORT}" \
    -v "${STATE_DIR}:/tmp/zero" \
    -e PORT="${HOST_PORT}" \
    -e ZERO_JOURNAL_PATH=/tmp/zero/decisions.jsonl \
    -e ZERO_HYPERLIQUID_LIVE_PRICES=false \
    -e ZERO_INTELLIGENCE_API_TOKEN=railway-intelligence-token \
    -e ZERO_INTELLIGENCE_API_PLAN=team_fund \
    -e ZERO_INTELLIGENCE_API_ACCOUNT_ID=acct_railway \
    -e ZERO_INTELLIGENCE_WEBHOOK_SIGNING_KEY=railway-webhook-signing-key \
    "${IMAGE}" \
    /app/scripts/railway_start.sh >/dev/null

  READY=0
  for _ in {1..80}; do
    if curl -fsS "${API}/health" 2>/dev/null \
      | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["status"] == "ok"' >/dev/null 2>&1; then
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
  sleep 0.5
}

docker build -t "${IMAGE}" .
start_container

curl_retry "${API}/health" | python3 -m json.tool >/dev/null
curl_retry "${API}/market/quote?symbol=BTC" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["source"] == "paper:static"'
curl_retry \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"railway-smoke"}' \
  "${API}/execute" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is True; assert p["simulated"] is True; assert p["trace_id"].startswith("trace-")'
curl_retry \
  -H "content-type: application/json" \
  -H "x-zero-mode: live" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"railway-live-refused"}' \
  "${API}/execute" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is False; assert p["simulated"] is False; assert p["reason"] == "live executor not configured"'
curl_retry "${API}/journal?limit=1" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["count"] == 1; assert p["decisions"][0]["symbol"] == "BTC"; assert p["decisions"][0]["trace_id"].startswith("trace-")'
curl_retry "${API}/metrics" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.metrics.v1"; assert p["api"]["execute_count"] >= 2; assert p["api"]["execute_rejected"] >= 1'
curl_retry "${API}/immune" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.immune.v1"; assert p["risk_increasing_allowed"] is False; assert p["summary"]["risk_blocking"] >= 1'
curl_retry "${API}/operator/context" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.operator_context.v1"; assert p["handle"] == "local-operator"; assert p["scope"] == "local-private"'
curl_retry "${API}/live/preflight" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_preflight.v1"; assert p["ready"] is False; assert p["live_mode"] == "refused"'
curl_retry "${API}/live/cockpit" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_cockpit.v1"; assert p["ready"] is False; assert p["risk_increasing_allowed"] is False; assert p["operator_context"]["handle"] == "local-operator"; assert p["preflight"]["summary"]["failed"] >= 1'
curl_retry "${API}/live/certification" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.live_certification.v1"; assert p["mode"] == "dry_run"; assert p["passed"] is True; assert p["summary"]["orders_placed_live"] == 0'
curl_retry "${API}/network/profile" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.network.profile.v1"; assert p["profile"]["publish_enabled"] is False; assert p["verification"]["deployment_claim_hash"] == p["deployment_claim"]["claim_hash"]; assert p["verification"]["deployment_heartbeat_hash"] == p["deployment_heartbeat"]["heartbeat_hash"]; assert "railway-smoke" not in body; assert "trace-" not in body'
curl_retry "${API}/deployment/claim" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.deployment.claim.v1"; assert p["claim_hash"].startswith("sha256:"); assert p["signature"]["status"] == "unsigned_local"; assert "railway-smoke" not in body; assert "trace-" not in body'
curl_retry "${API}/deployment/heartbeat" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.deployment.heartbeat.v1"; assert p["heartbeat_hash"].startswith("sha256:"); assert p["deployment_claim_hash"].startswith("sha256:"); assert p["signature"]["status"] == "unsigned_local"; assert p["signature"]["signed_heartbeat_hash"] == p["heartbeat_hash"]; assert p["liveness"]["status"] == "paper_only"; assert "railway-smoke" not in body; assert "trace-" not in body'
IDENTITY_AUDIT="$(mktemp)"
IDENTITY_CLAIM="$(mktemp)"
IDENTITY_HEARTBEAT="$(mktemp)"
IDENTITY_PRIVATE_KEY="$(mktemp)"
IDENTITY_PUBLIC_KEY="$(mktemp)"
IDENTITY_BUNDLE_DIR="$(mktemp -d)"
curl_retry "${API}/audit/export?limit=1" >"${IDENTITY_AUDIT}"
python3 - "${IDENTITY_AUDIT}" "${IDENTITY_CLAIM}" "${IDENTITY_HEARTBEAT}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    audit = json.load(fh)
with open(sys.argv[2], "w", encoding="utf-8") as fh:
    json.dump(audit["deployment_claim"], fh)
with open(sys.argv[3], "w", encoding="utf-8") as fh:
    json.dump(audit["deployment_heartbeat"], fh)
PY
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "${IDENTITY_PRIVATE_KEY}" >/dev/null 2>&1
openssl pkey -in "${IDENTITY_PRIVATE_KEY}" -pubout -out "${IDENTITY_PUBLIC_KEY}" >/dev/null 2>&1
python3 scripts/deployment_identity_evidence.py create "${IDENTITY_CLAIM}" "${IDENTITY_HEARTBEAT}" \
  --private-key "${IDENTITY_PRIVATE_KEY}" \
  --public-key "${IDENTITY_PUBLIC_KEY}" \
  --signer ci-railway-smoke \
  --output "${IDENTITY_BUNDLE_DIR}" >/tmp/zero-railway-deployment-identity-evidence.txt
python3 scripts/deployment_identity_evidence.py verify "${IDENTITY_BUNDLE_DIR}" \
  --require-signature \
  --forbid-token railway-smoke >/tmp/zero-railway-deployment-identity-verify.txt
python3 -c 'import json,pathlib,sys; d=pathlib.Path(sys.argv[1]); b=json.loads((d/"identity_bundle.json").read_text(encoding="utf-8")); s=json.loads((d/"IDENTITY_SIGNATURE.json").read_text(encoding="utf-8")); assert b["schema_version"] == "zero.deployment_identity_evidence.v1"; assert b["ok"] is True; assert b["heartbeat"]["deployment_claim_hash"] == b["claim"]["claim_hash"]; assert s["schema_version"] == "zero.deployment_identity_signature.v1"; assert s["key_material_included"] is False; assert "PRIVATE KEY" not in json.dumps(s); assert (d/"SHA256SUMS").is_file()' \
  "${IDENTITY_BUNDLE_DIR}"
rm -f "${IDENTITY_AUDIT}" "${IDENTITY_CLAIM}" "${IDENTITY_HEARTBEAT}" "${IDENTITY_PRIVATE_KEY}" "${IDENTITY_PUBLIC_KEY}"
curl_retry "${API}/network/leaderboard" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.network.leaderboard.v1"; assert len(p["rows"]) == 1; assert p["rows"][0]["proof_hash"].startswith("sha256:"); assert p["rows"][0]["deployment_claim_hash"].startswith("sha256:"); assert p["rows"][0]["deployment_heartbeat_hash"].startswith("sha256:")'
PROFILE_FILE="$(mktemp)"
curl_retry "${API}/network/profile" >"${PROFILE_FILE}"
python3 - "${API}" "${PROFILE_FILE}" <<'PY'
import json
import sys
import urllib.request

api, profile_path = sys.argv[1], sys.argv[2]
with open(profile_path, encoding="utf-8") as fh:
    profile = json.load(fh)
profile["profile"]["publish_enabled"] = True
request = urllib.request.Request(
    f"{api}/network/ingest",
    data=json.dumps({"profiles": [profile]}).encode("utf-8"),
    headers={"content-type": "application/json"},
    method="POST",
)
with urllib.request.urlopen(request) as response:
    packet = json.load(response)
body = json.dumps(packet)
assert packet["schema_version"] == "zero.network.ingestion.v1"
assert packet["summary"]["accepted"] == 1
assert packet["leaderboard"]["row_count"] == 1
assert packet["records"][0]["decision"] == "accepted"
assert "railway-smoke" not in body
assert "trace-" not in body
PY
rm -f "${PROFILE_FILE}"
curl_retry \
  -H "content-type: application/json" \
  -d '{"consent":false}' \
  "${API}/network/publish" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "explicit consent required"'
curl_retry "${API}/intelligence/snapshot" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.snapshot.v1"; assert p["access"]["class"] == "public_delayed"; assert p["source"]["proof_hash"].startswith("sha256:"); assert p["source"]["deployment_claim_hash"].startswith("sha256:"); assert p["source"]["deployment_heartbeat_hash"].startswith("sha256:"); assert "railway-smoke" not in body; assert "trace-" not in body'
curl_retry "${API}/intelligence/catalog" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.intelligence.catalog.v1"; assert p["public"]["model_gateway_status"]["schema_version"] == "zero.model_gateway.status.v1"; assert p["hosted_api_contract"]["schema_version"] == "zero.intelligence.commercial.v1"; assert "local runtime use" in p["commercial"]["not_metered_by"]; assert "freshness" in p["commercial"]["metered_by"]'
curl_retry "${API}/intelligence/commercial" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.commercial.v1"; assert p["auth"]["runtime_required"] is False; assert p["plans"][0]["id"] == "free"; assert p["plans"][-1]["id"] == "enterprise"; assert "x-zero-ratelimit-policy" in p["rate_limits"]["headers"]; assert p["privacy"]["exchange_credentials_collected"] is False; assert "railway-smoke" not in body; assert "trace-" not in body'
HEADER_FILE="$(mktemp)"
curl_retry -D "${HEADER_FILE}" "${API}/v1/intelligence/snapshots" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.hosted.snapshots.v1"; assert p["account"]["plan"] == "free"; assert p["access"]["freshness"] == "delayed"; assert p["usage"]["billable"] is False; assert "railway-intelligence-token" not in body; assert "trace-" not in body'
grep -qi '^x-zero-ratelimit-policy: free;w=3600' "${HEADER_FILE}"
rm -f "${HEADER_FILE}"
python3 - "${API}" <<'PY'
import json
import urllib.error
import urllib.request
import sys

api = sys.argv[1]
try:
    urllib.request.urlopen(f"{api}/v1/intelligence/history", timeout=5)
except urllib.error.HTTPError as exc:
    assert exc.code == 401
    packet = json.loads(exc.read().decode("utf-8"))
    assert packet["schema_version"] == "zero.intelligence.hosted_error.v1"
    assert packet["error"] == "missing_or_invalid_token"
    assert "railway-intelligence-token" not in json.dumps(packet)
else:
    raise AssertionError("history endpoint must require a bearer token")
PY
curl_retry \
  -H "authorization: Bearer railway-intelligence-token" \
  "${API}/v1/intelligence/history?limit=10" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.hosted.history.v1"; assert p["account"]["id"] == "acct_railway"; assert p["usage"]["name"] == "history.query"; assert p["storage"]["status"] == "reference_current_runtime_only"; assert "railway-intelligence-token" not in body'
curl_retry \
  -H "content-type: application/json" \
  -H "authorization: Bearer railway-intelligence-token" \
  -d '{"url":"https://example.com/zero","event_types":["snapshot.accepted"]}' \
  "${API}/v1/intelligence/webhooks" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.hosted.webhook_subscription.v1"; assert p["signing"]["fixture_headers"]["x-zero-signature"].startswith("v1="); assert p["signing"]["key_material_included"] is False; assert "railway-intelligence-token" not in body; assert "railway-webhook-signing-key" not in body'
curl_retry "${API}/intelligence/model-gateway" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.model_gateway.status.v1"; assert p["mode"] == "fail_closed"; assert p["routing"]["structured_output"] is None; assert p["privacy"]["prompts_included"] is False; assert "sk-" not in body; assert "private_key" not in body; assert "railway-smoke" not in body; assert "trace-" not in body'
curl_retry "${API}/intelligence/model-gateway/health" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.model_gateway.health.v1"; assert p["status"] == "failed_closed"; assert p["network_probe"]["requested"] is False; assert p["safety"]["network_probe_requires_explicit_query"] is True; assert "sk-" not in body; assert "private_key" not in body; assert "railway-smoke" not in body; assert "trace-" not in body'
curl_retry "${API}/intelligence/model-gateway/audit" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.model_gateway.audit.v1"; assert p["health"]["schema_version"] == "zero.model_gateway.health.v1"; assert p["controls"]["advisory_only"] is True; assert p["privacy"]["provider_request_ids_included"] is False; assert "sk-" not in body; assert "private_key" not in body; assert "railway-smoke" not in body; assert "trace-" not in body'
curl_retry \
  -H "content-type: application/json" \
  -d '{"consent":false}' \
  "${API}/intelligence/export" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "explicit consent required"'
curl_retry \
  -H "content-type: application/json" \
  -d '{}' \
  "${API}/live/kill" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "live executor not configured"; assert p["operator_context"]["handle"] == "local-operator"'

python3 scripts/railway_doctor.py "${API}" \
  --token railway-intelligence-token \
  --expect-paper \
  --json >/tmp/zero-railway-doctor.json
python3 -c 'import json,sys; p=json.load(open(sys.argv[1], encoding="utf-8")); assert p["schema_version"] == "zero.railway_doctor.v1"; assert p["summary"]["fail"] == 0' \
  /tmp/zero-railway-doctor.json

EVIDENCE_DIR="$(mktemp -d)"
python3 scripts/deployment_evidence.py "${API}" \
  --token railway-intelligence-token \
  --output "${EVIDENCE_DIR}" >/tmp/zero-railway-deployment-evidence.txt
python3 -c 'import json,pathlib,sys; d=pathlib.Path(sys.argv[1]); m=json.loads((d/"manifest.json").read_text(encoding="utf-8")); audit=(d/"audit_export.json").read_text(encoding="utf-8"); assert m["schema_version"] == "zero.deployment_evidence.v1"; assert m["doctor"]["summary"]["fail"] == 0; assert (d/"SHA256SUMS").is_file(); assert "\"trace_id\": \"trace-" not in audit; assert "railway-smoke" not in audit' \
  "${EVIDENCE_DIR}"
python3 scripts/deployment_evidence_verify.py "${EVIDENCE_DIR}" \
  --forbid-token railway-smoke >/tmp/zero-railway-deployment-evidence-verify.txt
ROLLBACK_REHEARSAL_DIR="$(mktemp -d)"
python3 scripts/deployment_rollback_rehearsal.py "${EVIDENCE_DIR}" \
  --previous-bundle "${EVIDENCE_DIR}" \
  --forbid-token railway-smoke \
  --output "${ROLLBACK_REHEARSAL_DIR}" >/tmp/zero-railway-deployment-rollback-rehearsal.txt
python3 -c 'import json,pathlib,sys; d=pathlib.Path(sys.argv[1]); r=json.loads((d/"rollback_rehearsal.json").read_text(encoding="utf-8")); assert r["schema_version"] == "zero.deployment_rollback_rehearsal.v1"; assert r["ok"] is True; assert r["rollback_plan"]["rollback_ready"] is True; assert r["rollback_plan"]["remote_mutation_performed"] is False; assert r["current"]["evidence_verification"]["ok"] is True; assert (d/"SHA256SUMS").is_file()' \
  "${ROLLBACK_REHEARSAL_DIR}"

docker rm -f "${CONTAINER_NAME}" >/dev/null
start_container

curl_retry "${API}/health" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["recovery"]["status"] == "recovered"; assert p["recovery"]["current_positions"] == 1'
curl_retry "${API}/audit/export?limit=5" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.audit.v1"; assert p["operator_context"]["handle"] == "local-operator"; assert p["deployment_claim"]["claim_hash"].startswith("sha256:"); assert p["deployment_heartbeat"]["heartbeat_hash"].startswith("sha256:"); assert p["deployment_heartbeat"]["deployment_claim_hash"] == p["deployment_claim"]["claim_hash"]; assert p["decisions"][0]["trace_id"].startswith("trace-")'
curl_retry "${API}/positions" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["count"] == 1; assert p["positions"][0]["symbol"] == "BTC"'
curl_retry \
  -H "content-type: application/json" \
  -d '{"coin":"BTC","side":"buy","size":0.01,"idempotency_key":"railway-smoke"}' \
  "${API}/execute" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["accepted"] is True; assert p["fill_id"] == "paper-railway-"'
curl_retry "${API}/positions" \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); assert p["count"] == 1; assert p["positions"][0]["size"] == 0.01'
