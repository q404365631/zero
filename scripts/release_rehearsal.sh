#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

mkdir -p \
  "${WORK_DIR}/downloaded/zero-engine-python" \
  "${WORK_DIR}/downloaded/zero-linux" \
  "${WORK_DIR}/downloaded/zero-macos" \
  "${WORK_DIR}/downloaded/zero-paper-image"

printf 'fake wheel for release rehearsal\n' \
  >"${WORK_DIR}/downloaded/zero-engine-python/zero_engine-0.1.2-py3-none-any.whl"
printf 'fake sdist for release rehearsal\n' \
  >"${WORK_DIR}/downloaded/zero-engine-python/zero_engine-0.1.2.tar.gz"
printf '#!/usr/bin/env sh\necho zero linux rehearsal\n' \
  >"${WORK_DIR}/downloaded/zero-linux/zero-linux"
printf '#!/usr/bin/env sh\necho zero macos rehearsal\n' \
  >"${WORK_DIR}/downloaded/zero-macos/zero-macos"
printf 'fake paper image tar for release rehearsal\n' \
  >"${WORK_DIR}/downloaded/zero-paper-image/zero-paper-image.tar"

"${ROOT}/scripts/assemble_release_assets.sh" \
  "${WORK_DIR}/downloaded" \
  "${WORK_DIR}/release-dist"

python3 "${ROOT}/scripts/release_verify.py" "${WORK_DIR}/release-dist" \
  --json >"${WORK_DIR}/release-verify.json"
python3 - "${WORK_DIR}/release-verify.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    report = json.load(handle)
assert report["schema_version"] == "zero.release_verify.v1"
assert report["summary"]["fail"] == 0
PY

printf 'tamper\n' >>"${WORK_DIR}/release-dist/zero-linux"
if python3 "${ROOT}/scripts/release_verify.py" "${WORK_DIR}/release-dist" >/dev/null 2>&1; then
  echo "release verification did not detect a tampered binary" >&2
  exit 1
fi

echo "release rehearsal passed"
