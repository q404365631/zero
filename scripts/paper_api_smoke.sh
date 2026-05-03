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
  ZERO_INTELLIGENCE_API_TOKEN=smoke-intelligence-token \
  ZERO_INTELLIGENCE_API_PLAN=team_fund \
  ZERO_INTELLIGENCE_API_ACCOUNT_ID=acct_smoke \
  ZERO_INTELLIGENCE_WEBHOOK_SIGNING_KEY=smoke-webhook-signing-key \
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
PROFILE_FILE="$(mktemp)"
curl -fsS "${API}/network/profile" >"${PROFILE_FILE}"
"${PYTHON_BIN}" - "${API}" "${PROFILE_FILE}" <<'PY'
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
assert "smoke-1" not in body
assert "trace-" not in body
PY
rm -f "${PROFILE_FILE}"
curl -fsS \
  -H "content-type: application/json" \
  -d '{"consent":false}' \
  "${API}/network/publish" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["ok"] is False; assert p["reason"] == "explicit consent required"'
curl -fsS "${API}/intelligence/snapshot" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.snapshot.v1"; assert p["access"]["class"] == "public_delayed"; assert p["source"]["proof_hash"].startswith("sha256:"); assert p["source"]["deployment_claim_hash"].startswith("sha256:"); assert p["source"]["deployment_heartbeat_hash"].startswith("sha256:"); assert "smoke-1" not in body; assert "trace-" not in body'
curl -fsS "${API}/intelligence/catalog" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.intelligence.catalog.v1"; assert p["public"]["model_gateway_status"]["schema_version"] == "zero.model_gateway.status.v1"; assert p["hosted_api_contract"]["schema_version"] == "zero.intelligence.commercial.v1"; assert "local runtime use" in p["commercial"]["not_metered_by"]; assert "freshness" in p["commercial"]["metered_by"]'
curl -fsS "${API}/intelligence/commercial" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.commercial.v1"; assert p["auth"]["runtime_required"] is False; assert p["plans"][0]["id"] == "free"; assert p["plans"][-1]["id"] == "enterprise"; assert "x-zero-ratelimit-policy" in p["rate_limits"]["headers"]; assert p["privacy"]["exchange_credentials_collected"] is False; assert "smoke-1" not in body; assert "trace-" not in body'
HEADER_FILE="$(mktemp)"
curl -fsS -D "${HEADER_FILE}" "${API}/v1/intelligence/snapshots" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.hosted.snapshots.v1"; assert p["account"]["plan"] == "free"; assert p["access"]["freshness"] == "delayed"; assert p["usage"]["name"] == "snapshot.delayed.read"; assert p["usage"]["billable"] is False; assert "smoke-1" not in body; assert "trace-" not in body; assert "smoke-intelligence-token" not in body'
grep -qi '^x-zero-ratelimit-policy: free;w=3600' "${HEADER_FILE}"
rm -f "${HEADER_FILE}"
"${PYTHON_BIN}" - "${API}" <<'PY'
import json
import urllib.error
import urllib.request
import sys

api = sys.argv[1]
try:
    urllib.request.urlopen(f"{api}/v1/intelligence/history", timeout=5)
except urllib.error.HTTPError as exc:
    assert exc.code == 401
    payload = json.loads(exc.read().decode("utf-8"))
    assert payload["schema_version"] == "zero.intelligence.hosted_error.v1"
    assert payload["error"] == "missing_or_invalid_token"
    assert "smoke-intelligence-token" not in json.dumps(payload)
else:
    raise AssertionError("history endpoint must require a bearer token")
PY
curl -fsS \
  -H "authorization: Bearer smoke-intelligence-token" \
  "${API}/v1/intelligence/history?limit=10" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.hosted.history.v1"; assert p["account"]["id"] == "acct_smoke"; assert p["account"]["plan"] == "team_fund"; assert p["usage"]["name"] == "history.query"; assert p["usage"]["billable"] is True; assert p["storage"]["status"] == "reference_current_runtime_only"; assert "smoke-intelligence-token" not in body'
curl -fsS \
  -H "content-type: application/json" \
  -H "authorization: Bearer smoke-intelligence-token" \
  -d '{"url":"https://example.com/zero","event_types":["snapshot.accepted"]}' \
  "${API}/v1/intelligence/webhooks" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.intelligence.hosted.webhook_subscription.v1"; assert p["signing"]["key_material_included"] is False; assert p["signing"]["fixture_headers"]["x-zero-signature"].startswith("v1="); assert p["fixture_payload"]["schema_version"] == "zero.intelligence.webhook.v1"; assert "smoke-intelligence-token" not in body; assert "smoke-webhook-signing-key" not in body'
curl -fsS \
  -H "content-type: application/json" \
  -H "authorization: Bearer smoke-intelligence-token" \
  -d '{"format":"jsonl","dataset":"verified_behavior_snapshots"}' \
  "${API}/v1/intelligence/exports" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); assert p["schema_version"] == "zero.intelligence.hosted.export.v1"; assert p["export"]["status"] == "reference_ready"; assert p["export"]["raw_private_data"] is False; assert p["usage"]["name"] == "export.created"'
curl -fsS "${API}/intelligence/model-gateway" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.model_gateway.status.v1"; assert p["mode"] == "fail_closed"; assert p["routing"]["structured_output"] is None; assert p["privacy"]["prompts_included"] is False; assert "sk-" not in body; assert "private_key" not in body; assert "smoke-1" not in body; assert "trace-" not in body'
curl -fsS "${API}/intelligence/model-gateway/health" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.model_gateway.health.v1"; assert p["status"] == "failed_closed"; assert p["network_probe"]["requested"] is False; assert p["safety"]["network_probe_requires_explicit_query"] is True; assert "sk-" not in body; assert "private_key" not in body; assert "smoke-1" not in body; assert "trace-" not in body'
curl -fsS "${API}/intelligence/model-gateway/audit" \
  | "${PYTHON_BIN}" -c 'import json,sys; p=json.load(sys.stdin); body=json.dumps(p); assert p["schema_version"] == "zero.model_gateway.audit.v1"; assert p["health"]["schema_version"] == "zero.model_gateway.health.v1"; assert p["controls"]["advisory_only"] is True; assert p["privacy"]["provider_request_ids_included"] is False; assert "sk-" not in body; assert "private_key" not in body; assert "smoke-1" not in body; assert "trace-" not in body'
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

"${PYTHON_BIN}" scripts/railway_doctor.py "${API}" \
  --token smoke-intelligence-token \
  --expect-paper \
  --json >/tmp/zero-paper-api-railway-doctor.json
"${PYTHON_BIN}" -c 'import json,sys; p=json.load(open(sys.argv[1], encoding="utf-8")); assert p["schema_version"] == "zero.railway_doctor.v1"; assert p["summary"]["fail"] == 0' \
  /tmp/zero-paper-api-railway-doctor.json

COCKPIT_DRILL_DIR="$(mktemp -d)"
"${PYTHON_BIN}" scripts/live_cockpit_drill.py "${API}" \
  --output "${COCKPIT_DRILL_DIR}" \
  --forbid-token smoke-1 >/tmp/zero-paper-api-live-cockpit-drill.txt
"${PYTHON_BIN}" -c 'import json,pathlib,sys; d=pathlib.Path(sys.argv[1]); m=json.loads((d/"manifest.json").read_text(encoding="utf-8")); body=json.dumps(m); assert m["schema_version"] == "zero.live_cockpit_drill.v1"; assert m["summary"]["ok"] is True; assert m["summary"]["ready"] is False; assert m["summary"]["risk_increasing_allowed"] is False; assert (d/"SHA256SUMS").is_file(); assert "smoke-1" not in body; assert "trace-" not in body' \
  "${COCKPIT_DRILL_DIR}"

EVIDENCE_DIR="$(mktemp -d)"
"${PYTHON_BIN}" scripts/deployment_evidence.py "${API}" \
  --token smoke-intelligence-token \
  --output "${EVIDENCE_DIR}" >/tmp/zero-paper-api-deployment-evidence.txt
"${PYTHON_BIN}" -c 'import json,pathlib,sys; d=pathlib.Path(sys.argv[1]); m=json.loads((d/"manifest.json").read_text(encoding="utf-8")); audit=(d/"audit_export.json").read_text(encoding="utf-8"); assert m["schema_version"] == "zero.deployment_evidence.v1"; assert m["doctor"]["summary"]["fail"] == 0; assert (d/"SHA256SUMS").is_file(); assert "\"trace_id\": \"trace-" not in audit; assert "smoke-1" not in audit' \
  "${EVIDENCE_DIR}"

CANARY_DIR="$(mktemp -d)"
"${PYTHON_BIN}" scripts/live_canary_rehearsal.py "${API}" \
  --mode refusal \
  --idempotency-key smoke-live-canary-refusal \
  --output "${CANARY_DIR}" >/tmp/zero-paper-api-live-canary-rehearsal.txt
"${PYTHON_BIN}" -c 'import json,pathlib,sys; d=pathlib.Path(sys.argv[1]); m=json.loads((d/"manifest.json").read_text(encoding="utf-8")); e=(d/"91_live_evidence.json").read_text(encoding="utf-8"); assert m["schema_version"] == "zero.live_canary_rehearsal.v1"; assert m["summary"]["live_order_attempted"] is True; assert m["summary"]["live_order_accepted"] is False; assert m["summary"]["live_order_reason"] == "live executor not configured"; assert m["summary"]["evidence_hash"].startswith("sha256:"); assert (d/"SHA256SUMS").is_file(); assert "smoke-live-canary-refusal" not in e; assert "\"trace_id\": \"trace-" not in e' \
  "${CANARY_DIR}"
EXCHANGE_SOURCE="$(mktemp)"
printf '{"orders":[],"fills":[]}\n' >"${EXCHANGE_SOURCE}"
"${PYTHON_BIN}" scripts/live_canary_exchange_evidence.py "${CANARY_DIR}" "${EXCHANGE_SOURCE}" \
  --require-match >/tmp/zero-paper-api-live-canary-exchange-evidence.txt
rm -f "${EXCHANGE_SOURCE}"
"${PYTHON_BIN}" scripts/live_canary_verify.py "${CANARY_DIR}" \
  --require-mode refusal \
  --require-exchange-evidence \
  --forbid-token smoke-live-canary-refusal >/tmp/zero-paper-api-live-canary-verify.txt
OPERATOR_DIR="$(mktemp -d)"
"${PYTHON_BIN}" scripts/live_canary_operator.py "${API}" \
  --mode refusal \
  --idempotency-key smoke-live-canary-operator-refusal \
  --output "${OPERATOR_DIR}" >/tmp/zero-paper-api-live-canary-operator.txt
"${PYTHON_BIN}" -c 'import json,pathlib,sys; d=pathlib.Path(sys.argv[1]); r=json.loads((d/"operator_report.json").read_text(encoding="utf-8")); body=json.dumps(r); assert r["schema_version"] == "zero.live_canary_operator.v1"; assert r["ok"] is True; assert r["summary"]["exchange_evidence_attached"] is True; assert r["summary"]["auto_empty_exchange_export"] is True; assert (d/"bundle"/"exchange_evidence.json").is_file(); assert "smoke-live-canary-operator-refusal" not in body' \
  "${OPERATOR_DIR}"
"${PYTHON_BIN}" scripts/live_canary_operator_verify.py "${OPERATOR_DIR}" \
  --forbid-token smoke-live-canary-operator-refusal >/tmp/zero-paper-api-live-canary-operator-verify.txt

(
  cd "${ROOT}/cli"
  cargo run -q -p zero -- --api "${API}" run positions >/tmp/zero-paper-api-positions.txt
  cargo run -q -p zero -- --api "${API}" run live-cockpit >/tmp/zero-paper-api-live-cockpit.txt
  cargo run -q -p zero -- --api "${API}" run immune >/tmp/zero-paper-api-immune.txt
)

grep -q "BTC" /tmp/zero-paper-api-positions.txt
grep -q "live-cockpit:" /tmp/zero-paper-api-live-cockpit.txt
grep -q "immune:" /tmp/zero-paper-api-immune.txt
